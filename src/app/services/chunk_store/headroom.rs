//! Bounded bulk admission, sampled on producer threads, never on typing/paint.
use super::BULK_PENDING_BYTES;
use serde::Serialize;
use std::time::{Duration, Instant};

const MAX_PENDING: usize = 256 * 1024 * 1024;
#[derive(Clone, Copy)]
pub(super) struct MemorySample {
    pub available: u64,
    pub pressure: bool,
}
impl MemorySample {
    fn read() -> Self {
        let mut system = sysinfo::System::new();
        system.refresh_memory();
        let host = system.available_memory();
        let mut available = if host == 0 { 256 * 1024 * 1024 } else { host };
        // Diagnostic budget simulation, not an OS/cgroup memory restriction.
        if let Some(profile) = std::env::var("SCRATCHPAD_MEMORY_PROFILE")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .filter(|v| *v > 0)
        {
            available = available.min(profile);
        }
        let pressure = std::fs::read_to_string("/proc/pressure/memory")
            .ok()
            .is_some_and(|text| memory_pressure(&text));
        Self {
            available,
            pressure,
        }
    }
    pub(super) fn scan_limits(self) -> (usize, usize) {
        if self.pressure {
            return (0, 0);
        }
        let aggregate = usize::try_from(self.available / 8)
            .unwrap_or(usize::MAX)
            .min(2 * 1024 * 1024 * 1024);
        (aggregate, aggregate.min(1024 * 1024 * 1024))
    }
    pub(super) fn pending_limit(self, failure_limit: usize) -> usize {
        let headroom = if self.pressure {
            BULK_PENDING_BYTES
        } else {
            usize::try_from(self.available / 32)
                .unwrap_or(usize::MAX)
                .clamp(BULK_PENDING_BYTES, MAX_PENDING)
        };
        headroom.min(failure_limit)
    }
}
fn memory_pressure(text: &str) -> bool {
    text.lines()
        .find(|line| line.starts_with("some "))
        .and_then(|line| {
            line.split_whitespace()
                .find_map(|field| field.strip_prefix("avg10="))
        })
        .and_then(|value| value.parse::<f64>().ok())
        .is_some_and(|percent| percent.is_finite() && percent >= 1.0)
}

pub(super) struct Headroom {
    sample: MemorySample,
    refreshed: Instant,
}
impl Headroom {
    pub(super) fn new() -> Self {
        Self {
            sample: MemorySample::read(),
            refreshed: Instant::now(),
        }
    }
    pub(super) fn refresh(&mut self) -> bool {
        if self.refreshed.elapsed() < Duration::from_secs(1) {
            return false;
        }
        self.sample = MemorySample::read();
        self.refreshed = Instant::now();
        true
    }
    pub(super) fn scan_limits(&self) -> (usize, usize) {
        self.sample.scan_limits()
    }
    pub(super) fn observation(&self, failure: usize) -> AdmissionObservation {
        AdmissionObservation {
            pending_flow_threshold_bytes: self.sample.pending_limit(failure),
            policy_available_memory_bytes: self.sample.available,
            policy_failure_limit_bytes: failure,
            policy_memory_pressure: self.sample.pressure,
        }
    }
}
#[derive(Clone, Copy, Serialize)]
pub struct AdmissionObservation {
    pub pending_flow_threshold_bytes: usize,
    pub policy_available_memory_bytes: u64,
    pub policy_failure_limit_bytes: usize,
    pub policy_memory_pressure: bool,
}

#[cfg(test)]
mod tests {
    use super::{MemorySample, memory_pressure};
    #[test]
    fn memory_profiles_pressure_and_failure_budget_bound_admission() {
        for (available, expected) in [
            (512 << 20, 32 << 20),
            (4u64 << 30, 128 << 20),
            (16 << 30, 256 << 20),
            (u64::MAX, 256 << 20),
        ] {
            let sample = MemorySample {
                available,
                pressure: false,
            };
            assert_eq!(sample.pending_limit(1 << 30), expected);
            let (global, owner) = sample.scan_limits();
            assert!(owner <= global && global <= 2 * 1024 * 1024 * 1024);
            assert_eq!(
                MemorySample {
                    pressure: true,
                    ..sample
                }
                .scan_limits(),
                (0, 0)
            );
            assert_eq!(sample.pending_limit(16 << 20), 16 << 20);
            assert_eq!(
                MemorySample {
                    pressure: true,
                    ..sample
                }
                .pending_limit(1 << 30),
                32 << 20
            );
        }
        assert!(memory_pressure(
            "some avg10=1.01 avg60=0.00 total=42\nfull avg10=0.00"
        ));
        assert!(!memory_pressure("some avg10=0.99 avg60=9.00 total=42"));
        assert!(!memory_pressure("full avg10=99.0"));
        assert!(!memory_pressure("some avg10=NaN"));
    }
}
