#![forbid(unsafe_code)]

use scratchpad::app::capacity_metrics::{CapacityMetricsSnapshot, FramePhase};
use scratchpad::profile::{
    MB, RECOMMENDED_UI_RENDER_FRAME_BYTES, RECOMMENDED_UI_RENDER_FRAME_ITERATIONS,
    ui_render_frame_metrics, ui_scroll_frame_metrics,
};
use serde::Serialize;

const UI_RENDER_FRAME_BUDGET_MS: f64 = 8.33;
const UI_RENDER_FRAME_P99_BUDGET_MS: f64 = 12.0;
const FRAME_PHASES: [(FramePhase, &str); 10] = [
    (FramePhase::Prepare, "prepare"),
    (FramePhase::BackgroundPoll, "background-poll"),
    (FramePhase::Paint, "paint"),
    (FramePhase::Chrome, "chrome"),
    (FramePhase::ActiveSurface, "active-surface"),
    (FramePhase::Gutter, "gutter"),
    (FramePhase::Scroll, "scroll"),
    (FramePhase::Dialogs, "dialogs"),
    (FramePhase::Shortcuts, "shortcuts"),
    (FramePhase::Finish, "finish"),
];

#[derive(Serialize)]
struct FrameMetricsReport {
    meta: FrameMetricsMeta,
    scenarios: Vec<FrameScenario>,
}

#[derive(Serialize)]
struct FrameMetricsMeta {
    generated_from: &'static str,
    bytes: usize,
    iterations: usize,
}

#[derive(Serialize)]
struct FrameScenario {
    scenario_id: String,
    scenario_label: String,
    workload_family: &'static str,
    budget_ms: f64,
    p99_budget_ms: f64,
    frame_count: u64,
    mean_ms: f64,
    p50_ms: f64,
    p95_ms: f64,
    p99_ms: f64,
    max_ms: f64,
    over_budget: bool,
    phases: Vec<FramePhaseMetric>,
}

#[derive(Serialize)]
struct FramePhaseMetric {
    phase: &'static str,
    mean_ms: f64,
    max_ms: f64,
}

fn main() {
    let bytes = RECOMMENDED_UI_RENDER_FRAME_BYTES;
    let iterations = RECOMMENDED_UI_RENDER_FRAME_ITERATIONS;
    let scenarios = vec![
        frame_scenario(
            &ui_render_frame_metrics(bytes, iterations),
            "ui_render_frame_120hz",
            bytes,
            "steady-repaint",
        ),
        frame_scenario(
            &ui_scroll_frame_metrics(MB, iterations),
            "editor_scroll_frame_120hz/1048576",
            MB,
            "wheel-scroll",
        ),
        frame_scenario(
            &ui_scroll_frame_metrics(4 * MB, iterations),
            "editor_scroll_frame_120hz/4194304",
            4 * MB,
            "wheel-scroll",
        ),
    ];
    let report = FrameMetricsReport {
        meta: FrameMetricsMeta {
            generated_from: "src/bin/frame_metrics.rs",
            bytes,
            iterations,
        },
        scenarios,
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&report).expect("serialize frame metrics")
    );
}

fn frame_scenario(
    metrics: &CapacityMetricsSnapshot,
    scenario_id: &str,
    bytes: usize,
    workload_family: &'static str,
) -> FrameScenario {
    let mean_ms = ns_to_ms(metrics.frame_time_mean_ns());
    let p50_ms = ns_to_ms(metrics.frame_time_percentile_ns(0.50));
    let p95_ms = ns_to_ms(metrics.frame_time_percentile_ns(0.95));
    let p99_ms = ns_to_ms(metrics.frame_time_percentile_ns(0.99));
    FrameScenario {
        scenario_id: scenario_id.to_owned(),
        scenario_label: format!("Editor frame 120 Hz ({})", human_bytes(bytes)),
        workload_family,
        budget_ms: UI_RENDER_FRAME_BUDGET_MS,
        p99_budget_ms: UI_RENDER_FRAME_P99_BUDGET_MS,
        frame_count: metrics.frame_count,
        mean_ms,
        p50_ms,
        p95_ms,
        p99_ms,
        max_ms: ns_to_ms(metrics.frame_time_max_ns as f64),
        over_budget: p95_ms > UI_RENDER_FRAME_BUDGET_MS || p99_ms > UI_RENDER_FRAME_P99_BUDGET_MS,
        phases: FRAME_PHASES
            .iter()
            .map(|&(phase, label)| frame_phase_metric(metrics, phase, label))
            .collect(),
    }
}

fn human_bytes(bytes: usize) -> String {
    let mib = bytes as f64 / MB as f64;
    if mib >= 1.0 {
        format!("{mib:.0} MiB")
    } else {
        format!("{} KiB", bytes / 1024)
    }
}

fn frame_phase_metric(
    metrics: &CapacityMetricsSnapshot,
    phase: FramePhase,
    label: &'static str,
) -> FramePhaseMetric {
    let phase_metrics = metrics.frame_phase(phase);
    FramePhaseMetric {
        phase: label,
        mean_ms: ns_to_ms(if metrics.frame_count == 0 {
            0.0
        } else {
            phase_metrics.total_ns as f64 / metrics.frame_count as f64
        }),
        max_ms: ns_to_ms(phase_metrics.max_ns as f64),
    }
}

fn ns_to_ms(ns: f64) -> f64 {
    ns / 1_000_000.0
}
