#[path = "resource_probe/alloc_metrics.rs"]
mod alloc_metrics;

use alloc_metrics::{allocation_snapshot, reset_allocation_counters};
use scratchpad::app::domain::buffer::PieceTreeLite;
use scratchpad::app::domain::{
    AnchorBias, AnchorOwner, BufferState, PieceSource, SearchHighlightState, SplitAxis,
    TextDocument, TextHistoryBudget, WorkspaceTab,
};
use scratchpad::app::services::file_service::FileService;
use scratchpad::app::services::search::{SearchMode, SearchOptions, SearchProgram, search_program};
use scratchpad::app::services::session_store::SessionStore;
use scratchpad::app::ui::editor_content::native_editor::{
    CharCursor, CursorRange, EditOperation, OperationRecord,
};
use scratchpad::app::ui::editor_content::{EditorHighlightStyle, build_layouter};
use serde::Serialize;
use std::hint::black_box;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Instant;

const KB: usize = 1024;
const MB: usize = 1024 * KB;
const GB: usize = 1024 * MB;
const TAB_BYTES_PER_BUFFER: usize = 48 * KB;
const MANY_FILE_BYTES_PER_BUFFER: usize = KB;
const SESSION_BYTES_PER_BUFFER: usize = 4 * KB;
const VIEW_COUNT_BUFFER_BYTES: usize = MB;
const PASTE_RESOURCE_BASE_BYTES: usize = MB;
const FIRST_VISIBLE_PAINT_MAX_CHARS: usize = 192 * KB;
const PREVIEW_LIMIT: usize = 10_000;
const UTF8_SAMPLE_LINE: &str =
    "Scratchpad edits UTF-8: café résumé 東京 Привет مرحبا 0123456789.\n";
const UTF8_SEARCH_UNIT: &str = "hay café 東京 Привет مرحبا hay\n";
const FRAGMENT_UNIT: &str = "hay needle café 東京\n";

#[derive(Serialize)]
struct ResourceEvent {
    scenario: &'static str,
    scenario_label: &'static str,
    workload_family: &'static str,
    focus: &'static str,
    step_index: usize,
    workload_value: usize,
    workload_unit: &'static str,
    workload_label: String,
    elapsed_ns: u128,
    allocated_bytes: u64,
    deallocated_bytes: u64,
    live_bytes: u64,
    peak_live_bytes: u64,
    allocation_count: u64,
    deallocation_count: u64,
    reallocation_count: u64,
    result_value: usize,
    result_unit: &'static str,
    result_label: String,
    manifest_size_bytes: Option<u64>,
    status: &'static str,
    note: Option<String>,
}

struct StepOutcome {
    result_value: usize,
    result_unit: &'static str,
    result_label: String,
    manifest_size_bytes: Option<u64>,
}

struct StepDescriptor {
    scenario: &'static str,
    scenario_label: &'static str,
    workload_family: &'static str,
    focus: &'static str,
    step_index: usize,
    workload_value: usize,
    workload_unit: &'static str,
    workload_label: String,
}

#[derive(Clone, Copy)]
struct WorkloadSpec {
    scenario: &'static str,
    scenario_label: &'static str,
    workload_family: &'static str,
    focus: &'static str,
    workload_unit: &'static str,
}

fn main() {
    emit_large_utf8_load_peak_memory();
    emit_file_backed_open_allocations();
    emit_edited_buffer_search_preview_rendering();
    emit_provenance_retained_memory();
    emit_anchor_heavy_view_editing();
    emit_fragmented_long_session_mutations();
    emit_many_file_resource_tracking();
    emit_search_resource_tracking();
    emit_paste_allocations();
    emit_tab_count_resource_tracking();
    emit_view_count_resource_tracking();
    emit_session_persist_restore_costs();
}

fn emit_large_utf8_load_peak_memory() {
    let root = unique_probe_root("large-utf8-load-memory");
    std::fs::create_dir_all(&root).expect("create large UTF-8 load root");
    let max_bytes = file_backed_open_max_bytes();

    for (step_index, bytes) in [64 * MB, 256 * MB, GB, 2 * GB]
        .into_iter()
        .filter(|bytes| *bytes <= max_bytes)
        .enumerate()
    {
        let path = root.join(format!("utf8_load_{bytes}.txt"));
        write_utf8_text_file(&path, bytes).expect("write UTF-8 load probe file");
        emit_step(
            StepDescriptor {
                scenario: "large_utf8_load_peak_memory",
                scenario_label: "Large UTF-8 load peak memory",
                workload_family: "file-load",
                focus: "peak-memory",
                step_index,
                workload_value: bytes,
                workload_unit: "bytes",
                workload_label: human_bytes(bytes),
            },
            || run_large_utf8_load_cycle(&path),
        );
    }

    let _ = std::fs::remove_dir_all(root);
}

fn emit_file_backed_open_allocations() {
    let root = unique_probe_root("file-backed-open");
    std::fs::create_dir_all(&root).expect("create file-backed open root");
    let max_bytes = file_backed_open_max_bytes();

    for (step_index, bytes) in [32 * MB, 128 * MB, 512 * MB, GB, 2 * GB]
        .into_iter()
        .filter(|bytes| *bytes <= max_bytes)
        .enumerate()
    {
        let path = root.join(format!("file_open_{bytes}.txt"));
        write_utf8_text_file(&path, bytes).expect("write probe file");
        emit_step(
            StepDescriptor {
                scenario: "file_backed_open_first_visible_paint",
                scenario_label: "File-backed open and first visible paint",
                workload_family: "file-load",
                focus: "first-paint",
                step_index,
                workload_value: bytes,
                workload_unit: "bytes",
                workload_label: human_bytes(bytes),
            },
            || run_file_backed_open_first_visible_paint_cycle(&path),
        );
    }

    let _ = std::fs::remove_dir_all(root);
}

fn emit_search_resource_tracking() {
    emit_workload_steps(
        [64 * MB, 256 * MB],
        WorkloadSpec {
            scenario: "search_file_size_resource_tracking",
            scenario_label: "Search file-size allocation tracking",
            workload_family: "search",
            focus: "allocation",
            workload_unit: "bytes",
        },
        run_search_file_size_cycle,
    );

    emit_workload_steps(
        [1_000usize, 10_000],
        WorkloadSpec {
            scenario: "search_target_resource_tracking",
            scenario_label: "Search target-count allocation tracking",
            workload_family: "search",
            focus: "allocation",
            workload_unit: "files",
        },
        run_search_target_count_cycle,
    );
}

fn emit_edited_buffer_search_preview_rendering() {
    emit_workload_steps(
        [256usize, 2_048, 8_192],
        WorkloadSpec {
            scenario: "edited_buffer_search_preview_rendering",
            scenario_label: "Edited-buffer search preview rendering",
            workload_family: "search",
            focus: "preview-rendering",
            workload_unit: "pieces",
        },
        run_edited_buffer_search_preview_cycle,
    );
}

fn emit_provenance_retained_memory() {
    emit_workload_steps(
        [10_000usize, 100_000],
        WorkloadSpec {
            scenario: "provenance_retained_memory",
            scenario_label: "Provenance retained memory after long edit session",
            workload_family: "edit-history",
            focus: "bounded-memory",
            workload_unit: "edits",
        },
        run_provenance_retained_memory_cycle,
    );
}

fn emit_anchor_heavy_view_editing() {
    emit_workload_steps(
        [1_000usize, 10_000, 40_000],
        WorkloadSpec {
            scenario: "anchor_heavy_view_editing",
            scenario_label: "Anchor-heavy many-view editing",
            workload_family: "split-layout",
            focus: "anchors",
            workload_unit: "anchors",
        },
        run_anchor_heavy_view_edit_cycle,
    );
}

fn emit_fragmented_long_session_mutations() {
    emit_workload_steps(
        [1_000usize, 5_000, 20_000],
        WorkloadSpec {
            scenario: "fragmented_long_session_mutation",
            scenario_label: "Fragmented long-session paste/cut/undo/redo",
            workload_family: "edit-paste",
            focus: "fragmented-mutation",
            workload_unit: "fragments",
        },
        run_fragmented_long_session_mutation_cycle,
    );
}

fn emit_paste_allocations() {
    emit_workload_steps(
        [8 * MB, 64 * MB, 128 * MB],
        WorkloadSpec {
            scenario: "paste_allocation",
            scenario_label: "Paste allocation profile",
            workload_family: "edit-paste",
            focus: "allocation",
            workload_unit: "bytes",
        },
        run_paste_cycle,
    );
}

fn emit_many_file_resource_tracking() {
    emit_workload_steps(
        [1_000usize, 10_000, 50_000],
        WorkloadSpec {
            scenario: "many_file_resource_tracking",
            scenario_label: "Many-file allocation and workspace tracking",
            workload_family: "many-files",
            focus: "memory",
            workload_unit: "files",
        },
        run_many_file_count_cycle,
    );
}

fn emit_tab_count_resource_tracking() {
    emit_workload_steps(
        [128usize, 512, 4_096, 10_000],
        WorkloadSpec {
            scenario: "tab_count_resource_tracking",
            scenario_label: "Tab count working-set and page-fault tracking",
            workload_family: "tab-management",
            focus: "memory",
            workload_unit: "tabs",
        },
        run_tab_count_cycle,
    );
}

fn emit_view_count_resource_tracking() {
    emit_workload_steps(
        [128usize, 512, 1_000],
        WorkloadSpec {
            scenario: "view_count_resource_tracking",
            scenario_label: "View count allocation and layout tracking",
            workload_family: "split-layout",
            focus: "memory",
            workload_unit: "views",
        },
        run_view_count_cycle,
    );
}

fn emit_workload_steps(
    values: impl IntoIterator<Item = usize>,
    spec: WorkloadSpec,
    run: impl Fn(usize) -> StepOutcome,
) {
    for (step_index, workload_value) in values.into_iter().enumerate() {
        emit_step(spec.descriptor(step_index, workload_value), || {
            run(workload_value)
        });
    }
}

impl WorkloadSpec {
    fn descriptor(self, step_index: usize, workload_value: usize) -> StepDescriptor {
        StepDescriptor {
            scenario: self.scenario,
            scenario_label: self.scenario_label,
            workload_family: self.workload_family,
            focus: self.focus,
            step_index,
            workload_value,
            workload_unit: self.workload_unit,
            workload_label: workload_label(workload_value, self.workload_unit),
        }
    }
}

fn workload_label(value: usize, unit: &str) -> String {
    match unit {
        "bytes" => human_bytes(value),
        "files" => format!("{value} files"),
        "tabs" => format!("{value} tabs"),
        "views" => format!("{value} views"),
        "pieces" => format!("{value} pieces"),
        "edits" => format!("{value} edits"),
        "anchors" => format!("{value} anchors"),
        "fragments" => format!("{value} fragments"),
        _ => format!("{value} {unit}"),
    }
}

fn emit_session_persist_restore_costs() {
    let root = unique_probe_root("session-cost");
    std::fs::create_dir_all(&root).expect("create session cost root");

    for (step_index, tab_count) in [100usize, 1_000, 10_000].into_iter().enumerate() {
        let tabs = build_tabs(tab_count, SESSION_BYTES_PER_BUFFER);
        let store_root = root.join(format!("tabs_{tab_count}"));
        let store = SessionStore::new(store_root.clone());

        emit_step(
            StepDescriptor {
                scenario: "session_persist_cost",
                scenario_label: "Session persist cost",
                workload_family: "session-persistence",
                focus: "session",
                step_index,
                workload_value: tab_count,
                workload_unit: "tabs",
                workload_label: format!("{tab_count} tabs"),
            },
            || run_session_persist_cycle(&store, &tabs),
        );

        emit_step(
            StepDescriptor {
                scenario: "session_restore_cost",
                scenario_label: "Session restore cost",
                workload_family: "session-persistence",
                focus: "session",
                step_index,
                workload_value: tab_count,
                workload_unit: "tabs",
                workload_label: format!("{tab_count} tabs"),
            },
            || run_session_restore_cycle(&store),
        );

        emit_step(
            StepDescriptor {
                scenario: "startup_visible_restore_cost",
                scenario_label: "Startup-visible session restore",
                workload_family: "session-persistence",
                focus: "startup-visible",
                step_index,
                workload_value: tab_count,
                workload_unit: "tabs",
                workload_label: format!("{tab_count} tabs"),
            },
            || run_startup_visible_restore_cycle(&store),
        );
    }

    let _ = std::fs::remove_dir_all(root);
}

fn emit_step(step: StepDescriptor, run: impl FnOnce() -> StepOutcome) {
    reset_allocation_counters();
    let start = Instant::now();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(run));
    let elapsed_ns = start.elapsed().as_nanos();
    let metrics = allocation_snapshot();
    let (status, outcome, note) = match result {
        Ok(outcome) => ("ok", outcome, None),
        Err(payload) => ("panic", StepOutcome::items(0), Some(panic_message(payload))),
    };

    let event = ResourceEvent {
        scenario: step.scenario,
        scenario_label: step.scenario_label,
        workload_family: step.workload_family,
        focus: step.focus,
        step_index: step.step_index,
        workload_value: step.workload_value,
        workload_unit: step.workload_unit,
        workload_label: step.workload_label,
        elapsed_ns,
        allocated_bytes: metrics.allocated_bytes,
        deallocated_bytes: metrics.deallocated_bytes,
        live_bytes: metrics.live_bytes,
        peak_live_bytes: metrics.peak_live_bytes,
        allocation_count: metrics.allocation_count,
        deallocation_count: metrics.deallocation_count,
        reallocation_count: metrics.reallocation_count,
        result_value: outcome.result_value,
        result_unit: outcome.result_unit,
        result_label: outcome.result_label,
        manifest_size_bytes: outcome.manifest_size_bytes,
        status,
        note,
    };

    println!(
        "{}",
        serde_json::to_string(&event).expect("serialize resource event")
    );
    let _ = std::io::stdout().flush();
}

impl StepOutcome {
    fn items(value: usize) -> Self {
        Self {
            result_value: value,
            result_unit: "items",
            result_label: format!("{value} items"),
            manifest_size_bytes: None,
        }
    }

    fn items_with_manifest(value: usize, manifest_size_bytes: Option<u64>) -> Self {
        Self {
            manifest_size_bytes,
            ..Self::items(value)
        }
    }
}

fn run_file_backed_open_first_visible_paint_cycle(path: &Path) -> StepOutcome {
    let file = FileService::read_file(path).expect("open file through file service");
    let disk_state = FileService::read_disk_state(path).ok();
    let buffer = FileService::build_buffer_from_file_content(path, file, disk_state);
    let painted_rows = render_first_visible_text_paint(&buffer);
    StepOutcome::items(black_box(
        buffer.document().piece_tree().len_bytes() + buffer.line_count + painted_rows,
    ))
}

fn run_large_utf8_load_cycle(path: &Path) -> StepOutcome {
    let file = FileService::read_file(path).expect("open large UTF-8 file through file service");
    let disk_state = FileService::read_disk_state(path).ok();
    let buffer = FileService::build_buffer_from_file_content(path, file, disk_state);
    StepOutcome::items(black_box(
        buffer.document().piece_tree().len_bytes()
            + buffer.document().piece_tree().metrics().newlines
            + buffer.line_count,
    ))
}

fn render_first_visible_text_paint(buffer: &BufferState) -> usize {
    let snapshot = buffer.document_snapshot();
    let visible_text = snapshot
        .extract_range_bounded(
            0..snapshot.len_chars().min(FIRST_VISIBLE_PAINT_MAX_CHARS),
            FIRST_VISIBLE_PAINT_MAX_CHARS,
        )
        .0;
    let ctx = eframe::egui::Context::default();
    let font_id = eframe::egui::FontId::monospace(15.0);
    let highlight_style = EditorHighlightStyle::new(
        eframe::egui::Color32::from_rgb(90, 146, 214),
        eframe::egui::Color32::WHITE,
    );
    let mut rows = 0usize;

    let _ = ctx.run_ui(eframe::egui::RawInput::default(), |ui| {
        eframe::egui::CentralPanel::default().show_inside(ui, |ui| {
            let mut layouter = build_layouter(
                font_id.clone(),
                false,
                eframe::egui::Color32::WHITE,
                highlight_style,
                SearchHighlightState::default(),
                None,
            );
            let galley = layouter(ui, &visible_text, 980.0);
            rows = galley.rows.len().max(1);
        });
    });

    rows
}

fn run_paste_cycle(insert_bytes: usize) -> StepOutcome {
    let mut buffer = BufferState::new(
        "paste_resource.txt".to_owned(),
        utf8_text_of_size(PASTE_RESOURCE_BASE_BYTES),
        None,
    );
    let inserted = utf8_text_of_size(insert_bytes);
    let midpoint = buffer.document().piece_tree().len_chars() / 2;
    buffer.document_mut().insert_direct(midpoint, &inserted);
    buffer.refresh_text_metadata();
    StepOutcome::items(black_box(
        buffer.line_count + buffer.document().piece_tree().len_bytes(),
    ))
}

fn run_many_file_count_cycle(file_count: usize) -> StepOutcome {
    let buffers = (0..file_count)
        .map(|index| {
            BufferState::new(
                format!("file_{index}.txt"),
                utf8_text_of_size(MANY_FILE_BYTES_PER_BUFFER),
                Some(PathBuf::from(format!("file_{index}.txt"))),
            )
        })
        .collect::<Vec<_>>();
    StepOutcome::items(black_box(
        buffers
            .iter()
            .map(|buffer| buffer.line_count + buffer.document().piece_tree().len_bytes())
            .sum(),
    ))
}

fn run_search_file_size_cycle(bytes: usize) -> StepOutcome {
    let text = search_text_of_size(bytes);
    let program = search_capacity_program();
    StepOutcome::items(black_box(
        search_program(black_box(&text), &program).matches.len(),
    ))
}

fn run_search_target_count_cycle(file_count: usize) -> StepOutcome {
    let target = search_text_of_size(4 * KB);
    let program = search_capacity_program();
    StepOutcome::items(black_box(
        (0..file_count)
            .map(|_| search_program(black_box(&target), &program).matches.len())
            .sum(),
    ))
}

fn run_edited_buffer_search_preview_cycle(piece_count: usize) -> StepOutcome {
    let document = build_fragmented_document(piece_count);
    let text = document.extract_text();
    let program = search_capacity_program();
    let matches = search_program(black_box(&text), &program).matches;
    let previews = document
        .piece_tree()
        .previews_for_matches(black_box(&matches), PREVIEW_LIMIT);
    StepOutcome {
        result_value: black_box(previews.len()),
        result_unit: "previews",
        result_label: format!("{} previews from {} matches", previews.len(), matches.len()),
        manifest_size_bytes: None,
    }
}

fn run_provenance_retained_memory_cycle(edit_count: usize) -> StepOutcome {
    let mut document = TextDocument::new(String::new());
    document.set_history_budget(TextHistoryBudget {
        per_file_entry_limit: 100,
        per_file_byte_budget: MB as u64,
        aggregate_byte_budget: 4 * MB as u64,
        persisted_payload_budget: MB as u64,
        derived_from_memory: false,
    });

    for index in 0..edit_count {
        let insert = if index.is_multiple_of(2) {
            "x\n"
        } else {
            "y\n"
        };
        let start = document.piece_tree().len_chars();
        document.insert_direct_with_source(start, insert, PieceSource::Paste);
        document.push_edit_operation_with_source(
            OperationRecord {
                previous_cursor: CursorRange::one(CharCursor::new(start)),
                next_cursor: CursorRange::one(CharCursor::new(start + insert.chars().count())),
                edits: vec![EditOperation {
                    start_char: start,
                    deleted_text: String::new(),
                    inserted_text: insert.to_owned(),
                    deleted_spans: Vec::new(),
                }],
            },
            PieceSource::Paste,
        );
    }

    let provenance_entries = document.piece_tree().provenance_entry_count();
    StepOutcome {
        result_value: black_box(provenance_entries),
        result_unit: "entries",
        result_label: format!(
            "{provenance_entries} provenance entries, {} history entries",
            document.operation_undo_depth()
        ),
        manifest_size_bytes: None,
    }
}

fn run_anchor_heavy_view_edit_cycle(anchor_count: usize) -> StepOutcome {
    let mut tree = PieceTreeLite::from_string(utf8_text_of_size(4 * MB));
    let len = tree.len_chars().max(1);
    let anchors = (0..anchor_count)
        .map(|index| {
            let offset = (index * 97) % len;
            let owner = match index % 4 {
                0 => AnchorOwner::view_scroll(index as u64),
                1 => AnchorOwner::cursor(index as u64),
                2 => AnchorOwner::selection_endpoint(index as u64),
                _ => AnchorOwner::search_endpoint(index as u64),
            };
            tree.create_anchor_with_owner(
                offset,
                if index.is_multiple_of(2) {
                    AnchorBias::Left
                } else {
                    AnchorBias::Right
                },
                owner,
            )
        })
        .collect::<Vec<_>>();

    let midpoint = tree.len_chars() / 2;
    tree.insert(midpoint, "anchor edit café 東京\n");
    tree.remove_char_range(midpoint.saturating_sub(8)..midpoint.saturating_sub(1));
    let resolved = anchors
        .iter()
        .filter_map(|anchor| tree.anchor_position(*anchor))
        .sum::<usize>();
    StepOutcome::items(black_box(resolved))
}

fn run_fragmented_long_session_mutation_cycle(fragment_count: usize) -> StepOutcome {
    let mut document = build_fragmented_document(fragment_count);
    let paste_at = document.piece_tree().len_chars() / 3;
    let pasted = utf8_text_of_size(256 * KB);
    document.insert_direct_with_source(paste_at, &pasted, PieceSource::Paste);
    document.push_edit_operation_with_source(
        OperationRecord {
            previous_cursor: CursorRange::one(CharCursor::new(paste_at)),
            next_cursor: CursorRange::one(CharCursor::new(paste_at + pasted.chars().count())),
            edits: vec![EditOperation {
                start_char: paste_at,
                deleted_text: String::new(),
                inserted_text: pasted,
                deleted_spans: Vec::new(),
            }],
        },
        PieceSource::Paste,
    );

    let cut_start = document.piece_tree().len_chars() / 2;
    let cut_end = (cut_start + 4 * KB).min(document.piece_tree().len_chars());
    let deleted_text = document.piece_tree().extract_range(cut_start..cut_end);
    let deleted_spans = document.byte_spans_for_range(cut_start..cut_end);
    document.delete_char_range_direct(cut_start..cut_end);
    document.push_edit_operation_with_source(
        OperationRecord {
            previous_cursor: CursorRange::two(cut_start, cut_end),
            next_cursor: CursorRange::one(CharCursor::new(cut_start)),
            edits: vec![EditOperation {
                start_char: cut_start,
                deleted_text,
                inserted_text: String::new(),
                deleted_spans,
            }],
        },
        PieceSource::Cut,
    );

    let _ = document.undo_last_operation();
    let _ = document.redo_last_operation();
    StepOutcome {
        result_value: black_box(document.piece_tree().metrics().pieces),
        result_unit: "pieces",
        result_label: format!(
            "{} pieces, {} undo entries",
            document.piece_tree().metrics().pieces,
            document.operation_undo_depth()
        ),
        manifest_size_bytes: None,
    }
}

fn run_tab_count_cycle(tab_count: usize) -> StepOutcome {
    let mut tabs = build_tabs(tab_count, TAB_BYTES_PER_BUFFER);
    let mut activations = 0usize;
    for (index, tab) in tabs.iter_mut().enumerate() {
        let _ = tab.split_active_view(if index.is_multiple_of(2) {
            SplitAxis::Vertical
        } else {
            SplitAxis::Horizontal
        });
        activations += 1;
    }
    if tabs.len() > 2 {
        combine_tabs(&mut tabs, 0, 1);
        activations += 1;
    }
    StepOutcome::items(black_box(activations + tabs.len()))
}

fn run_view_count_cycle(view_count: usize) -> StepOutcome {
    let mut tab = WorkspaceTab::new(BufferState::new(
        "many_views.txt".to_owned(),
        utf8_text_of_size(VIEW_COUNT_BUFFER_BYTES),
        None,
    ));
    while tab.views.len() < view_count {
        let _ = tab.split_active_view(if tab.views.len().is_multiple_of(2) {
            SplitAxis::Vertical
        } else {
            SplitAxis::Horizontal
        });
    }
    let _ = tab.rebalance_views_equally();
    StepOutcome::items(black_box(tab.views.len()))
}

fn run_session_persist_cycle(store: &SessionStore, tabs: &[WorkspaceTab]) -> StepOutcome {
    let profile = store
        .persist_profiled(tabs, 0, 14.0, true)
        .expect("persist session");
    StepOutcome {
        result_value: black_box(profile.tab_count),
        result_unit: "tabs",
        result_label: format!(
            "{} tabs; snapshot {} ms, file I/O {} ms, serialize {} ms",
            profile.tab_count,
            ns_to_ms_label(profile.snapshot_capture_ns),
            ns_to_ms_label(profile.snapshot_write_ns + profile.manifest_write_ns),
            ns_to_ms_label(profile.manifest_serialize_ns)
        ),
        manifest_size_bytes: Some(profile.manifest_size_bytes),
    }
}

fn run_session_restore_cycle(store: &SessionStore) -> StepOutcome {
    let profiled = store.load_profiled().expect("load persisted session");
    let restore_profile = profiled.profile;
    let restored = profiled.restored.expect("restored session present");
    StepOutcome {
        result_value: black_box(restored.tabs.len()),
        result_unit: "tabs",
        result_label: format!(
            "{} tabs; manifest read/parse {} ms, reconstruction {} ms",
            restore_profile.tab_count,
            ns_to_ms_label(restore_profile.manifest_read_parse_ns),
            ns_to_ms_label(restore_profile.restore_reconstruction_ns)
        ),
        manifest_size_bytes: session_manifest_size(store),
    }
}

fn run_startup_visible_restore_cycle(store: &SessionStore) -> StepOutcome {
    let restored = store
        .load()
        .expect("load startup session")
        .expect("restored startup session present");
    let active_index = restored
        .active_tab_index
        .min(restored.tabs.len().saturating_sub(1));
    let painted_rows = restored
        .tabs
        .get(active_index)
        .map(|tab| render_first_visible_text_paint(&tab.buffer))
        .unwrap_or_default();
    StepOutcome::items_with_manifest(
        black_box(restored.tabs.len() + painted_rows),
        session_manifest_size(store),
    )
}

fn build_fragmented_document(fragment_count: usize) -> TextDocument {
    let mut document = TextDocument::new(utf8_text_of_size(MB));
    for index in 0..fragment_count {
        let len = document.piece_tree().len_chars().max(1);
        let offset = (index * 131) % len;
        document.insert_direct_with_source(offset, FRAGMENT_UNIT, PieceSource::Paste);
    }
    document
}

fn session_manifest_size(store: &SessionStore) -> Option<u64> {
    std::fs::metadata(store.root().join("session.json"))
        .ok()
        .map(|metadata| metadata.len())
}

fn build_tabs(tab_count: usize, bytes_per_buffer: usize) -> Vec<WorkspaceTab> {
    (0..tab_count)
        .map(|index| {
            let buffer = BufferState::new(
                format!("tab_{index}.txt"),
                utf8_text_of_size(bytes_per_buffer),
                None,
            );
            WorkspaceTab::new(buffer)
        })
        .collect()
}

fn combine_tabs(tabs: &mut Vec<WorkspaceTab>, source_idx: usize, target_idx: usize) {
    if source_idx == target_idx || source_idx >= tabs.len() || target_idx >= tabs.len() {
        return;
    }

    let source_tab = tabs.remove(source_idx);
    let adjusted_target_idx = if source_idx < target_idx {
        target_idx - 1
    } else {
        target_idx
    };
    let target_tab = &mut tabs[adjusted_target_idx];
    let _ = target_tab.combine_with_tab(source_tab, SplitAxis::Horizontal, false, 0.5);
}

fn write_utf8_text_file(path: &Path, target_bytes: usize) -> std::io::Result<()> {
    let line = UTF8_SAMPLE_LINE.as_bytes();
    let mut file = std::fs::File::create(path)?;
    let repeats = target_bytes / line.len();
    for _ in 0..repeats {
        file.write_all(line)?;
    }
    let remaining = target_bytes % line.len();
    if remaining > 0 {
        let end = utf8_prefix_len(UTF8_SAMPLE_LINE, remaining);
        file.write_all(&line[..end])?;
    }
    file.flush()
}

fn file_backed_open_max_bytes() -> usize {
    std::env::var("SCRATCHPAD_FILE_BACKED_OPEN_MAX_BYTES")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(2 * GB)
}

fn utf8_text_of_size(target_bytes: usize) -> String {
    let repeats = (target_bytes / UTF8_SAMPLE_LINE.len()).max(1);
    let mut text = String::with_capacity(repeats * UTF8_SAMPLE_LINE.len());
    for _ in 0..repeats {
        text.push_str(UTF8_SAMPLE_LINE);
    }
    text
}

fn search_capacity_program() -> SearchProgram {
    SearchProgram::compile(
        "needle",
        SearchOptions {
            mode: SearchMode::PlainText,
            match_case: true,
            whole_word: false,
        },
    )
    .expect("literal search program compiles")
}

fn search_text_of_size(target_bytes: usize) -> String {
    let repeats = (target_bytes / UTF8_SEARCH_UNIT.len()).max(1);
    let mut text = String::with_capacity(repeats * UTF8_SEARCH_UNIT.len());
    for _ in 0..repeats {
        text.push_str(UTF8_SEARCH_UNIT);
    }
    text.push_str("needle café 東京\n");
    text
}

fn utf8_prefix_len(text: &str, max_bytes: usize) -> usize {
    if max_bytes >= text.len() {
        return text.len();
    }
    let mut end = max_bytes;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    end
}

fn unique_probe_root(label: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock before unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "scratchpad-resource-probe-{label}-{}-{nanos}",
        std::process::id()
    ))
}

fn human_bytes(value: usize) -> String {
    if value >= GB {
        return format!("{:.1} GB", value as f64 / GB as f64);
    }
    if value >= MB {
        return format!("{:.1} MB", value as f64 / MB as f64);
    }
    if value >= KB {
        return format!("{:.0} KB", value as f64 / KB as f64);
    }
    format!("{value} B")
}

fn ns_to_ms_label(ns: u128) -> String {
    format!("{:.2}", ns as f64 / 1_000_000.0)
}

fn panic_message(payload: Box<dyn std::any::Any + Send>) -> String {
    if let Some(message) = payload.downcast_ref::<String>() {
        return message.clone();
    }
    if let Some(message) = payload.downcast_ref::<&'static str>() {
        return (*message).to_owned();
    }
    "unknown panic".to_owned()
}
