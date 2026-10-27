//! Process counters sampled outside the operation timer. Unsupported counters are null,
//! not zero. RSS high-water is process-lifetime, not a per-operation sampled peak.
use serde::Serialize;
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

#[derive(Clone, Debug, Default, Serialize)]
pub struct ProcessSnapshot {
    pub working_set_bytes: Option<u64>,
    pub peak_working_set_bytes: Option<u64>,
    pub process_cpu_time_ms: Option<u64>,
    pub read_bytes: Option<u64>,
    pub write_bytes: Option<u64>,
    pub minor_page_faults: Option<u64>,
    pub major_page_faults: Option<u64>,
    pub handle_count: Option<u64>,
}

/// Samples best-effort counters for the current process.
///
/// Unsupported or unreadable counters remain `None`. The working-set peak is a
/// process-lifetime high-water mark, not a peak attributable to one operation.
#[must_use = "sampling only produces a snapshot; retain it for a measurement"]
pub fn process_snapshot() -> ProcessSnapshot {
    let pid = Pid::from_u32(std::process::id());
    let mut system = System::new();
    system.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[pid]),
        true,
        ProcessRefreshKind::nothing()
            .without_tasks()
            .with_memory()
            .with_cpu()
            .with_disk_usage(),
    );
    let mut sample = ProcessSnapshot::default();
    if let Some(process) = system.process(pid) {
        sample.working_set_bytes = Some(process.memory());
        sample.process_cpu_time_ms = Some(process.accumulated_cpu_time());
        sample.read_bytes = Some(process.disk_usage().total_read_bytes);
        sample.write_bytes = Some(process.disk_usage().total_written_bytes);
        sample.handle_count = process.open_files().map(|value| value as u64);
    }
    #[cfg(target_os = "linux")]
    {
        sample.peak_working_set_bytes = std::fs::read_to_string("/proc/self/status")
            .ok()
            .and_then(|text| status_bytes(&text, "VmHWM:"));
        if let Ok(stat) = std::fs::read_to_string("/proc/self/stat") {
            // comm can contain spaces and parentheses; fields after the last ')' begin at state (3).
            if let Some((_, fields)) = stat.rsplit_once(')') {
                let fields: Vec<_> = fields.split_whitespace().collect();
                sample.minor_page_faults = fields.get(7).and_then(|value| value.parse().ok());
                sample.major_page_faults = fields.get(9).and_then(|value| value.parse().ok());
            }
        }
    }
    sample
}

#[cfg(target_os = "linux")]
fn status_bytes(text: &str, name: &str) -> Option<u64> {
    text.lines()
        .find_map(|line| line.strip_prefix(name))?
        .split_whitespace()
        .next()?
        .parse::<u64>()
        .ok()?
        .checked_mul(1024)
}

#[derive(Clone, Debug, Serialize)]
pub struct ProcessMeasurement {
    pub before: ProcessSnapshot,
    pub after: ProcessSnapshot,
    pub cpu_elapsed_ms: Option<u64>,
    pub read_bytes: Option<u64>,
    pub write_bytes: Option<u64>,
    pub minor_page_faults: Option<u64>,
    pub major_page_faults: Option<u64>,
    pub scope: &'static str,
    pub peak_scope: &'static str,
}

impl ProcessMeasurement {
    /// Retains both snapshots and computes nonnegative counter deltas.
    ///
    /// Missing counters and counter decreases produce `None`, not zero. Equal
    /// available counters produce `Some(0)`. Sampling/background overhead remains
    /// included; these boundaries do not isolate one operation's resource use.
    ///
    /// ```
    /// use scratchpad::profile::process_metrics::{ProcessMeasurement, ProcessSnapshot};
    ///
    /// let measurement = ProcessMeasurement::between(
    ///     ProcessSnapshot::default(),
    ///     ProcessSnapshot::default(),
    /// );
    /// assert!(measurement.read_bytes.is_none());
    /// ```
    ///
    /// Discarding the measurement loses the only computed result:
    ///
    /// ```compile_fail
    /// #![deny(unused_must_use)]
    /// use scratchpad::profile::process_metrics::{ProcessMeasurement, ProcessSnapshot};
    ///
    /// ProcessMeasurement::between(ProcessSnapshot::default(), ProcessSnapshot::default());
    /// ```
    #[must_use = "the returned measurement contains the counter deltas"]
    pub fn between(before: ProcessSnapshot, after: ProcessSnapshot) -> Self {
        let delta = |first: Option<u64>, last: Option<u64>| last?.checked_sub(first?);
        Self {
            cpu_elapsed_ms: delta(before.process_cpu_time_ms, after.process_cpu_time_ms),
            read_bytes: delta(before.read_bytes, after.read_bytes),
            write_bytes: delta(before.write_bytes, after.write_bytes),
            minor_page_faults: delta(before.minor_page_faults, after.minor_page_faults),
            major_page_faults: delta(before.major_page_faults, after.major_page_faults),
            before,
            after,
            scope: "process_wide_boundary_counters_including_background_and_sampling_overhead",
            peak_scope: "process_lifetime_high_water_not_operation_peak",
        }
    }
}

#[cfg(test)]
mod tests {
    #[cfg(target_os = "linux")]
    use super::status_bytes;
    use super::{ProcessMeasurement, ProcessSnapshot};
    #[test]
    fn counter_deltas_preserve_zero_and_abstain_for_missing_or_reset_samples() {
        let snapshot = |value| ProcessSnapshot {
            process_cpu_time_ms: value,
            read_bytes: value,
            write_bytes: value,
            minor_page_faults: value,
            major_page_faults: value,
            ..Default::default()
        };
        for (first, last, expected) in [
            (None, None, None),
            (Some(10), None, None),
            (None, Some(12), None),
            (Some(10), Some(8), None),
            (Some(10), Some(10), Some(0)),
            (Some(10), Some(13), Some(3)),
            (Some(u64::MAX), Some(u64::MAX), Some(0)),
            (Some(u64::MAX), Some(0), None),
            (Some(0), Some(u64::MAX), Some(u64::MAX)),
        ] {
            let measured = ProcessMeasurement::between(snapshot(first), snapshot(last));
            for actual in [
                measured.cpu_elapsed_ms,
                measured.read_bytes,
                measured.write_bytes,
                measured.minor_page_faults,
                measured.major_page_faults,
            ] {
                assert_eq!(actual, expected, "counter samples: {first:?} -> {last:?}");
            }
        }
    }
    #[test]
    #[cfg(target_os = "linux")]
    fn proc_rss_units_are_bytes() {
        assert_eq!(status_bytes("VmHWM:\t123 kB\n", "VmHWM:"), Some(123 * 1024));
    }
}
