//! Conditions for deferrable background work and the worker thread's policy.
//! Unlike the window and accessibility entries these are thread-safe and may be
//! called from any thread; none prompts or needs a permission.

/// `ProcessInfo.thermalState`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ThermalState {
    Nominal,
    Fair,
    Serious,
    Critical,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SystemConditions {
    pub thermal: ThermalState,
    pub low_power_mode: bool,
    /// `None` when the power source is unknown (treated as battery by callers).
    pub on_ac_power: Option<bool>,
}

#[cfg(target_os = "macos")]
mod native {
    unsafe extern "C" {
        pub fn td_thermal_state() -> i32;
        pub fn td_low_power_mode() -> bool;
        pub fn td_on_ac_power() -> i32;
        pub fn td_background_thread() -> bool;
        pub fn td_thread_cpu_ns() -> u64;
    }
}

pub fn system_conditions() -> SystemConditions {
    #[cfg(target_os = "macos")]
    {
        // SAFETY: Scalar returns only; the native functions read process and
        // power-source state without retaining anything past the call.
        let (thermal, low_power, ac) = unsafe {
            (
                native::td_thermal_state(),
                native::td_low_power_mode(),
                native::td_on_ac_power(),
            )
        };
        SystemConditions {
            thermal: match thermal {
                0 => ThermalState::Nominal,
                1 => ThermalState::Fair,
                2 => ThermalState::Serious,
                _ => ThermalState::Critical,
            },
            low_power_mode: low_power,
            on_ac_power: (ac >= 0).then_some(ac == 1),
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        SystemConditions {
            thermal: ThermalState::Nominal,
            low_power_mode: false,
            on_ac_power: None,
        }
    }
}

/// Put the calling thread at background QoS with throttled disk I/O. Returns
/// whether both policies were applied.
pub fn enter_background_thread() -> bool {
    #[cfg(target_os = "macos")]
    // SAFETY: Changes only the calling thread's scheduling and I/O policy.
    unsafe {
        native::td_background_thread()
    }
    #[cfg(not(target_os = "macos"))]
    false
}

/// CPU time used by the calling thread.
pub fn thread_cpu_time() -> std::time::Duration {
    #[cfg(target_os = "macos")]
    // SAFETY: Reads the calling thread's CPU clock.
    let nanos = unsafe { native::td_thread_cpu_ns() };
    #[cfg(not(target_os = "macos"))]
    let nanos = 0;
    std::time::Duration::from_nanos(nanos)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conditions_and_thread_policy_are_readable() {
        let conditions = system_conditions();
        assert!(conditions.thermal <= ThermalState::Critical);
        let handle = std::thread::spawn(|| {
            let applied = enter_background_thread();
            let start = thread_cpu_time();
            let mut x = 0u64;
            for i in 0..2_000_000u64 {
                x = x.wrapping_add(i * i);
            }
            std::hint::black_box(x);
            (applied, thread_cpu_time() > start)
        });
        let (applied, advanced) = handle.join().unwrap();
        if cfg!(target_os = "macos") {
            assert!(applied && advanced);
        }
    }
}
