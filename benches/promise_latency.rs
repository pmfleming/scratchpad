#![forbid(unsafe_code)]

use criterion::{BatchSize, BenchmarkId, Criterion, criterion_group, criterion_main};
use scratchpad::app::domain::buffer::PieceTreeLite;
use scratchpad::app::domain::{BufferState, PieceSource, TabManager, TextDocument, WorkspaceTab};
use scratchpad::app::services::session_store::SessionStore;
use scratchpad::app::ui::editor_content::native_editor::{
    CharCursor, CursorRange, EditOperation, OperationRecord,
};
use std::hint::black_box;
use std::time::Duration;

const MB: usize = 1024 * 1024;
const TAB_COUNTS: &[usize] = &[1_000, 10_000];
const PASTE_BYTES: usize = 128 * MB;
const SESSION_TABS: usize = 10_000;
const SESSION_BYTES_PER_BUFFER: usize = 4 * 1024;
const SNAPSHOT_SHARED_ADD_BYTES: usize = 16 * MB;
const SNAPSHOT_SHARED_FRAGMENTS: usize = 20_000;
const LARGE_UNDO_BYTES: usize = 16 * MB;
const REVERSE_WALK_CHARS: usize = 8 * 1024;
const DENSE_LINE_TEXT_BYTES: usize = 4 * MB;
const DENSE_LINE_LOOKUPS: usize = 1024;

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

fn bench_snapshot_shared_edit_latency(c: &mut Criterion) {
    let mut group = short_group(c, "snapshot_shared_edit_latency");
    group.bench_function(
        BenchmarkId::from_parameter(SNAPSHOT_SHARED_ADD_BYTES),
        |b| {
            b.iter_batched(
                || {
                    let mut document = TextDocument::new(utf8_text_of_size(MB));
                    let end = document.piece_tree().len_chars();
                    document.insert_direct(end, &utf8_text_of_size(SNAPSHOT_SHARED_ADD_BYTES));
                    let snapshot = document.snapshot();
                    (document, snapshot)
                },
                |(mut document, snapshot)| {
                    let midpoint = document.piece_tree().len_chars() / 2;
                    document.insert_direct(midpoint, "snapshot-shared edit café 東京\n");
                    black_box(snapshot.len_bytes() + document.piece_tree().len_bytes())
                },
                BatchSize::LargeInput,
            );
        },
    );
    group.finish();
}

fn bench_snapshot_shared_fragmented_edit_latency(c: &mut Criterion) {
    let mut base = TextDocument::new(utf8_text_of_size(MB));
    for index in 0..SNAPSHOT_SHARED_FRAGMENTS {
        let len = base.piece_tree().len_chars();
        base.insert_direct((index * 131) % len, "fragment café 東京\n");
    }

    let mut group = short_group(c, "snapshot_shared_fragmented_edit_latency");
    group.bench_function(
        BenchmarkId::from_parameter(SNAPSHOT_SHARED_FRAGMENTS),
        |b| {
            b.iter_batched(
                || {
                    let document = base.clone();
                    let snapshot = document.snapshot();
                    (document, snapshot)
                },
                |(mut document, snapshot)| {
                    let midpoint = document.piece_tree().len_chars() / 2;
                    document.insert_direct(midpoint, "snapshot-shared fragmented edit\n");
                    black_box(snapshot.len_bytes() + document.piece_tree().len_bytes())
                },
                BatchSize::LargeInput,
            );
        },
    );
    group.finish();
}

fn bench_dense_line_lookup_latency(c: &mut Criterion) {
    let tree = PieceTreeLite::from_string("x\n".repeat(DENSE_LINE_TEXT_BYTES / 2));
    let line_count = tree.metrics().newlines + 1;
    let mut group = short_group(c, "dense_line_lookup_latency");
    group.bench_function(BenchmarkId::from_parameter(DENSE_LINE_TEXT_BYTES), |b| {
        b.iter(|| {
            let mut checksum = 0usize;
            for index in 0..DENSE_LINE_LOOKUPS {
                let line = (index * 104_729) % line_count;
                let info = tree.line_info(line);
                checksum = checksum.wrapping_add(info.start_char + info.char_len);
            }
            black_box(checksum)
        });
    });
    group.finish();
}

fn bench_reverse_character_walk_latency(c: &mut Criterion) {
    let text = "界".repeat(REVERSE_WALK_CHARS);
    let tree = PieceTreeLite::from_string(text);
    let mut group = short_group(c, "reverse_character_walk_latency");
    group.bench_function(BenchmarkId::from_parameter(REVERSE_WALK_CHARS), |b| {
        b.iter(|| {
            let mut cursor = tree.char_cursor(tree.len_chars());
            let mut checksum = 0usize;
            while let Some(ch) = cursor.previous_char() {
                checksum = checksum.wrapping_add(ch as usize);
            }
            black_box(checksum)
        });
    });
    group.finish();
}

fn bench_fragmented_tree_edit_latency(c: &mut Criterion) {
    let mut tree = PieceTreeLite::from_string(utf8_text_of_size(MB));
    for index in 0..SNAPSHOT_SHARED_FRAGMENTS {
        let len = tree.len_chars();
        tree.insert_with_source(
            (index * 131) % len,
            "fragment café 東京\n",
            PieceSource::Edit,
        );
    }

    let mut group = short_group(c, "fragmented_tree_edit_latency");
    group.bench_function(
        BenchmarkId::from_parameter(SNAPSHOT_SHARED_FRAGMENTS),
        |b| {
            b.iter(|| {
                let midpoint = tree.len_chars() / 2;
                black_box(tree.insert_with_source(midpoint, "x", PieceSource::Edit));
            });
        },
    );
    group.finish();
}

fn bench_large_paste_undo_latency(c: &mut Criterion) {
    let mut document = TextDocument::new(utf8_text_of_size(MB));
    let inserted = utf8_text_of_size(LARGE_UNDO_BYTES);
    let start = document.piece_tree().len_chars() / 2;
    let end = start + inserted.chars().count();
    document.insert_direct_with_source(start, &inserted, PieceSource::Paste);
    document.push_edit_operation_with_source(
        OperationRecord {
            previous_cursor: CursorRange::one(CharCursor::new(start)),
            next_cursor: CursorRange::one(CharCursor::new(end)),
            edits: vec![EditOperation {
                start_char: start,
                deleted_text: String::new(),
                inserted_text: inserted,
                deleted_spans: Vec::new(),
            }],
        },
        PieceSource::Paste,
    );

    let mut group = short_group(c, "large_paste_undo_latency");
    group.bench_function(BenchmarkId::from_parameter(LARGE_UNDO_BYTES), |b| {
        b.iter_batched(
            || document.clone(),
            |mut candidate| {
                black_box(candidate.undo_last_operation());
                black_box(candidate.piece_tree().len_bytes())
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
    bench_snapshot_shared_edit_latency,
    bench_snapshot_shared_fragmented_edit_latency,
    bench_dense_line_lookup_latency,
    bench_reverse_character_walk_latency,
    bench_fragmented_tree_edit_latency,
    bench_large_paste_undo_latency,
    bench_session_restore_latency,
    bench_session_persist_latency
);
criterion_main!(benches);
