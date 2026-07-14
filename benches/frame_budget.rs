#![forbid(unsafe_code)]

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use scratchpad::profile::{
    MB, RECOMMENDED_UI_RENDER_FRAME_BYTES, UiRenderFrameHarness, run_scroll_stress_profile,
    run_viewport_extraction_profile,
};
use std::hint::black_box;

fn bench_ui_render_frame_120hz(c: &mut Criterion) {
    let mut group = c.benchmark_group("ui_render_frame_120hz");
    group.sample_size(10);
    let mut harness = UiRenderFrameHarness::new(RECOMMENDED_UI_RENDER_FRAME_BYTES);
    group.bench_function("steady_workspace", |b| {
        b.iter(|| black_box(harness.run_frame()));
    });
    group.finish();
}

fn bench_viewport_extraction(c: &mut Criterion) {
    let mut group = c.benchmark_group("viewport_extraction_latency");
    group.sample_size(10);
    for bytes in [MB, 4 * MB] {
        group.bench_with_input(BenchmarkId::from_parameter(bytes), &bytes, |b, &bytes| {
            b.iter(|| black_box(run_viewport_extraction_profile(bytes, 1)));
        });
    }
    group.finish();
}

fn bench_scroll_stress(c: &mut Criterion) {
    let mut group = c.benchmark_group("scroll_stress_latency");
    group.sample_size(10);
    for bytes in [MB, 4 * MB] {
        group.bench_with_input(BenchmarkId::from_parameter(bytes), &bytes, |b, &bytes| {
            b.iter(|| black_box(run_scroll_stress_profile(bytes, 1)));
        });
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_ui_render_frame_120hz,
    bench_viewport_extraction,
    bench_scroll_stress
);
criterion_main!(benches);
