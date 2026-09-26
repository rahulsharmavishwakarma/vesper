//! Best-effort device pressure samples and bounded preview scheduling.
//! Unknown telemetry is never represented as zero available memory.
use serde::Serialize;
use std::time::Duration;

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Memory {
    pub physical_bytes: Option<u64>,
    pub available_bytes: Option<u64>,
    pub process_rss_bytes: Option<u64>,
}

impl Memory {
    pub fn read() -> Self {
        let mut system = sysinfo::System::new();
        system.refresh_memory();
        let physical = system.total_memory();
        let rss = sysinfo::get_current_pid().ok().and_then(|pid| {
            system.refresh_processes(sysinfo::ProcessesToUpdate::Some(&[pid]), true);
            system.process(pid).map(|process| process.memory())
        });
        Self {
            physical_bytes: (physical > 0).then_some(physical),
            available_bytes: (physical > 0).then_some(system.available_memory()),
            process_rss_bytes: rss,
        }
    }

    pub fn pressured(&self) -> bool {
        self.available_bytes.is_some_and(|available| {
            available
                < self
                    .physical_bytes
                    .unwrap_or(0)
                    .saturating_div(20)
                    .max(256 * 1024 * 1024)
        })
    }
}

pub(crate) fn preview_delay(elapsed: Duration, pressured: bool) -> Duration {
    // Leave the owner idle between passes. Admission/quality never depends on
    // claiming this heuristic is an optimal hardware benchmark result.
    elapsed
        .mul_f64(if pressured { 2.0 } else { 0.5 })
        .clamp(Duration::from_millis(500), Duration::from_secs(5))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unknown_memory_and_slow_decode_have_explicit_bounded_policy() {
        assert!(!Memory::default().pressured());
        assert!(Memory {
            physical_bytes: Some(8 << 30),
            available_bytes: Some(128 << 20),
            ..Default::default()
        }
        .pressured());
        assert_eq!(
            preview_delay(Duration::ZERO, false),
            Duration::from_millis(500)
        );
        assert_eq!(
            preview_delay(Duration::from_secs(30), false),
            Duration::from_secs(5)
        );
        assert!(
            preview_delay(Duration::from_secs(1), true)
                > preview_delay(Duration::from_secs(1), false)
        );
    }
}
