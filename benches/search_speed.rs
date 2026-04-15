use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use scratchpad::app::services::search::SearchOptions;
use std::ops::Range;
use std::time::Duration;

const KB: usize = 1024;
const MB: usize = 1024 * KB;
const CURRENT_FILE_SIZE_FIXED_ITEMS: usize = 8;
const ALL_FILE_SIZE_FIXED_ITEMS: usize = 8;
const CURRENT_AGGREGATE_BYTES_PER_FILE: usize = 24 * KB;
const ALL_AGGREGATE_BYTES_PER_FILE: usize = 16 * KB;
const RESPONSE_MATCH_LIMIT: usize = 40;

fn corpus_text_of_size(item_index: usize, target_bytes: usize) -> String {
    let line = format!(
        "item {item_index} needle alpha beta gamma {}\n",
        "x".repeat(48)
    );
    let repeats = (target_bytes / line.len()).max(1);
    let mut text = String::with_capacity(repeats * line.len());
    for _ in 0..repeats {
        text.push_str(&line);
    }
    text
}

fn build_scope_texts(item_count: usize, bytes_per_item: usize) -> Vec<String> {
    (0..item_count)
        .map(|item_index| corpus_text_of_size(item_index, bytes_per_item))
        .collect()
}

fn full_scan_scope(texts: &[String], query: &str, options: SearchOptions) -> usize {
    texts
        .iter()
        .map(|text| find_matches(text, query, options).len())
        .sum()
}

fn first_response_scope(
    texts: &[String],
    query: &str,
    options: SearchOptions,
    match_limit: usize,
) -> usize {
    let mut total = 0;
    for text in texts {
        total += find_matches_until_limit(text, query, options, match_limit - total).len();
        if total >= match_limit {
            break;
        }
    }
    total
}

fn find_matches(text: &str, query: &str, options: SearchOptions) -> Vec<Range<usize>> {
    find_matches_until_limit(text, query, options, usize::MAX)
}

fn find_matches_until_limit(
    text: &str,
    query: &str,
    options: SearchOptions,
    max_results: usize,
) -> Vec<Range<usize>> {
    if query.is_empty() || max_results == 0 {
        return Vec::new();
    }

    let query_char_len = query.chars().count();
    let text_char_len = text.chars().count();
    if query_char_len > text_char_len {
        return Vec::new();
    }

    let char_to_byte = char_to_byte_map(text);
    let folded_query = (!options.match_case).then(|| query.to_lowercase());
    let text_chars = text.chars().collect::<Vec<_>>();
    let mut matches = Vec::new();

    for start in 0..=text_char_len - query_char_len {
        let end = start + query_char_len;
        let candidate = &text[char_to_byte[start]..char_to_byte[end]];
        if !candidate_matches(
            candidate,
            query,
            folded_query.as_deref(),
            options.match_case,
        ) {
            continue;
        }
        if options.whole_word && !is_whole_word_match(&text_chars, start, end) {
            continue;
        }
        matches.push(start..end);
        if matches.len() >= max_results {
            break;
        }
    }

    matches
}

fn candidate_matches(
    candidate: &str,
    query: &str,
    folded_query: Option<&str>,
    match_case: bool,
) -> bool {
    if match_case {
        candidate == query
    } else {
        candidate.to_lowercase() == folded_query.unwrap_or_default()
    }
}

fn char_to_byte_map(text: &str) -> Vec<usize> {
    let mut offsets = text
        .char_indices()
        .map(|(offset, _)| offset)
        .collect::<Vec<_>>();
    offsets.push(text.len());
    offsets
}

fn is_whole_word_match(text_chars: &[char], start: usize, end: usize) -> bool {
    let before_is_word = start > 0 && is_word_char(text_chars[start - 1]);
    let after_is_word = end < text_chars.len() && is_word_char(text_chars[end]);
    !before_is_word && !after_is_word
}

fn is_word_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

fn bench_active_completion_file_size(c: &mut Criterion) {
    let mut group = c.benchmark_group("search_active_completion_file_size");
    for bytes in [64 * KB, 256 * KB, MB] {
        let text = corpus_text_of_size(0, bytes);
        group.throughput(Throughput::Bytes(bytes as u64));
        group.bench_with_input(BenchmarkId::from_parameter(bytes), &text, |b, text| {
            b.iter(|| {
                criterion::black_box(full_scan_scope(
                    std::slice::from_ref(text),
                    "needle",
                    SearchOptions::default(),
                ))
            });
        });
    }
    group.finish();
}

fn bench_active_first_response_file_size(c: &mut Criterion) {
    let mut group = c.benchmark_group("search_active_first_response_file_size");
    for bytes in [64 * KB, 256 * KB, MB] {
        let text = corpus_text_of_size(0, bytes);
        group.throughput(Throughput::Bytes(bytes as u64));
        group.bench_with_input(BenchmarkId::from_parameter(bytes), &text, |b, text| {
            b.iter(|| {
                criterion::black_box(first_response_scope(
                    std::slice::from_ref(text),
                    "needle",
                    SearchOptions::default(),
                    RESPONSE_MATCH_LIMIT,
                ))
            });
        });
    }
    group.finish();
}

fn bench_current_completion_file_size(c: &mut Criterion) {
    let mut group = c.benchmark_group("search_current_completion_file_size");
    group.sample_size(30);
    group.measurement_time(Duration::from_secs(4));
    for bytes in [8 * KB, 32 * KB, 128 * KB] {
        let texts = build_scope_texts(CURRENT_FILE_SIZE_FIXED_ITEMS, bytes);
        let aggregate_bytes = CURRENT_FILE_SIZE_FIXED_ITEMS * bytes;
        group.throughput(Throughput::Bytes(aggregate_bytes as u64));
        group.bench_with_input(BenchmarkId::from_parameter(bytes), &texts, |b, texts| {
            b.iter(|| {
                criterion::black_box(full_scan_scope(texts, "needle", SearchOptions::default()))
            });
        });
    }
    group.finish();
}

fn bench_current_first_response_file_size(c: &mut Criterion) {
    let mut group = c.benchmark_group("search_current_first_response_file_size");
    group.sample_size(30);
    group.measurement_time(Duration::from_secs(4));
    for bytes in [8 * KB, 32 * KB, 128 * KB] {
        let texts = build_scope_texts(CURRENT_FILE_SIZE_FIXED_ITEMS, bytes);
        let aggregate_bytes = CURRENT_FILE_SIZE_FIXED_ITEMS * bytes;
        group.throughput(Throughput::Bytes(aggregate_bytes as u64));
        group.bench_with_input(BenchmarkId::from_parameter(bytes), &texts, |b, texts| {
            b.iter(|| {
                criterion::black_box(first_response_scope(
                    texts,
                    "needle",
                    SearchOptions::default(),
                    RESPONSE_MATCH_LIMIT,
                ))
            });
        });
    }
    group.finish();
}

fn bench_current_completion_aggregate_size(c: &mut Criterion) {
    let mut group = c.benchmark_group("search_current_completion_aggregate_size");
    group.sample_size(30);
    group.measurement_time(Duration::from_secs(4));
    for file_count in [4usize, 16, 32] {
        let texts = build_scope_texts(file_count, CURRENT_AGGREGATE_BYTES_PER_FILE);
        let aggregate_bytes = file_count * CURRENT_AGGREGATE_BYTES_PER_FILE;
        group.throughput(Throughput::Bytes(aggregate_bytes as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(file_count),
            &texts,
            |b, texts| {
                b.iter(|| {
                    criterion::black_box(full_scan_scope(texts, "needle", SearchOptions::default()))
                });
            },
        );
    }
    group.finish();
}

fn bench_current_first_response_aggregate_size(c: &mut Criterion) {
    let mut group = c.benchmark_group("search_current_first_response_aggregate_size");
    group.sample_size(30);
    group.measurement_time(Duration::from_secs(4));
    for file_count in [4usize, 16, 32] {
        let texts = build_scope_texts(file_count, CURRENT_AGGREGATE_BYTES_PER_FILE);
        let aggregate_bytes = file_count * CURRENT_AGGREGATE_BYTES_PER_FILE;
        group.throughput(Throughput::Bytes(aggregate_bytes as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(file_count),
            &texts,
            |b, texts| {
                b.iter(|| {
                    criterion::black_box(first_response_scope(
                        texts,
                        "needle",
                        SearchOptions::default(),
                        RESPONSE_MATCH_LIMIT,
                    ))
                });
            },
        );
    }
    group.finish();
}

fn bench_all_completion_file_size(c: &mut Criterion) {
    let mut group = c.benchmark_group("search_all_completion_file_size");
    group.sample_size(30);
    group.measurement_time(Duration::from_secs(4));
    for bytes in [8 * KB, 32 * KB, 128 * KB] {
        let texts = build_scope_texts(ALL_FILE_SIZE_FIXED_ITEMS, bytes);
        let aggregate_bytes = ALL_FILE_SIZE_FIXED_ITEMS * bytes;
        group.throughput(Throughput::Bytes(aggregate_bytes as u64));
        group.bench_with_input(BenchmarkId::from_parameter(bytes), &texts, |b, texts| {
            b.iter(|| {
                criterion::black_box(full_scan_scope(texts, "needle", SearchOptions::default()))
            });
        });
    }
    group.finish();
}

fn bench_all_first_response_file_size(c: &mut Criterion) {
    let mut group = c.benchmark_group("search_all_first_response_file_size");
    group.sample_size(30);
    group.measurement_time(Duration::from_secs(4));
    for bytes in [8 * KB, 32 * KB, 128 * KB] {
        let texts = build_scope_texts(ALL_FILE_SIZE_FIXED_ITEMS, bytes);
        let aggregate_bytes = ALL_FILE_SIZE_FIXED_ITEMS * bytes;
        group.throughput(Throughput::Bytes(aggregate_bytes as u64));
        group.bench_with_input(BenchmarkId::from_parameter(bytes), &texts, |b, texts| {
            b.iter(|| {
                criterion::black_box(first_response_scope(
                    texts,
                    "needle",
                    SearchOptions::default(),
                    RESPONSE_MATCH_LIMIT,
                ))
            });
        });
    }
    group.finish();
}

fn bench_all_completion_aggregate_size(c: &mut Criterion) {
    let mut group = c.benchmark_group("search_all_completion_aggregate_size");
    group.sample_size(20);
    group.measurement_time(Duration::from_secs(4));
    for tab_count in [8usize, 32, 128] {
        let texts = build_scope_texts(tab_count, ALL_AGGREGATE_BYTES_PER_FILE);
        let aggregate_bytes = tab_count * ALL_AGGREGATE_BYTES_PER_FILE;
        group.throughput(Throughput::Bytes(aggregate_bytes as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(tab_count),
            &texts,
            |b, texts| {
                b.iter(|| {
                    criterion::black_box(full_scan_scope(texts, "needle", SearchOptions::default()))
                });
            },
        );
    }
    group.finish();
}

fn bench_all_first_response_aggregate_size(c: &mut Criterion) {
    let mut group = c.benchmark_group("search_all_first_response_aggregate_size");
    group.sample_size(20);
    group.measurement_time(Duration::from_secs(4));
    for tab_count in [8usize, 32, 128] {
        let texts = build_scope_texts(tab_count, ALL_AGGREGATE_BYTES_PER_FILE);
        let aggregate_bytes = tab_count * ALL_AGGREGATE_BYTES_PER_FILE;
        group.throughput(Throughput::Bytes(aggregate_bytes as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(tab_count),
            &texts,
            |b, texts| {
                b.iter(|| {
                    criterion::black_box(first_response_scope(
                        texts,
                        "needle",
                        SearchOptions::default(),
                        RESPONSE_MATCH_LIMIT,
                    ))
                });
            },
        );
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_active_completion_file_size,
    bench_active_first_response_file_size,
    bench_current_completion_file_size,
    bench_current_first_response_file_size,
    bench_current_completion_aggregate_size,
    bench_current_first_response_aggregate_size,
    bench_all_completion_file_size,
    bench_all_first_response_file_size,
    bench_all_completion_aggregate_size,
    bench_all_first_response_aggregate_size
);
criterion_main!(benches);
