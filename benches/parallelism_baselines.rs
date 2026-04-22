use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use scratchpad::profile::{
    KB, MB, run_document_snapshot_profile, run_search_dispatch_all_tabs_profile,
    run_search_dispatch_current_profile, run_viewport_extraction_profile,
};
use std::time::Duration;

const SEARCH_DISPATCH_CURRENT_COUNTS: [usize; 6] = [4, 8, 16, 32, 64, 128];
const SEARCH_DISPATCH_ALL_COUNTS: [usize; 6] = [4, 8, 16, 32, 64, 128];
const SEARCH_DISPATCH_BYTES_PER_ITEM: usize = 24 * KB;

fn bench_document_snapshot_creation_latency(c: &mut Criterion) {
    let mut group = c.benchmark_group("document_snapshot_creation_latency");
    group.sample_size(30);
    group.measurement_time(Duration::from_secs(4));
    for bytes in [256 * KB, MB, 4 * MB] {
        group.throughput(Throughput::Bytes(bytes as u64));
        group.bench_with_input(BenchmarkId::from_parameter(bytes), &bytes, |b, &bytes| {
            b.iter(|| criterion::black_box(run_document_snapshot_profile(bytes, 1)));
        });
    }
    group.finish();
}

fn bench_viewport_extraction_latency(c: &mut Criterion) {
    let mut group = c.benchmark_group("viewport_extraction_latency");
    group.sample_size(30);
    group.measurement_time(Duration::from_secs(4));
    for bytes in [256 * KB, MB, 4 * MB] {
        group.throughput(Throughput::Bytes(bytes as u64));
        group.bench_with_input(BenchmarkId::from_parameter(bytes), &bytes, |b, &bytes| {
            b.iter(|| criterion::black_box(run_viewport_extraction_profile(bytes, 1)));
        });
    }
    group.finish();
}

fn bench_search_current_dispatch_aggregate_size(c: &mut Criterion) {
    let mut group = c.benchmark_group("search_current_dispatch_aggregate_size");
    group.sample_size(20);
    group.measurement_time(Duration::from_secs(4));
    for file_count in SEARCH_DISPATCH_CURRENT_COUNTS {
        let aggregate_bytes = file_count * SEARCH_DISPATCH_BYTES_PER_ITEM;
        group.throughput(Throughput::Bytes(aggregate_bytes as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(file_count),
            &file_count,
            |b, &file_count| {
                b.iter(|| {
                    criterion::black_box(run_search_dispatch_current_profile(
                        file_count,
                        SEARCH_DISPATCH_BYTES_PER_ITEM,
                        1,
                    ))
                });
            },
        );
    }
    group.finish();
}

fn bench_search_all_dispatch_aggregate_size(c: &mut Criterion) {
    let mut group = c.benchmark_group("search_all_dispatch_aggregate_size");
    group.sample_size(20);
    group.measurement_time(Duration::from_secs(4));
    for tab_count in SEARCH_DISPATCH_ALL_COUNTS {
        let aggregate_bytes = tab_count * SEARCH_DISPATCH_BYTES_PER_ITEM;
        group.throughput(Throughput::Bytes(aggregate_bytes as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(tab_count),
            &tab_count,
            |b, &tab_count| {
                b.iter(|| {
                    criterion::black_box(run_search_dispatch_all_tabs_profile(
                        tab_count,
                        SEARCH_DISPATCH_BYTES_PER_ITEM,
                        1,
                    ))
                });
            },
        );
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_document_snapshot_creation_latency,
    bench_viewport_extraction_latency,
    bench_search_current_dispatch_aggregate_size,
    bench_search_all_dispatch_aggregate_size
);
criterion_main!(benches);
