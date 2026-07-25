#![forbid(unsafe_code)]

use criterion::{BatchSize, BenchmarkId, Criterion, criterion_group, criterion_main};
use scratchpad::app::domain::{BufferState, TabManager, WorkspaceTab};
use scratchpad::app::services::session_store::SessionStore;
use std::hint::black_box;
use std::time::Duration;

const MB: usize = 1024 * 1024;
const TAB_COUNTS: &[usize] = &[1_000, 10_000];
const PASTE_BYTES: usize = 128 * MB;
const SESSION_TABS: usize = 10_000;
const SESSION_BYTES_PER_BUFFER: usize = 4 * 1024;

fn bench_tab_reorder_latency(c: &mut Criterion) {
    let mut group = c.benchmark_group("tab_reorder_latency");
    group.sample_size(10);
    group.warm_up_time(Duration::from_secs(1));
    group.measurement_time(Duration::from_secs(2));

    for &tab_count in TAB_COUNTS {
        let mut manager = tab_manager(tab_count);
        let last_index = tab_count - 1;
        group.bench_with_input(
            BenchmarkId::from_parameter(tab_count),
            &tab_count,
            |b, _| {
                b.iter(|| {
                    black_box(manager.reorder_tab(1, last_index));
                    black_box(manager.reorder_tab(last_index, 1));
                });
            },
        );
    }
    group.finish();
}

fn bench_paste_stress_latency(c: &mut Criterion) {
    let mut group = c.benchmark_group("paste_stress_latency");
    group.sample_size(10);
    group.warm_up_time(Duration::from_secs(1));
    group.measurement_time(Duration::from_secs(2));

    group.bench_with_input(
        BenchmarkId::from_parameter(PASTE_BYTES),
        &PASTE_BYTES,
        |b, &insert_bytes| {
            b.iter_batched(
                || {
                    (
                        BufferState::new(
                            "paste_benchmark.txt".to_owned(),
                            utf8_text_of_size(MB),
                            None,
                        ),
                        utf8_text_of_size(insert_bytes),
                    )
                },
                |(mut buffer, inserted)| {
                    let midpoint = buffer.document().piece_tree().len_chars() / 2;
                    buffer.document_mut().insert_direct(midpoint, &inserted);
                    buffer.refresh_text_metadata();
                    black_box(buffer.line_count + buffer.document().piece_tree().len_bytes())
                },
                BatchSize::LargeInput,
            );
        },
    );
    group.finish();

    bench_paste_insert_phase(c);
    bench_paste_metadata_phase(c);
}

fn bench_paste_insert_phase(c: &mut Criterion) {
    let mut group = short_group(c, "paste_insert_phase");
    group.bench_function(BenchmarkId::from_parameter(PASTE_BYTES), |b| {
        b.iter_batched(
            || {
                (
                    BufferState::new(
                        "paste_insert_phase.txt".to_owned(),
                        utf8_text_of_size(MB),
                        None,
                    ),
                    utf8_text_of_size(PASTE_BYTES),
                )
            },
            |(mut buffer, inserted)| {
                let midpoint = buffer.document().piece_tree().len_chars() / 2;
                buffer.document_mut().insert_direct(midpoint, &inserted);
                black_box(buffer.document().piece_tree().len_bytes())
            },
            BatchSize::LargeInput,
        );
    });
    group.finish();
}

fn bench_paste_metadata_phase(c: &mut Criterion) {
    let mut group = short_group(c, "paste_metadata_phase");
    group.bench_function(BenchmarkId::from_parameter(PASTE_BYTES), |b| {
        b.iter_batched(
            || {
                let mut buffer = BufferState::new(
                    "paste_metadata_phase.txt".to_owned(),
                    utf8_text_of_size(MB),
                    None,
                );
                let inserted = utf8_text_of_size(PASTE_BYTES);
                let midpoint = buffer.document().piece_tree().len_chars() / 2;
                buffer.document_mut().insert_direct(midpoint, &inserted);
                buffer
            },
            |mut buffer| {
                buffer.refresh_text_metadata();
                black_box(buffer.line_count)
            },
            BatchSize::LargeInput,
        );
    });
    group.finish();
}

fn bench_session_restore_latency(c: &mut Criterion) {
    let fixture = session_fixture(SESSION_TABS);

    let mut startup_group = short_group(c, "session_restore_latency");
    startup_group.bench_function(BenchmarkId::from_parameter(SESSION_TABS), |b| {
        b.iter(|| {
            let restored = fixture
                .store
                .load_startup_visible()
                .expect("load startup-visible session")
                .expect("session exists");
            black_box(restored.tabs.len())
        });
    });
    startup_group.finish();

    let mut completion_group = short_group(c, "session_restore_background_completion");
    completion_group.bench_function(BenchmarkId::from_parameter(SESSION_TABS), |b| {
        b.iter(|| {
            let restored = fixture
                .store
                .load_profiled()
                .expect("load complete session");
            black_box(restored.profile.tab_count)
        });
    });
    completion_group.finish();
}

fn bench_session_persist_latency(c: &mut Criterion) {
    let fixture = session_fixture(SESSION_TABS);
    let mut group = short_group(c, "session_persist_latency");
    group.bench_function(BenchmarkId::from_parameter(SESSION_TABS), |b| {
        b.iter(|| {
            let profile = fixture
                .store
                .persist_profiled(&fixture.tabs, 0, 14.0, true)
                .expect("persist session");
            black_box(profile.tab_count)
        });
    });
    group.finish();
}

struct SessionFixture {
    _directory: tempfile::TempDir,
    store: SessionStore,
    tabs: Vec<WorkspaceTab>,
}

fn session_fixture(tab_count: usize) -> SessionFixture {
    let directory = tempfile::tempdir().expect("create session benchmark directory");
    let store = SessionStore::new(directory.path().join("session"));
    let tabs = (0..tab_count)
        .map(|index| {
            WorkspaceTab::new(BufferState::new(
                format!("session_{index}.txt"),
                utf8_text_of_size(SESSION_BYTES_PER_BUFFER),
                None,
            ))
        })
        .collect::<Vec<_>>();
    store
        .persist(&tabs, 0, 14.0, true)
        .expect("prepare session benchmark fixture");
    SessionFixture {
        _directory: directory,
        store,
        tabs,
    }
}

fn short_group<'a>(
    c: &'a mut Criterion,
    name: &str,
) -> criterion::BenchmarkGroup<'a, criterion::measurement::WallTime> {
    let mut group = c.benchmark_group(name);
    group.sample_size(10);
    group.warm_up_time(Duration::from_secs(1));
    group.measurement_time(Duration::from_secs(2));
    group
}

fn tab_manager(tab_count: usize) -> TabManager {
    let tabs = (0..tab_count)
        .map(|index| {
            WorkspaceTab::new(BufferState::new(
                format!("tab_{index}.txt"),
                String::new(),
                None,
            ))
        })
        .collect();
    let mut manager = TabManager::new();
    manager.set_tabs(tabs, 0);
    manager
}

fn utf8_text_of_size(target_bytes: usize) -> String {
    const UNIT: &str = "Scratchpad edits UTF-8: café 東京 Привет مرحبا.\n";
    let repeats = target_bytes.div_ceil(UNIT.len());
    let mut text = UNIT.repeat(repeats);
    while !text.is_char_boundary(target_bytes.min(text.len())) {
        text.pop();
    }
    text.truncate(target_bytes.min(text.len()));
    text
}

criterion_group!(
    benches,
    bench_tab_reorder_latency,
    bench_paste_stress_latency,
    bench_session_restore_latency,
    bench_session_persist_latency
);
criterion_main!(benches);
