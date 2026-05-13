use super::alloc_metrics::{allocation_snapshot, reset_allocation_counters};
use serde::Serialize;
use std::io::Write;
use std::time::Instant;

const KB: usize = 1024;
const MB: usize = 1024 * KB;
const GB: usize = 1024 * MB;

#[derive(Serialize)]
struct ResourceEvent {
    scenario: &'static str,
    scenario_label: &'static str,
    workload_family: &'static str,
    focus: &'static str,
    step_index: usize,
    workload_value: usize,
    workload_unit: &'static str,
    workload_label: String,
    elapsed_ns: u128,
    allocated_bytes: u64,
    deallocated_bytes: u64,
    live_bytes: u64,
    peak_live_bytes: u64,
    allocation_count: u64,
    deallocation_count: u64,
    reallocation_count: u64,
    result_value: usize,
    result_unit: &'static str,
    result_label: String,
    manifest_size_bytes: Option<u64>,
    status: &'static str,
    note: Option<String>,
}

pub(super) struct StepOutcome {
    pub(super) result_value: usize,
    pub(super) result_unit: &'static str,
    pub(super) result_label: String,
    pub(super) manifest_size_bytes: Option<u64>,
}

pub(super) struct StepDescriptor {
    pub(super) scenario: &'static str,
    pub(super) scenario_label: &'static str,
    pub(super) workload_family: &'static str,
    pub(super) focus: &'static str,
    pub(super) step_index: usize,
    pub(super) workload_value: usize,
    pub(super) workload_unit: &'static str,
    pub(super) workload_label: String,
}

#[derive(Clone, Copy)]
pub(super) struct WorkloadSpec {
    pub(super) scenario: &'static str,
    pub(super) scenario_label: &'static str,
    pub(super) workload_family: &'static str,
    pub(super) focus: &'static str,
    pub(super) workload_unit: &'static str,
}

pub(super) fn emit_workload_steps(
    values: impl IntoIterator<Item = usize>,
    spec: WorkloadSpec,
    run: impl Fn(usize) -> StepOutcome,
) {
    for (step_index, workload_value) in values.into_iter().enumerate() {
        emit_step(spec.descriptor(step_index, workload_value), || {
            run(workload_value)
        });
    }
}

pub(super) fn emit_step(step: StepDescriptor, run: impl FnOnce() -> StepOutcome) {
    reset_allocation_counters();
    let start = Instant::now();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(run));
    let elapsed_ns = start.elapsed().as_nanos();
    let metrics = allocation_snapshot();
    let (status, outcome, note) = match result {
        Ok(outcome) => ("ok", outcome, None),
        Err(payload) => ("panic", StepOutcome::items(0), Some(panic_message(payload))),
    };

    let event = ResourceEvent {
        scenario: step.scenario,
        scenario_label: step.scenario_label,
        workload_family: step.workload_family,
        focus: step.focus,
        step_index: step.step_index,
        workload_value: step.workload_value,
        workload_unit: step.workload_unit,
        workload_label: step.workload_label,
        elapsed_ns,
        allocated_bytes: metrics.allocated_bytes,
        deallocated_bytes: metrics.deallocated_bytes,
        live_bytes: metrics.live_bytes,
        peak_live_bytes: metrics.peak_live_bytes,
        allocation_count: metrics.allocation_count,
        deallocation_count: metrics.deallocation_count,
        reallocation_count: metrics.reallocation_count,
        result_value: outcome.result_value,
        result_unit: outcome.result_unit,
        result_label: outcome.result_label,
        manifest_size_bytes: outcome.manifest_size_bytes,
        status,
        note,
    };

    println!(
        "{}",
        serde_json::to_string(&event).expect("serialize resource event")
    );
    let _ = std::io::stdout().flush();
}

impl StepOutcome {
    pub(super) fn items(value: usize) -> Self {
        Self {
            result_value: value,
            result_unit: "items",
            result_label: format!("{value} items"),
            manifest_size_bytes: None,
        }
    }

    pub(super) fn items_with_manifest(value: usize, manifest_size_bytes: Option<u64>) -> Self {
        Self {
            manifest_size_bytes,
            ..Self::items(value)
        }
    }
}

impl WorkloadSpec {
    fn descriptor(self, step_index: usize, workload_value: usize) -> StepDescriptor {
        StepDescriptor {
            scenario: self.scenario,
            scenario_label: self.scenario_label,
            workload_family: self.workload_family,
            focus: self.focus,
            step_index,
            workload_value,
            workload_unit: self.workload_unit,
            workload_label: workload_label(workload_value, self.workload_unit),
        }
    }
}

fn workload_label(value: usize, unit: &str) -> String {
    match unit {
        "bytes" => human_bytes(value),
        "files" => format!("{value} files"),
        "tabs" => format!("{value} tabs"),
        "views" => format!("{value} views"),
        "pieces" => format!("{value} pieces"),
        "edits" => format!("{value} edits"),
        "anchors" => format!("{value} anchors"),
        "fragments" => format!("{value} fragments"),
        _ => format!("{value} {unit}"),
    }
}

pub(super) fn human_bytes(value: usize) -> String {
    if value >= GB {
        return format!("{:.1} GB", value as f64 / GB as f64);
    }
    if value >= MB {
        return format!("{:.1} MB", value as f64 / MB as f64);
    }
    if value >= KB {
        return format!("{:.0} KB", value as f64 / KB as f64);
    }
    format!("{value} B")
}

pub(super) fn ns_to_ms_label(ns: u128) -> String {
    format!("{:.2}", ns as f64 / 1_000_000.0)
}

fn panic_message(payload: Box<dyn std::any::Any + Send>) -> String {
    if let Some(message) = payload.downcast_ref::<String>() {
        return message.clone();
    }
    if let Some(message) = payload.downcast_ref::<&'static str>() {
        return (*message).to_owned();
    }
    "unknown panic".to_owned()
}
