#![forbid(unsafe_code)]

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use scratchpad::app::services::search::{SearchMode, SearchOptions, SearchProgram, search_program};
use std::hint::black_box;

const QUERY: &str = "needle";
const FILE_SIZES: &[usize] = &[16 * 1024, 128 * 1024, 1024 * 1024];
const TARGET_COUNTS: &[usize] = &[1, 8, 32];
const BYTES_PER_TARGET: usize = 16 * 1024;

fn plain_options() -> SearchOptions {
    SearchOptions {
        mode: SearchMode::PlainText,
        match_case: true,
        whole_word: false,
    }
}

fn make_search_text(target_bytes: usize) -> String {
    let line = "alpha beta gamma delta needle epsilon zeta eta theta\n";
    let mut text = String::with_capacity(target_bytes + line.len());
    while text.len() < target_bytes {
        text.push_str(line);
    }
    text.truncate(target_bytes);
    text
}

fn count_matches(text: &str, program: &SearchProgram) -> usize {
    search_program(text, program).matches.len()
}

fn count_matches_across_targets(targets: &[String], program: &SearchProgram) -> usize {
    targets
        .iter()
        .map(|target| count_matches(target, program))
        .sum()
}

fn bench_active_file_size(c: &mut Criterion) {
    let program = SearchProgram::compile(QUERY, plain_options()).expect("valid search query");
    let mut group = c.benchmark_group("search_active_file_size");
    for bytes in FILE_SIZES {
        let text = make_search_text(*bytes);
        group.bench_with_input(BenchmarkId::from_parameter(bytes), bytes, |b, _| {
            b.iter(|| black_box(count_matches(black_box(&text), black_box(&program))));
        });
    }
    group.finish();
}

fn bench_current_files_completion(c: &mut Criterion) {
    bench_aggregate_completion(c, "search_current_files_completion");
}

fn bench_all_tabs_completion(c: &mut Criterion) {
    bench_aggregate_completion(c, "search_all_tabs_completion");
}

fn bench_aggregate_completion(c: &mut Criterion, group_name: &str) {
    let program = SearchProgram::compile(QUERY, plain_options()).expect("valid search query");
    let mut group = c.benchmark_group(group_name);
    for count in TARGET_COUNTS {
        let targets = make_targets(*count);
        group.bench_with_input(BenchmarkId::from_parameter(count), count, |b, _| {
            b.iter(|| {
                black_box(count_matches_across_targets(
                    black_box(&targets),
                    black_box(&program),
                ))
            });
        });
    }
    group.finish();
}

fn bench_current_files_first_response(c: &mut Criterion) {
    bench_first_response(c, "search_current_files_first_response");
}

fn bench_all_tabs_first_response(c: &mut Criterion) {
    bench_first_response(c, "search_all_tabs_first_response");
}

fn bench_first_response(c: &mut Criterion, group_name: &str) {
    let program = SearchProgram::compile(QUERY, plain_options()).expect("valid search query");
    let mut group = c.benchmark_group(group_name);
    for count in TARGET_COUNTS {
        let targets = make_targets(*count);
        group.bench_with_input(BenchmarkId::from_parameter(count), count, |b, _| {
            b.iter(|| {
                let first = targets.first().expect("at least one target");
                black_box(count_matches(black_box(first), black_box(&program)))
            });
        });
    }
    group.finish();
}

fn make_targets(count: usize) -> Vec<String> {
    (0..count)
        .map(|index| {
            let mut text = make_search_text(BYTES_PER_TARGET);
            text.push_str("target ");
            text.push_str(&index.to_string());
            text
        })
        .collect()
}

criterion_group! {
    name = benches;
    config = Criterion::default().sample_size(10);
    targets =
        bench_active_file_size,
        bench_current_files_completion,
        bench_current_files_first_response,
        bench_all_tabs_completion,
        bench_all_tabs_first_response
}
criterion_main!(benches);
