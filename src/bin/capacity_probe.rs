use scratchpad::app::capacity_metrics::{
    CapacityMetricsSnapshot, capacity_metrics_snapshot, reset_capacity_metrics,
};
use scratchpad::app::domain::{BufferState, SearchHighlightState, SplitAxis, WorkspaceTab};
use scratchpad::app::memory_budget::{self, MemoryBudgetSnapshot};
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
const SPLIT_BYTES_PER_TILE: usize = 256 * KB;
const BASE_PASTE_BUFFER_BYTES: usize = MB;

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

fn main() {
    emit_file_size_sweep();
    emit_layout_bytes_sweep();
    emit_tab_count_sweep();
    emit_split_count_sweep();
    emit_paste_size_sweep();
}

fn emit_file_size_sweep() {
    for (step_index, bytes) in [MB, 8 * MB, 32 * MB, 128 * MB, 512 * MB, GB]
        .into_iter()
        .enumerate()
    {
        emit_step(
            StepDescriptor {
                scenario: "file_size_ceiling",
                scenario_label: "File size ceiling sweep",
                workload_family: "capacity-stress",
                step_index,
                workload_value: bytes,
                workload_unit: "bytes",
                workload_label: human_bytes(bytes),
            },
            || {
                let buffer = BufferState::new(
                    format!("file_size_{bytes}.txt"),
                    plain_text_of_size(bytes),
                    None,
                );
                black_box(buffer.line_count + buffer.document().piece_tree().len_bytes())
            },
        );
    }
}

fn emit_layout_bytes_sweep() {
    for (step_index, bytes) in [64 * KB, MB, 8 * MB, 32 * MB, 128 * MB]
        .into_iter()
        .enumerate()
    {
        emit_step(
            StepDescriptor {
                scenario: "layout_bytes_ceiling",
                scenario_label: "Layout bytes ceiling sweep",
                workload_family: "capacity-measurement",
                step_index,
                workload_value: bytes,
                workload_unit: "bytes",
                workload_label: human_bytes(bytes),
            },
            || black_box(run_layout_capacity_cycle(bytes)),
        );
    }
}

fn emit_tab_count_sweep() {
    for (step_index, tab_count) in [32usize, 512, 4_096, 32_768, 131_072, 512_000]
        .into_iter()
        .enumerate()
    {
        emit_step(
            StepDescriptor {
                scenario: "tab_count_ceiling",
                scenario_label: "Tab count ceiling sweep",
                workload_family: "capacity-stress",
                step_index,
                workload_value: tab_count,
                workload_unit: "tabs",
                workload_label: format!("{tab_count} tabs"),
            },
            || black_box(run_tab_capacity_cycle(tab_count)),
        );
    }
}

fn emit_split_count_sweep() {
    for (step_index, split_count) in [4usize, 8, 16, 24, 32].into_iter().enumerate() {
        emit_step(
            StepDescriptor {
                scenario: "split_count_ceiling",
                scenario_label: "Split count ceiling sweep",
                workload_family: "capacity-stress",
                step_index,
                workload_value: split_count,
                workload_unit: "splits",
                workload_label: format!("{split_count} splits"),
            },
            || black_box(run_split_capacity_cycle(split_count)),
        );
    }
}

fn emit_paste_size_sweep() {
    for (step_index, insert_bytes) in [64 * KB, MB, 8 * MB, 64 * MB, 256 * MB, 512 * MB]
        .into_iter()
        .enumerate()
    {
        emit_step(
            StepDescriptor {
                scenario: "paste_size_ceiling",
                scenario_label: "Paste size ceiling sweep",
                workload_family: "capacity-stress",
                step_index,
                workload_value: insert_bytes,
                workload_unit: "bytes",
                workload_label: human_bytes(insert_bytes),
            },
            || black_box(run_paste_capacity_cycle(insert_bytes)),
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

fn run_layout_capacity_cycle(bytes: usize) -> usize {
    let text = plain_text_of_size(bytes);
    let ctx = eframe::egui::Context::default();
    let font_id = eframe::egui::FontId::monospace(15.0);
    let highlight_style = EditorHighlightStyle::new(
        eframe::egui::Color32::from_rgb(90, 146, 214),
        eframe::egui::Color32::WHITE,
    );
    let mut total_rows = 0usize;

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

            for wrap_width in [980.0, 720.0, 520.0, 980.0] {
                let galley = layouter(ui, &text, wrap_width);
                total_rows += galley.rows.len().max(1);
            }
        });
    });

    total_rows
}

fn run_tab_capacity_cycle(tab_count: usize) -> usize {
    let mut tabs = build_tabs(tab_count, TAB_BYTES_PER_BUFFER);
    let mut activations = 0usize;
    for (index, tab) in tabs.iter_mut().enumerate() {
        tab.split_active_view(if index.is_multiple_of(2) {
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
    activations + tabs.len()
}

fn run_split_capacity_cycle(split_count: usize) -> usize {
    let mut tab = build_tile_heavy_tab(split_count, SPLIT_BYTES_PER_TILE);
    let _ = tab.rebalance_views_equally();
    let _ = tab.split_active_view(SplitAxis::Vertical);
    if tab.views.len() > split_count
        && let Some(view_id) = tab.views.last().map(|view| view.id)
    {
        let _ = tab.close_view(view_id);
    }
    tab.views.len()
}

fn run_paste_capacity_cycle(insert_bytes: usize) -> usize {
    let mut buffer = BufferState::new(
        "paste_capacity.txt".to_owned(),
        plain_text_of_size(BASE_PASTE_BUFFER_BYTES),
        None,
    );
    let inserted = plain_text_of_size(insert_bytes);
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
                plain_text_of_size(bytes_per_buffer),
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

fn build_tile_heavy_tab(tile_count: usize, bytes_per_tile: usize) -> WorkspaceTab {
    let mut tab = WorkspaceTab::new(BufferState::new(
        "tile_0.txt".to_owned(),
        plain_text_of_size(bytes_per_tile),
        None,
    ));
    for tile_index in 1..tile_count.max(1) {
        let _ = tab.open_buffer_with_balanced_layout(BufferState::new(
            format!("tile_{tile_index}.txt"),
            plain_text_of_size(bytes_per_tile),
            None,
        ));
    }
    tab
}

fn plain_text_of_size(target_bytes: usize) -> String {
    let line = "The quick brown fox jumps over the lazy dog 0123456789.\n";
    let repeats = (target_bytes / line.len()).max(1);
    let mut text = String::with_capacity(repeats * line.len());
    for _ in 0..repeats {
        text.push_str(line);
    }
    text
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
