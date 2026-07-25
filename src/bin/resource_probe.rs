#[path = "resource_probe/alloc_metrics.rs"]
mod alloc_metrics;
#[path = "resource_probe/events.rs"]
mod events;
#[path = "resource_probe/scenarios.rs"]
mod scenarios;

use events::{StepOutcome, ns_to_ms_label};
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
use scratchpad::profile::{
    run_many_file_lazy_open_profile, run_search_all_tabs_profile, run_tab_strip_frame_profile,
};
use std::hint::black_box;
use std::io::Write;
use std::path::{Path, PathBuf};

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

fn main() {
    scenarios::run_all();
}

fn run_file_backed_open_first_visible_paint_cycle(path: &Path) -> StepOutcome {
    let window = FileService::read_first_visible_window(path, FIRST_VISIBLE_PAINT_MAX_CHARS)
        .expect("read first visible file window");
    let buffer = BufferState::new(
        path.file_name().map_or_else(
            || path.display().to_string(),
            |name| name.to_string_lossy().into_owned(),
        ),
        window.text,
        Some(path.to_path_buf()),
    );
    let painted_rows = render_first_visible_text_paint(&buffer);
    StepOutcome {
        result_value: black_box(window.loaded_bytes + painted_rows),
        result_unit: "bytes+rows",
        result_label: format!(
            "{} visible bytes painted from {} byte file in {} rows; full hydration deferred",
            window.loaded_bytes, window.file_size_bytes, painted_rows
        ),
        manifest_size_bytes: None,
        retained_file_chunks: None,
        file_chunk_cache_limit: None,
    }
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

fn prepare_file_backed_cache_traversal(path: &Path) -> TextDocument {
    FileService::read_file(path)
        .expect("open file-backed cache fixture")
        .document
}

fn run_file_backed_cache_traversal_cycle(document: &TextDocument) -> StepOutcome {
    let tree = document.piece_tree();
    let visited_bytes = tree
        .spans_for_range(0..tree.len_chars())
        .map(|span| black_box(span.text.len()))
        .sum();
    StepOutcome::file_chunks(
        tree.loaded_file_chunk_count(),
        tree.file_chunk_cache_limit(),
        visited_bytes,
    )
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
        eframe::egui::CentralPanel::default().show(ui, |ui| {
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
        retained_file_chunks: None,
        file_chunk_cache_limit: None,
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
        retained_file_chunks: None,
        file_chunk_cache_limit: None,
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
    tree.insert_with_source(midpoint, "anchor edit café 東京\n", PieceSource::Edit);
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
        retained_file_chunks: None,
        file_chunk_cache_limit: None,
    }
}

fn run_tab_count_cycle(tab_count: usize) -> StepOutcome {
    let mut tabs = build_tabs(tab_count, TAB_BYTES_PER_BUFFER);
    let activations = split_tabs_once(&mut tabs) + combine_first_tabs(&mut tabs);
    StepOutcome::items(black_box(activations + tabs.len()))
}

fn run_search_app_result_cycle(tab_count: usize) -> StepOutcome {
    let match_count = run_search_all_tabs_profile(tab_count, 4 * KB, 1);
    StepOutcome::items(black_box(match_count))
}

fn run_many_file_lazy_open_cycle(paths: &[PathBuf]) -> StepOutcome {
    let profile_count = run_many_file_lazy_open_profile(paths);
    StepOutcome::items(black_box(profile_count))
}

fn run_tab_strip_frame_cycle(tab_count: usize) -> StepOutcome {
    let iterations = 10usize;
    let total_ns = run_tab_strip_frame_profile(tab_count, iterations);
    StepOutcome {
        result_value: black_box((total_ns / iterations as u128) as usize),
        result_unit: "ns/frame",
        result_label: format!(
            "{} ns/frame across {iterations} frames",
            total_ns / iterations as u128
        ),
        manifest_size_bytes: None,
        retained_file_chunks: None,
        file_chunk_cache_limit: None,
    }
}

fn run_view_count_cycle(view_count: usize) -> StepOutcome {
    let mut tab = WorkspaceTab::new(BufferState::new(
        "many_views.txt".to_owned(),
        utf8_text_of_size(VIEW_COUNT_BUFFER_BYTES),
        None,
    ));
    while tab.layout.views.len() < view_count {
        let _ = tab.split_active_view(if tab.layout.views.len().is_multiple_of(2) {
            SplitAxis::Vertical
        } else {
            SplitAxis::Horizontal
        });
    }
    let _ = tab.rebalance_views_equally();
    StepOutcome::items(black_box(tab.layout.views.len()))
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
        retained_file_chunks: None,
        file_chunk_cache_limit: None,
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
        retained_file_chunks: None,
        file_chunk_cache_limit: None,
    }
}

fn run_startup_visible_restore_cycle(store: &SessionStore) -> StepOutcome {
    let restored = store
        .load_startup_visible()
        .expect("load startup session")
        .expect("visible startup session present");
    let painted_rows = restored
        .tabs
        .get(restored.active_tab_index)
        .map(|tab| render_first_visible_text_paint(&tab.buffers.buffer))
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

fn split_tabs_once(tabs: &mut [WorkspaceTab]) -> usize {
    let mut activations = 0usize;
    for (index, tab) in tabs.iter_mut().enumerate() {
        let _ = tab.split_active_view(if index.is_multiple_of(2) {
            SplitAxis::Vertical
        } else {
            SplitAxis::Horizontal
        });
        activations += 1;
    }
    activations
}

fn combine_first_tabs(tabs: &mut Vec<WorkspaceTab>) -> usize {
    if tabs.len() > 2 {
        combine_tabs(tabs, 0, 1);
        1
    } else {
        0
    }
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
