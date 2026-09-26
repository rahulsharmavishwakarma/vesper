use super::*;
use vesper_core::{SemanticAnalysisReport, SemanticAnalysisRequest, SemanticStatus};

pub(super) struct SemanticJob {
    request: SemanticAnalysisRequest,
    config: AppConfig,
    root: PathBuf,
    cancelled: Arc<AtomicBool>,
    completed: Box<dyn FnOnce(SemanticAnalysisReport) + Send>,
}

#[derive(Default)]
pub(super) struct SemanticQueue {
    pending: Option<SemanticJob>,
    running: Option<Arc<AtomicBool>>,
    submitted: u64,
    coalesced: u64,
    cancelled: u64,
    finished: u64,
}

impl SemanticQueue {
    pub(super) fn has_pending(&self) -> bool {
        self.pending.is_some()
    }
    fn cancel(&mut self) {
        for flag in self
            .pending
            .as_ref()
            .map(|j| &j.cancelled)
            .into_iter()
            .chain(self.running.as_ref())
        {
            if !flag.swap(true, Ordering::AcqRel) {
                self.cancelled += 1;
            }
        }
    }
    fn replace(&mut self, job: SemanticJob) {
        self.cancel();
        self.submitted += 1;
        if self.pending.replace(job).is_some() {
            self.coalesced += 1;
        }
    }
    fn take(&mut self) -> Option<SemanticJob> {
        let job = self.pending.take()?;
        self.running = Some(Arc::clone(&job.cancelled));
        Some(job)
    }
}

impl InferenceManager {
    /// Nonblocking, one pending job. The caller owns idle/capture eligibility and
    /// must set this request's cancellation flag when that eligibility changes.
    pub fn submit_semantic_shadow(
        &self,
        request: SemanticAnalysisRequest,
        config: AppConfig,
        root: PathBuf,
        cancelled: Arc<AtomicBool>,
        completed: impl FnOnce(SemanticAnalysisReport) + Send + 'static,
    ) -> Result<()> {
        request
            .validate()
            .map_err(|e| native("semantic request", e))?;
        self.semantic
            .lock()
            .map_err(worker_unavailable)?
            .replace(SemanticJob {
                request,
                config,
                root,
                cancelled,
                completed: Box::new(completed),
            });
        // A full queue already wakes the worker; no background caller waits for it.
        match self.sender.try_send(Command::WakeSemantic) {
            Ok(()) | Err(mpsc::TrySendError::Full(_)) => Ok(()),
            Err(mpsc::TrySendError::Disconnected(_)) => {
                Err(native("semantic queue", "worker unavailable"))
            }
        }
    }

    pub fn cancel_semantic(&self) {
        if let Ok(mut queue) = self.semantic.lock() {
            queue.cancel();
        }
    }

    pub fn semantic_queue_evidence(&self) -> serde_json::Value {
        self.semantic.lock().map(|q| serde_json::json!({"submitted":q.submitted,"coalesced":q.coalesced,"cancelled":q.cancelled,"finished":q.finished,"pending":q.pending.is_some(),"running":q.running.is_some()})).unwrap_or_default()
    }
}

pub(super) fn run_pending(state: &WorkerState, queue: &Mutex<SemanticQueue>) {
    let Some(job) = queue.lock().ok().and_then(|mut q| q.take()) else {
        return;
    };
    let mut report = if job.cancelled.load(Ordering::Acquire) {
        SemanticAnalysisReport::empty(SemanticStatus::Cancelled)
    } else if !job.config.cleanup_enabled
        || job.config.models.custom_rewriter.is_some()
        || state.rewriter.is_none()
    {
        SemanticAnalysisReport::empty(SemanticStatus::Unsupported)
    } else {
        match (qwen::key(&job.config, &job.root), &state.rewriter) {
            (Ok(key), Some((loaded, adapters::TextModel::Llama(model)))) if key == *loaded => {
                model.analyze_semantic(&job.request, &job.config, &job.cancelled)
            }
            _ => SemanticAnalysisReport::empty(SemanticStatus::Unsupported),
        }
    };
    if job.cancelled.load(Ordering::Acquire) {
        report.status = SemanticStatus::Cancelled;
        report.proposal = None;
    }
    if let Ok(mut q) = queue.lock() {
        q.running = None;
        q.finished += 1;
    }
    (job.completed)(report);
}

#[cfg(test)]
mod tests {
    use super::*;
    fn job(id: &str) -> SemanticJob {
        SemanticJob {
            request: SemanticAnalysisRequest {
                schema_version: 1,
                origin: vesper_core::SemanticOrigin::AuthoredEvaluation,
                session_id: None,
                context: vesper_core::LearningContext {
                    result_id: id.into(),
                    source_key: "s".into(),
                    episode_id: "e".into(),
                    learning_enabled: true,
                    ..Default::default()
                },
                result_revision: 1,
                before: "read me".into(),
                after: "README".into(),
                earlier: None,
                memories: vec![],
            },
            config: AppConfig::default(),
            root: PathBuf::new(),
            cancelled: Arc::new(AtomicBool::new(false)),
            completed: Box::new(|_| {}),
        }
    }
    #[test]
    fn queue_coalesces_and_cancels_running_work_without_waiting() {
        let mut q = SemanticQueue::default();
        q.replace(job("a"));
        let running = q.take().unwrap();
        q.replace(job("b"));
        assert!(running.cancelled.load(Ordering::Acquire));
        q.replace(job("c"));
        assert_eq!(q.coalesced, 1);
        q.cancel();
        let pending = q.take().unwrap();
        assert_eq!(pending.request.context.result_id, "c");
        assert!(pending.cancelled.load(Ordering::Acquire));
    }
    #[test]
    fn background_work_never_loads_a_missing_model() {
        let manager = InferenceManager::new().unwrap();
        let job = job("a");
        let (tx, rx) = mpsc::channel();
        manager
            .submit_semantic_shadow(job.request, job.config, job.root, job.cancelled, move |r| {
                let _ = tx.send(r);
            })
            .unwrap();
        let report = rx.recv_timeout(std::time::Duration::from_secs(2)).unwrap();
        assert_eq!(report.status, SemanticStatus::Unsupported);
        assert!(report.proposal.is_none());
        assert_eq!(manager.latest_execution_evidence(), serde_json::json!({}));
    }
}
