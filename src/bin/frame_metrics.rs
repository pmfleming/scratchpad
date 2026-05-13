#![forbid(unsafe_code)]

use scratchpad::app::capacity_metrics::CapacityMetricsSnapshot;
use scratchpad::profile::{
    MB, RECOMMENDED_UI_RENDER_FRAME_BYTES, RECOMMENDED_UI_RENDER_FRAME_ITERATIONS,
    ui_render_frame_metrics, ui_scroll_frame_metrics,
};
use serde::Serialize;

const UI_RENDER_FRAME_BUDGET_MS: f64 = 8.33;
const UI_RENDER_FRAME_P99_BUDGET_MS: f64 = 12.0;

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
    let mean_ms = ns_to_ms(divide(metrics.frame_time_total_ns, metrics.frame_count));
    let p50_ms = frame_percentile_ms(metrics, 0.50);
    let p95_ms = frame_percentile_ms(metrics, 0.95);
    let p99_ms = frame_percentile_ms(metrics, 0.99);
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
        phases: vec![
            phase(
                "prepare",
                metrics.frame_prepare_total_ns,
                metrics.frame_prepare_max_ns,
                metrics.frame_count,
            ),
            phase(
                "background-poll",
                metrics.frame_background_poll_total_ns,
                metrics.frame_background_poll_max_ns,
                metrics.frame_count,
            ),
            phase(
                "paint",
                metrics.frame_paint_total_ns,
                metrics.frame_paint_max_ns,
                metrics.frame_count,
            ),
            phase(
                "chrome",
                metrics.frame_chrome_total_ns,
                metrics.frame_chrome_max_ns,
                metrics.frame_count,
            ),
            phase(
                "active-surface",
                metrics.frame_active_surface_total_ns,
                metrics.frame_active_surface_max_ns,
                metrics.frame_count,
            ),
            phase(
                "gutter",
                metrics.frame_gutter_total_ns,
                metrics.frame_gutter_max_ns,
                metrics.frame_count,
            ),
            phase(
                "scroll",
                metrics.frame_scroll_total_ns,
                metrics.frame_scroll_max_ns,
                metrics.frame_count,
            ),
            phase(
                "dialogs",
                metrics.frame_dialogs_total_ns,
                metrics.frame_dialogs_max_ns,
                metrics.frame_count,
            ),
            phase(
                "shortcuts",
                metrics.frame_shortcuts_total_ns,
                metrics.frame_shortcuts_max_ns,
                metrics.frame_count,
            ),
            phase(
                "finish",
                metrics.frame_finish_total_ns,
                metrics.frame_finish_max_ns,
                metrics.frame_count,
            ),
        ],
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

fn phase(phase: &'static str, total_ns: u64, max_ns: u64, frame_count: u64) -> FramePhaseMetric {
    FramePhaseMetric {
        phase,
        mean_ms: ns_to_ms(divide(total_ns, frame_count)),
        max_ms: ns_to_ms(max_ns as f64),
    }
}

fn frame_percentile_ms(metrics: &CapacityMetricsSnapshot, percentile: f64) -> f64 {
    if metrics.frame_count == 0 {
        return 0.0;
    }
    let target = ((metrics.frame_count as f64) * percentile).ceil() as u64;
    let mut cumulative = 0;
    for (index, count) in metrics.frame_time_bucket_counts.iter().enumerate() {
        cumulative += count;
        if cumulative >= target {
            let bucket_upper_ns = ((index as u64) + 1) * metrics.frame_time_bucket_width_ns;
            return ns_to_ms(bucket_upper_ns as f64);
        }
    }
    ns_to_ms(metrics.frame_time_max_ns as f64)
}

fn divide(total_ns: u64, count: u64) -> f64 {
    if count == 0 {
        0.0
    } else {
        (total_ns as f64) / (count as f64)
    }
}

fn ns_to_ms(ns: f64) -> f64 {
    ns / 1_000_000.0
}
