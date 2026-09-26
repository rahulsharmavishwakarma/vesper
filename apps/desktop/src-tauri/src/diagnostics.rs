//! Bounded, in-memory timings. Observation does not own or synchronize inference.
use serde::Serialize;
use std::{
    sync::{Arc, Mutex},
    time::Instant,
};
use vesper_core::{AppConfig, AsrModel};

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Stage {
    Transcribe,
    Cleanup,
    Save,
}
impl Stage {
    fn index(self) -> usize {
        match self {
            Self::Transcribe => 0,
            Self::Cleanup => 1,
            Self::Save => 2,
        }
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobTimings {
    pub operation_id: String,
    pub asr_model: String,
    pub cleanup_model: Option<String>,
    pub speech_mode: &'static str,
    pub outcome: &'static str,
    pub active_stage: Option<Stage>,
    pub transcribe_ms: Option<f64>,
    pub cleanup_ms: Option<f64>,
    pub save_ms: Option<f64>,
}
struct Job {
    snapshot: JobTimings,
    totals: [Option<f64>; 3],
    active: Option<(Stage, Instant)>,
}
#[derive(Default)]
pub struct Diagnostics {
    job: Mutex<Option<Job>>,
}
impl Diagnostics {
    pub fn begin(&self, id: &str, config: &AppConfig, recording: bool) {
        if let Ok(mut slot) = self.job.lock() {
            *slot = Some(Job {
                snapshot: JobTimings {
                    operation_id: id.into(),
                    asr_model: config.asr_name().into(),
                    cleanup_model: config
                        .cleanup_enabled
                        .then(|| config.rewriter_name().into()),
                    speech_mode: if recording
                        && config.models.custom_asr.is_none()
                        && config.models.asr == AsrModel::Nemotron35AsrStreaming06b
                    {
                        "finalization"
                    } else {
                        "offline"
                    },
                    outcome: "running",
                    active_stage: None,
                    transcribe_ms: None,
                    cleanup_ms: None,
                    save_ms: None,
                },
                totals: [None; 3],
                active: None,
            });
        }
    }
    pub fn mark_offline_fallback(&self, id: &str) {
        if let Ok(mut slot) = self.job.lock() {
            if let Some(job) = slot
                .as_mut()
                .filter(|job| job.snapshot.operation_id == id && job.snapshot.outcome == "running")
            {
                job.snapshot.speech_mode = "fallback";
            }
        }
    }
    pub fn snapshot(&self) -> Option<JobTimings> {
        let slot = self.job.lock().ok()?;
        let job = slot.as_ref()?;
        let mut snapshot = job.snapshot.clone();
        let mut times = job.totals;
        if let Some((stage, start)) = job.active {
            snapshot.active_stage = Some(stage);
            times[stage.index()] = Some(
                times[stage.index()].unwrap_or_default() + start.elapsed().as_secs_f64() * 1000.0,
            );
        }
        [
            snapshot.transcribe_ms,
            snapshot.cleanup_ms,
            snapshot.save_ms,
        ] = times;
        Some(snapshot)
    }
    pub fn finish(&self, outcome: &'static str) {
        if let Ok(mut slot) = self.job.lock() {
            if let Some(job) = slot.as_mut() {
                if job.snapshot.outcome == "running" {
                    job.snapshot.outcome = outcome;
                    if let Some((stage, start)) = job.active.take() {
                        accumulate(job, stage, start);
                    }
                }
            }
        }
    }
    pub fn measure(self: &Arc<Self>, id: &str, stage: Stage) -> Timer {
        let start = Instant::now();
        if let Ok(mut slot) = self.job.lock() {
            if let Some(job) = slot
                .as_mut()
                .filter(|job| job.snapshot.operation_id == id && job.snapshot.outcome == "running")
            {
                job.active = Some((stage, start));
            }
        }
        Timer {
            diagnostics: Arc::clone(self),
            id: id.into(),
            stage,
            start,
        }
    }
}
fn accumulate(job: &mut Job, stage: Stage, start: Instant) {
    job.totals[stage.index()] = Some(
        job.totals[stage.index()].unwrap_or_default() + start.elapsed().as_secs_f64() * 1000.0,
    );
}
pub struct Timer {
    diagnostics: Arc<Diagnostics>,
    id: String,
    stage: Stage,
    start: Instant,
}
impl Drop for Timer {
    fn drop(&mut self) {
        if let Ok(mut slot) = self.diagnostics.job.lock() {
            if let Some(job) = slot.as_mut().filter(|job| {
                job.snapshot.operation_id == self.id && job.snapshot.outcome == "running"
            }) {
                if job.active == Some((self.stage, self.start)) {
                    job.active = None;
                    accumulate(job, self.stage, self.start);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn old_timer_and_finished_work_cannot_overwrite_a_new_operation() {
        let diagnostics = Arc::new(Diagnostics::default());
        let mut config = AppConfig::default();
        config.models.asr = vesper_core::AsrModel::Nemotron35AsrStreaming06b;
        diagnostics.begin("old", &config, false);
        let old = diagnostics.measure("old", Stage::Transcribe);
        diagnostics.begin("new", &config, true);
        diagnostics.mark_offline_fallback("old");
        drop(old);
        let current = diagnostics.snapshot().unwrap();
        assert_eq!(current.operation_id, "new");
        assert_eq!(current.speech_mode, "finalization");
        assert!(current.transcribe_ms.is_none());
        diagnostics.mark_offline_fallback("new");
        assert_eq!(diagnostics.snapshot().unwrap().speech_mode, "fallback");
        let pending = diagnostics.measure("new", Stage::Save);
        diagnostics.finish("cancelled");
        let before = diagnostics.snapshot().unwrap().save_ms;
        drop(pending);
        assert_eq!(diagnostics.snapshot().unwrap().save_ms, before);
        assert_eq!(diagnostics.snapshot().unwrap().outcome, "cancelled");
    }
    #[test]
    fn repeated_storage_work_accumulates_and_unused_stages_stay_unavailable() {
        let diagnostics = Arc::new(Diagnostics::default());
        let config = AppConfig {
            cleanup_enabled: false,
            ..AppConfig::default()
        };
        diagnostics.begin("job", &config, false);
        drop(diagnostics.measure("job", Stage::Save));
        let before = diagnostics.snapshot().unwrap().save_ms.unwrap();
        drop(diagnostics.measure("job", Stage::Save));
        diagnostics.finish("completed");
        let result = diagnostics.snapshot().unwrap();
        assert!(result.save_ms.unwrap() >= before);
        assert!(result.cleanup_model.is_none() && result.cleanup_ms.is_none());
        assert!(result.active_stage.is_none());
    }
}
