//! One lazy worker and one replaceable pending policy job. No model ownership.
use super::*;
use std::sync::mpsc::{sync_channel, SyncSender};
use transcribe_data::PolicyShadowJob;
type Job = (PathBuf, PolicyShadowJob, Arc<AtomicBool>);
#[derive(Default)]
pub(super) struct PolicyWorker(Mutex<Option<Worker>>);
struct Worker {
    pending: Arc<Mutex<Option<Job>>>,
    wake: Option<SyncSender<()>>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl PolicyWorker {
    pub(super) fn submit(&self, job: Job) {
        let Ok(mut worker) = self.0.lock() else {
            return;
        };
        if worker.is_none() {
            let (send, receive) = sync_channel(1);
            let pending = Arc::new(Mutex::new(None::<Job>));
            let jobs = Arc::clone(&pending);
            let thread = std::thread::Builder::new()
                .name("policy-shadow".into())
                .spawn(move || {
                    while receive.recv().is_ok() {
                        let job = jobs.lock().ok().and_then(|mut p| p.take());
                        if let Some((root, job, flag)) = job {
                            if !flag.load(Ordering::Acquire) {
                                if let Ok(store) = DatasetStore::open_shadow_completion(root) {
                                    let _ = store.finish_policy_shadow(job, &flag);
                                }
                            }
                        }
                    }
                });
            let Ok(thread) = thread else {
                return;
            };
            *worker = Some(Worker {
                pending,
                wake: Some(send),
                thread: Some(thread),
            });
        }
        let Some(worker) = worker.as_ref() else {
            return;
        };
        if let Ok(mut pending) = worker.pending.lock() {
            if let Some((_, _, flag)) = pending.replace(job) {
                flag.store(true, Ordering::Release);
            }
            if let Some(wake) = &worker.wake {
                let _ = wake.try_send(());
            }
        };
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        if let Ok(mut pending) = self.pending.lock() {
            if let Some((_, _, flag)) = pending.take() {
                flag.store(true, Ordering::Release);
            }
        }
        self.wake.take();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};
    #[test]
    fn ordinary_edit_reaches_shadow_worker_and_forget_prevents_late_recreation() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let store = DatasetStore::open(root.clone()).unwrap();
        let profile = store.dictation_personalization().unwrap();
        let runtime = Runtime::default();
        runtime.publish(PipelineResult {
            result_id: "worker".into(),
            raw_transcript: "read me".into(),
            cleaned_transcript: None,
            speaker_timeline: None,
            personalized_transcript: None,
            personalization_evidence: None,
            draft_transcript: None,
            draft_revision: 0,
            learning_outcome: None,
            learning_context: transcribe_core::LearningContext {
                result_id: "worker".into(),
                source_key: "authored".into(),
                episode_id: "worker".into(),
                learning_epoch: profile.learning_epoch,
                learning_enabled: true,
                language: Language::English,
                ..Default::default()
            },
            session: None,
            warnings: vec![],
            duration_seconds: 1.0,
        });
        runtime
            .edit_latest_draft(root.clone(), "worker", 0, "README".into())
            .unwrap();
        let start = Instant::now();
        while store.policy_shadow_state().unwrap().version == 0 {
            assert!(
                start.elapsed() < Duration::from_secs(3),
                "worker did not persist"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(store.list_sessions().unwrap().is_empty());
        runtime
            .edit_latest_draft(root, "worker", 1, "README file".into())
            .unwrap();
        store
            .forget_automatic_learning(store.personalization().unwrap().revision)
            .unwrap();
        runtime.clear_learning_context();
        drop(runtime);
        assert_eq!(store.policy_shadow_state().unwrap().version, 0);
    }
}
