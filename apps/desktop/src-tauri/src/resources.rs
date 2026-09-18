//! On-demand current-process telemetry. No background thread or model inspection.
use serde::Serialize;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use sysinfo::{CpuRefreshKind, Pid, ProcessRefreshKind, ProcessesToUpdate, System};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceSample {
    pub sampled_at_ms: u64,
    pub cpu_percent: Option<f64>,
    pub memory_bytes: Option<u64>,
    pub logical_cpus: usize,
    pub status: &'static str,
}

#[derive(Default)]
struct CpuBaseline(Option<(u64, Instant)>);
impl CpuBaseline {
    fn observe(&mut self, cpu_ms: u64, now: Instant, cpus: usize) -> Option<f64> {
        let previous = self.0.replace((cpu_ms, now));
        let (before, at) = previous?;
        let elapsed = now.checked_duration_since(at)?;
        if cpus == 0 || elapsed < Duration::from_millis(200) || elapsed > Duration::from_secs(5) {
            return None;
        }
        let delta = cpu_ms.checked_sub(before)?;
        Some((delta as f64 / elapsed.as_secs_f64() / 10.0 / cpus as f64).clamp(0.0, 100.0))
    }
}

pub struct ResourceSampler {
    system: System,
    pid: Pid,
    baseline: CpuBaseline,
    last: Option<(Instant, ResourceSample)>,
}
impl Default for ResourceSampler {
    fn default() -> Self {
        Self {
            system: System::new(),
            pid: Pid::from_u32(std::process::id()),
            baseline: CpuBaseline::default(),
            last: None,
        }
    }
}
impl ResourceSampler {
    pub fn sample(&mut self) -> ResourceSample {
        let now = Instant::now();
        if let Some((at, sample)) = &self.last {
            if now.duration_since(*at) < Duration::from_millis(900) {
                return sample.clone();
            }
        }
        if self.system.cpus().is_empty() {
            self.system.refresh_cpu_list(CpuRefreshKind::nothing());
        }
        self.system.refresh_processes_specifics(
            ProcessesToUpdate::Some(&[self.pid]),
            true,
            ProcessRefreshKind::nothing()
                .with_cpu()
                .with_memory()
                .without_tasks(),
        );
        let sampled = Instant::now();
        let mut result = ResourceSample {
            sampled_at_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
            cpu_percent: None,
            memory_bytes: None,
            logical_cpus: self.system.cpus().len(),
            status: "unavailable",
        };
        if let Some(process) = self.system.process(self.pid) {
            // RSS=0 also occurs when the OS refuses the memory read. Do not present it as measured zero.
            result.memory_bytes = (process.memory() > 0).then(|| process.memory());
            result.cpu_percent =
                self.baseline
                    .observe(process.accumulated_cpu_time(), sampled, result.logical_cpus);
            result.status = if result.memory_bytes.is_none() {
                "unavailable"
            } else if result.cpu_percent.is_none() {
                "warming"
            } else {
                "ready"
            };
            if result.memory_bytes.is_none() {
                result.cpu_percent = None;
                self.baseline.0 = None;
            }
        } else {
            self.baseline.0 = None;
        }
        self.last = Some((sampled, result.clone()));
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn interval_cpu_normalizes_capacity_and_rebaselines_after_pause_or_reset() {
        let mut baseline = CpuBaseline::default();
        let now = Instant::now();
        assert_eq!(baseline.observe(100, now, 8), None);
        assert_eq!(
            baseline.observe(2100, now + Duration::from_secs(1), 8),
            Some(25.0)
        );
        assert_eq!(
            baseline.observe(2100, now + Duration::from_secs(2), 8),
            Some(0.0)
        );
        assert_eq!(
            baseline.observe(2200, now + Duration::from_secs(10), 8),
            None
        );
        assert_eq!(baseline.observe(1, now + Duration::from_secs(11), 8), None);
        assert_eq!(baseline.observe(2, now + Duration::from_secs(12), 0), None);
    }
    #[test]
    fn rapid_reads_reuse_one_sample_without_inventing_initial_cpu() {
        let mut sampler = ResourceSampler::default();
        let first = sampler.sample();
        assert!(first.cpu_percent.is_none());
        assert_eq!(sampler.sample().sampled_at_ms, first.sampled_at_ms);
    }
    #[test]
    #[ignore = "explicit OS metrics/overhead observation; no recording or model loading"]
    fn measure_process_resources() {
        let mut sampler = ResourceSampler::default();
        println!("pid={}", std::process::id());
        for _ in 0..6 {
            let start = Instant::now();
            let sample = sampler.sample();
            println!(
                "{}",
                serde_json::json!({"sample":sample,"samplingMicros":start.elapsed().as_micros()})
            );
            std::thread::sleep(Duration::from_secs(1));
        }
    }
}
