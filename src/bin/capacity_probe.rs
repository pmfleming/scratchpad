use scratchpad::app::capacity_metrics::{
    CapacityMetricsSnapshot, capacity_metrics_snapshot, reset_capacity_metrics,
};
use scratchpad::app::domain::{BufferState, SearchHighlightState, SplitAxis, WorkspaceTab};
use scratchpad::app::memory_budget::{self, MemoryBudgetSnapshot};
use scratchpad::app::services::search::{SearchMode, SearchOptions, SearchProgram, search_program};
use scratchpad::app::ui::editor_content::{EditorHighlightStyle, build_layouter};
use serde::Serialize;
use std::hint::black_box;
use std::io::Write;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::time::Instant;

const KB: usize = 1024;
const MB: usize = 1024 * KB;
const GB: usize = 1024 * MB;
const TAB_BYTES_PER_BUFFER: usize = 48 * KB;
const MANY_FILE_BYTES_PER_BUFFER: usize = KB;
const SPLIT_BYTES_PER_TILE: usize = 128 * KB;
const VIEW_COUNT_BUFFER_BYTES: usize = MB;
const BASE_PASTE_BUFFER_BYTES: usize = MB;
const UTF8_SAMPLE_LINE: &str =
    "Scratchpad edits UTF-8: café résumé 東京 Привет مرحبا 0123456789.\n";
const UTF8_SEARCH_UNIT: &str = "hay café 東京 Привет مرحبا hay\n";

#[derive(Serialize)]
struct CapacityEvent {
    scenario: &'static str,
    scenario_label: &'static str,
    workload_family: &'static str,
    step_index: usize,
    workload_value: usize,
    workload_unit: &'static str,
    workload_label: String,
    elapsed_ns: u128,
    metrics: CapacityMetricsSnapshot,
    memory_budget: MemoryBudgetSnapshot,
    status: &'static str,
    note: Option<String>,
}

struct StepDescriptor {
    scenario: &'static str,
    scenario_label: &'static str,
    workload_family: &'static str,
    step_index: usize,
    workload_value: usize,
    workload_unit: &'static str,
    workload_label: String,
}

struct SweepDescriptor {
    scenario: &'static str,
    scenario_label: &'static str,
    workload_family: &'static str,
    workload_unit: &'static str,
    workload_label: fn(usize) -> String,
}

fn main() {
    emit_file_size_sweep();
    emit_text_layout_sweep();
    emit_many_file_count_sweep();
    emit_search_file_size_sweep();
    emit_search_target_count_sweep();
    emit_tab_count_sweep();
    emit_split_count_sweep();
    emit_view_count_sweep();
    emit_paste_size_sweep();
}

fn emit_file_size_sweep() {
    emit_sweep(
        SweepDescriptor::bytes(
            "file_size_ceiling",
            "File size ceiling sweep",
            "capacity-stress",
        ),
        [MB, 8 * MB, 32 * MB, 128 * MB, 512 * MB, GB],
        |bytes| {
            let buffer = BufferState::new(
                format!("file_size_{bytes}.txt"),
                utf8_text_of_size(bytes),
                None,
            );
            buffer.line_count + buffer.document().piece_tree().len_bytes()
        },
    );
}

fn emit_text_layout_sweep() {
    emit_sweep(
        SweepDescriptor::bytes("text_layout_ceiling", "Text Layout", "text-layout"),
        [
            64 * KB,
            MB,
            4 * MB,
            8 * MB,
            16 * MB,
            32 * MB,
            64 * MB,
            128 * MB,
        ],
        run_text_layout_capacity_cycle,
    );
}

fn emit_tab_count_sweep() {
    let descriptor = SweepDescriptor::count(
        "tab_count_ceiling",
        "Tab manipulation ceiling sweep",
        "tabs",
        tabs_label,
    );
    for (step_index, tab_count) in [32usize, 512, 4_096, 10_000, 20_000]
        .into_iter()
        .enumerate()
    {
        let mut tabs = build_tabs(tab_count, TAB_BYTES_PER_BUFFER);
        emit_step(
            StepDescriptor {
                scenario: descriptor.scenario,
                scenario_label: descriptor.scenario_label,
                workload_family: descriptor.workload_family,
                step_index,
                workload_value: tab_count,
                workload_unit: descriptor.workload_unit,
                workload_label: (descriptor.workload_label)(tab_count),
            },
            || black_box(run_tab_capacity_cycle(&mut tabs)),
        );
    }
}

fn emit_many_file_count_sweep() {
    emit_sweep(
        SweepDescriptor::count(
            "many_file_count_ceiling",
            "Many-file workspace ceiling sweep",
            "files",
            files_label,
        ),
        [1_000usize, 10_000, 50_000],
        run_many_file_capacity_cycle,
    );
}

fn emit_search_file_size_sweep() {
    emit_sweep(
        SweepDescriptor::bytes(
            "search_file_size_ceiling",
            "Search file-size ceiling sweep",
            "capacity-stress",
        ),
        [MB, 64 * MB, 256 * MB, GB],
        run_search_file_size_cycle,
    );
}

fn emit_search_target_count_sweep() {
    emit_sweep(
        SweepDescriptor::count(
            "search_target_count_ceiling",
            "Search target-count ceiling sweep",
            "files",
            files_label,
        ),
        [100usize, 1_000, 10_000],
        run_search_target_count_cycle,
    );
}

fn emit_split_count_sweep() {
    emit_sweep(
        SweepDescriptor::count(
            "split_count_ceiling",
            "Split count ceiling sweep",
            "splits",
            splits_label,
        ),
        [4usize, 32, 128, 512, 1_000],
        run_split_capacity_cycle,
    );
}

fn emit_view_count_sweep() {
    emit_sweep(
        SweepDescriptor::count(
            "view_count_ceiling",
            "View count ceiling sweep",
            "views",
            views_label,
        ),
        [32usize, 128, 512, 1_000],
        run_view_capacity_cycle,
    );
}

fn emit_paste_size_sweep() {
    emit_sweep(
        SweepDescriptor::bytes(
            "paste_size_ceiling",
            "Paste size ceiling sweep",
            "capacity-stress",
        ),
        [64 * KB, MB, 8 * MB, 64 * MB, 256 * MB, 512 * MB],
        run_paste_capacity_cycle,
    );
}

impl SweepDescriptor {
    fn bytes(
        scenario: &'static str,
        scenario_label: &'static str,
        workload_family: &'static str,
    ) -> Self {
        Self {
            scenario,
            scenario_label,
            workload_family,
            workload_unit: "bytes",
            workload_label: human_bytes,
        }
    }

    fn count(
        scenario: &'static str,
        scenario_label: &'static str,
        workload_unit: &'static str,
        workload_label: fn(usize) -> String,
    ) -> Self {
        Self {
            scenario,
            scenario_label,
            workload_family: "capacity-stress",
            workload_unit,
            workload_label,
        }
    }
}

fn emit_sweep<const N: usize>(
    descriptor: SweepDescriptor,
    values: [usize; N],
    run: impl Fn(usize) -> usize,
) {
    for (step_index, workload_value) in values.into_iter().enumerate() {
        emit_step(
            StepDescriptor {
                scenario: descriptor.scenario,
                scenario_label: descriptor.scenario_label,
                workload_family: descriptor.workload_family,
                step_index,
                workload_value,
                workload_unit: descriptor.workload_unit,
                workload_label: (descriptor.workload_label)(workload_value),
            },
            || black_box(run(workload_value)),
        );
    }
}

fn emit_step(step: StepDescriptor, run: impl FnOnce() -> usize) {
    reset_capacity_metrics();
    memory_budget::reset();
    let start = Instant::now();
    let result = catch_unwind(AssertUnwindSafe(run));
    let elapsed_ns = start.elapsed().as_nanos();
    let metrics = capacity_metrics_snapshot();
    let memory_budget_snapshot = memory_budget::snapshot();
    let (status, note) = match result {
        Ok(_) => ("ok", None),
        Err(payload) => ("panic", Some(panic_message(payload))),
    };

    let event = CapacityEvent {
        scenario: step.scenario,
        scenario_label: step.scenario_label,
        workload_family: step.workload_family,
        step_index: step.step_index,
        workload_value: step.workload_value,
        workload_unit: step.workload_unit,
        workload_label: step.workload_label,
        elapsed_ns,
        metrics,
        memory_budget: memory_budget_snapshot,
        status,
        note,
    };
    println!(
        "{}",
        serde_json::to_string(&event).expect("serialize capacity event")
    );
    let _ = std::io::stdout().flush();
}

fn run_text_layout_capacity_cycle(bytes: usize) -> usize {
    let text = utf8_text_of_size(bytes);
    let ctx = eframe::egui::Context::default();
    let font_id = eframe::egui::FontId::monospace(15.0);
    let highlight_style = EditorHighlightStyle::new(
        eframe::egui::Color32::from_rgb(90, 146, 214),
        eframe::egui::Color32::WHITE,
    );
    let mut total_rows = 0usize;

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

            for wrap_width in [980.0, 720.0, 520.0, 980.0] {
                let galley = layouter(ui, &text, wrap_width);
                total_rows += galley.rows.len().max(1);
            }
        });
    });

    total_rows
}

fn run_tab_capacity_cycle(tabs: &mut Vec<WorkspaceTab>) -> usize {
    let activations = split_tabs_once(tabs) + combine_first_tabs(tabs);
    activations + tabs.len()
}

fn run_many_file_capacity_cycle(file_count: usize) -> usize {
    let buffers = (0..file_count)
        .map(|index| {
            BufferState::new(
                format!("file_{index}.txt"),
                utf8_text_of_size(MANY_FILE_BYTES_PER_BUFFER),
                Some(std::path::PathBuf::from(format!("file_{index}.txt"))),
            )
        })
        .collect::<Vec<_>>();
    buffers
        .iter()
        .map(|buffer| buffer.line_count + buffer.document().piece_tree().len_bytes())
        .sum()
}

fn run_search_file_size_cycle(bytes: usize) -> usize {
    let text = search_text_of_size(bytes);
    let program = search_capacity_program();
    search_program(black_box(&text), &program).matches.len()
}

fn run_search_target_count_cycle(file_count: usize) -> usize {
    let program = search_capacity_program();
    let target = search_text_of_size(4 * KB);
    (0..file_count)
        .map(|_| search_program(black_box(&target), &program).matches.len())
        .sum()
}

fn run_split_capacity_cycle(split_count: usize) -> usize {
    let mut tab = build_tile_heavy_tab(split_count, SPLIT_BYTES_PER_TILE);
    let _ = tab.rebalance_views_equally();
    let _ = tab.split_active_view(SplitAxis::Vertical);
    if tab.layout.views.len() > split_count
        && let Some(view_id) = tab.layout.views.last().map(|view| view.id)
    {
        let _ = tab.close_view(view_id);
    }
    tab.layout.views.len()
}

fn run_view_capacity_cycle(view_count: usize) -> usize {
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
    tab.layout.views.len()
}

fn run_paste_capacity_cycle(insert_bytes: usize) -> usize {
    let mut buffer = BufferState::new(
        "paste_capacity.txt".to_owned(),
        utf8_text_of_size(BASE_PASTE_BUFFER_BYTES),
        None,
    );
    let inserted = utf8_text_of_size(insert_bytes);
    let midpoint = buffer.document().piece_tree().len_chars() / 2;
    buffer.document_mut().insert_direct(midpoint, &inserted);
    buffer.refresh_text_metadata();
    buffer.line_count + buffer.document().piece_tree().len_bytes()
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

fn build_tile_heavy_tab(tile_count: usize, bytes_per_tile: usize) -> WorkspaceTab {
    let mut tab = WorkspaceTab::new(BufferState::new(
        "tile_0.txt".to_owned(),
        utf8_text_of_size(bytes_per_tile),
        None,
    ));
    for tile_index in 1..tile_count.max(1) {
        let _ = tab.open_buffer_with_balanced_layout(BufferState::new(
            format!("tile_{tile_index}.txt"),
            utf8_text_of_size(bytes_per_tile),
            None,
        ));
    }
    tab
}

fn utf8_text_of_size(target_bytes: usize) -> String {
    repeat_unit_to_target_size(UTF8_SAMPLE_LINE, target_bytes)
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
    let mut text = repeat_unit_to_target_size(UTF8_SEARCH_UNIT, target_bytes);
    text.push_str("needle café 東京\n");
    text
}

fn repeat_unit_to_target_size(unit: &str, target_bytes: usize) -> String {
    let repeats = (target_bytes / unit.len()).max(1);
    let mut text = String::with_capacity(repeats * unit.len());
    for _ in 0..repeats {
        text.push_str(unit);
    }
    text
}

fn tabs_label(value: usize) -> String {
    format!("{value} tabs")
}

fn files_label(value: usize) -> String {
    format!("{value} files")
}

fn splits_label(value: usize) -> String {
    format!("{value} splits")
}

fn views_label(value: usize) -> String {
    format!("{value} views")
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

fn panic_message(payload: Box<dyn std::any::Any + Send>) -> String {
    if let Some(message) = payload.downcast_ref::<String>() {
        return message.clone();
    }
    if let Some(message) = payload.downcast_ref::<&'static str>() {
        return (*message).to_owned();
    }
    "unknown panic".to_owned()
}
