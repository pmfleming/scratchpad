use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use scratchpad::ScratchpadApp;
use scratchpad::app::app_state::SearchScope;
use scratchpad::app::domain::{BufferState, SplitAxis};
use scratchpad::app::services::search::SearchOptions;
use scratchpad::app::services::session_store::SessionStore;
use std::ops::Range;
use std::thread;
use std::time::Duration;
use std::time::Instant;

const KB: usize = 1024;
const MB: usize = 1024 * KB;
const CURRENT_FILE_SIZE_FIXED_ITEMS: usize = 8;
const ALL_FILE_SIZE_FIXED_ITEMS: usize = 8;
const CURRENT_AGGREGATE_BYTES_PER_FILE: usize = 24 * KB;
const ALL_AGGREGATE_BYTES_PER_FILE: usize = 16 * KB;
const APP_STATE_AGGREGATE_BYTES_PER_FILE: usize = 24 * KB;
const CURRENT_AGGREGATE_FILE_COUNTS: [usize; 7] = [4, 8, 16, 32, 64, 128, 256];
const ALL_AGGREGATE_TAB_COUNTS: [usize; 6] = [8, 16, 32, 64, 128, 256];
const RESPONSE_MATCH_LIMIT: usize = 40;
const APP_STATE_QUERY: &str = "needle";
const APP_STATE_RESET_QUERY: &str = "zzzz-no-match";

struct AppStateSearchBench {
    app: ScratchpadApp,
    expected_matches: usize,
}

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

fn buffer_name_for_index(item_index: usize) -> String {
    if item_index.is_multiple_of(2) {
        "mod.rs".to_owned()
    } else {
        "lib.rs".to_owned()
    }
}

fn build_app_state_search_bench(file_count: usize, bytes_per_item: usize) -> AppStateSearchBench {
    let session_root = tempfile::tempdir().expect("create session dir");
    let session_store = SessionStore::new(session_root.path().to_path_buf());
    let mut app = ScratchpadApp::with_session_store(session_store);
    let texts = build_scope_texts(file_count, bytes_per_item);
    let expected_matches = full_scan_scope(&texts, APP_STATE_QUERY, SearchOptions::default());

    app.tabs_mut()[0].buffer.name = buffer_name_for_index(0);
    app.tabs_mut()[0].buffer.replace_text(texts[0].clone());
    let first_view_id = app.tabs()[0].active_view_id;

    for (item_index, text) in texts.iter().enumerate().skip(1) {
        if item_index.is_multiple_of(2) {
            app.tabs_mut()[0].activate_view(first_view_id);
        }

        app.tabs_mut()[0]
            .open_buffer_as_split(
                BufferState::new(buffer_name_for_index(item_index), text.clone(), None),
                if item_index.is_multiple_of(2) {
                    SplitAxis::Horizontal
                } else {
                    SplitAxis::Vertical
                },
                false,
                0.5,
            )
            .expect("open split buffer");
    }

    app.open_search();
    app.set_search_scope(SearchScope::ActiveWorkspaceTab);
    app.set_search_query(APP_STATE_RESET_QUERY);
    wait_for_app_state_search_matches(&mut app, 0);

    AppStateSearchBench {
        app,
        expected_matches,
    }
}

fn wait_for_app_state_search_matches(app: &mut ScratchpadApp, expected: usize) {
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        app.poll_search();
        if app.search_match_count() == expected {
            return;
        }
        thread::yield_now();
    }

    panic!(
        "timed out waiting for {expected} search matches; got {}",
        app.search_match_count()
    );
}

fn run_app_state_search_iteration(bench: &mut AppStateSearchBench) -> usize {
    bench.app.set_search_query(APP_STATE_QUERY);
    wait_for_app_state_search_matches(&mut bench.app, bench.expected_matches);
    let matches = bench.app.search_match_count();
    bench.app.set_search_query(APP_STATE_RESET_QUERY);
    wait_for_app_state_search_matches(&mut bench.app, 0);
    matches
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
    for file_count in CURRENT_AGGREGATE_FILE_COUNTS {
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
    for file_count in CURRENT_AGGREGATE_FILE_COUNTS {
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

fn bench_current_app_state_completion_aggregate_size(c: &mut Criterion) {
    let mut group = c.benchmark_group("search_current_app_state_completion_aggregate_size");
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(4));
    for file_count in CURRENT_AGGREGATE_FILE_COUNTS {
        let aggregate_bytes = file_count * APP_STATE_AGGREGATE_BYTES_PER_FILE;
        let mut bench =
            build_app_state_search_bench(file_count, APP_STATE_AGGREGATE_BYTES_PER_FILE);
        group.throughput(Throughput::Bytes(aggregate_bytes as u64));
        group.bench_function(BenchmarkId::from_parameter(file_count), |b| {
            b.iter(|| criterion::black_box(run_app_state_search_iteration(&mut bench)));
        });
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
    for tab_count in ALL_AGGREGATE_TAB_COUNTS {
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
    for tab_count in ALL_AGGREGATE_TAB_COUNTS {
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
    bench_current_app_state_completion_aggregate_size,
    bench_all_completion_file_size,
    bench_all_first_response_file_size,
    bench_all_completion_aggregate_size,
    bench_all_first_response_aggregate_size
);
criterion_main!(benches);
