#![forbid(unsafe_code)]

use scratchpad::profile::{MB, UiRenderFrameHarness};
use serde::Serialize;
use std::hint::black_box;

const WARMUP_FRAMES: usize = 30;
const MEASURED_FRAMES: usize = 240;
const FRAME_BUDGET_MS: f64 = 8.33;
const P99_BUDGET_MS: f64 = 12.0;

#[derive(Serialize)]
struct Report {
    meta: Meta,
    scenarios: Vec<Scenario>,
}

#[derive(Serialize)]
struct Meta {
    generated_from: &'static str,
    renderer_scope: &'static str,
    measured_frames: usize,
    warmup_frames: usize,
}

#[derive(Serialize)]
struct Scenario {
    scenario_id: &'static str,
    scenario_label: &'static str,
    workload_family: &'static str,
    measurement_scope: &'static str,
    metric_role: &'static str,
    present_included: bool,
    vsync: bool,
    budget_ms: f64,
    p99_budget_ms: f64,
    frame_count: usize,
    mean_ms: f64,
    p50_ms: f64,
    p95_ms: f64,
    p99_ms: f64,
    max_ms: f64,
    over_budget: bool,
    mean_primitive_count: f64,
    mean_vertex_count: f64,
    included_work: Vec<&'static str>,
    omitted_work: Vec<&'static str>,
    phases: Vec<serde_json::Value>,
}

fn main() {
    let mut harness = UiRenderFrameHarness::new(4 * MB);
    for _ in 0..WARMUP_FRAMES {
        black_box(harness.run_event_to_tessellation_scroll_frame());
    }

    let mut elapsed_ns = Vec::with_capacity(MEASURED_FRAMES);
    let mut primitive_count = 0usize;
    let mut vertex_count = 0usize;
    for _ in 0..MEASURED_FRAMES {
        let sample = harness.run_event_to_tessellation_scroll_frame();
        elapsed_ns.push(sample.elapsed_ns);
        primitive_count += sample.primitive_count;
        vertex_count += sample.vertex_count;
        black_box(sample);
    }
    elapsed_ns.sort_unstable();

    let mean_ms = ns_to_ms(elapsed_ns.iter().sum::<u128>() as f64 / elapsed_ns.len() as f64);
    let p50_ms = ns_to_ms(percentile(&elapsed_ns, 0.50) as f64);
    let p95_ms = ns_to_ms(percentile(&elapsed_ns, 0.95) as f64);
    let p99_ms = ns_to_ms(percentile(&elapsed_ns, 0.99) as f64);
    let max_ms = ns_to_ms(*elapsed_ns.last().unwrap_or(&0) as f64);
    let report = Report {
        meta: Meta {
            generated_from: "src/bin/realistic_frame_metrics.rs",
            renderer_scope: "egui event/update/layout/paint/tessellation; no GPU or present",
            measured_frames: MEASURED_FRAMES,
            warmup_frames: WARMUP_FRAMES,
        },
        scenarios: vec![Scenario {
            scenario_id: "editor_scroll_event_to_tessellation/4194304",
            scenario_label: "Editor scroll event to tessellated render data (4 MiB)",
            workload_family: "wheel-scroll",
            measurement_scope: "end_to_end_event_to_tessellation",
            metric_role: "render_preparation_latency",
            present_included: false,
            vsync: false,
            budget_ms: FRAME_BUDGET_MS,
            p99_budget_ms: P99_BUDGET_MS,
            frame_count: elapsed_ns.len(),
            mean_ms,
            p50_ms,
            p95_ms,
            p99_ms,
            max_ms,
            over_budget: p95_ms > FRAME_BUDGET_MS || p99_ms > P99_BUDGET_MS,
            mean_primitive_count: primitive_count as f64 / elapsed_ns.len() as f64,
            mean_vertex_count: vertex_count as f64 / elapsed_ns.len() as f64,
            included_work: vec![
                "wheel and pointer event construction",
                "Scratchpad app update",
                "viewport extraction and editor layout",
                "egui paint command generation",
                "egui shape tessellation into render primitives",
            ],
            omitted_work: vec![
                "GPU buffer upload and command submission",
                "swap-chain acquisition",
                "OS compositor",
                "display present callback",
                "vsync pacing",
            ],
            phases: Vec::new(),
        }],
    };

    println!(
        "{}",
        serde_json::to_string_pretty(&report).expect("serialize realistic frame report")
    );
}

fn percentile(sorted: &[u128], percentile: f64) -> u128 {
    if sorted.is_empty() {
        return 0;
    }
    let rank = (percentile * sorted.len() as f64).ceil() as usize;
    sorted[rank.saturating_sub(1).min(sorted.len() - 1)]
}

fn ns_to_ms(ns: f64) -> f64 {
    ns / 1_000_000.0
}
