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

pub fn process_snapshot() -> ProcessSnapshot {
    let pid = Pid::from_u32(std::process::id());
    let mut system = System::new();
    system.refresh_processes_specifics(ProcessesToUpdate::Some(&[pid]), true,
        ProcessRefreshKind::nothing().without_tasks().with_memory().with_cpu().with_disk_usage());
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
        sample.peak_working_set_bytes = std::fs::read_to_string("/proc/self/status").ok()
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
    text.lines().find_map(|line| line.strip_prefix(name))?
        .split_whitespace().next()?.parse::<u64>().ok()?.checked_mul(1024)
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
    pub fn between(before: ProcessSnapshot, after: ProcessSnapshot) -> Self {
        let delta = |first: Option<u64>, last: Option<u64>| last?.checked_sub(first?);
        Self {
            cpu_elapsed_ms: delta(before.process_cpu_time_ms, after.process_cpu_time_ms),
            read_bytes: delta(before.read_bytes, after.read_bytes),
            write_bytes: delta(before.write_bytes, after.write_bytes),
            minor_page_faults: delta(before.minor_page_faults, after.minor_page_faults),
            major_page_faults: delta(before.major_page_faults, after.major_page_faults),
            before, after,
            scope: "process_wide_boundary_counters_including_background_and_sampling_overhead",
            peak_scope: "process_lifetime_high_water_not_operation_peak",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unavailable_and_reset_counters_do_not_become_zero() {
        let a = ProcessSnapshot { read_bytes:Some(10), ..Default::default() };
        let b = ProcessSnapshot { read_bytes:Some(8), ..Default::default() };
        let measured = ProcessMeasurement::between(a,b);
        assert_eq!(measured.cpu_elapsed_ms, None);
        assert_eq!(measured.read_bytes, None);
    }
    #[test]
    #[cfg(target_os = "linux")]
    fn proc_rss_units_are_bytes() { assert_eq!(status_bytes("VmHWM:\t123 kB\n", "VmHWM:"), Some(123*1024)); }
}
