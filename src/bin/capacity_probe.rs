use scratchpad::app::capacity_metrics::{
    CapacityMetricsSnapshot, capacity_metrics_snapshot, reset_capacity_metrics,
};
use scratchpad::app::domain::{
    BufferState, SearchHighlightState, SplitAxis, TabManager, WorkspaceTab,
};
use scratchpad::app::memory_budget::{self, MemoryBudgetSnapshot};
use scratchpad::app::services::file_service::FileService;
use scratchpad::app::services::search::{SearchMode, SearchOptions, SearchProgram, search_program};
use scratchpad::app::ui::editor_content::{EditorHighlightStyle, build_layouter};
use scratchpad::profile::run_many_file_first_visible_profile;
use serde::Serialize;
use std::hint::black_box;
use std::io::{BufWriter, Write};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::time::Instant;

const KB: usize = 1024;
const MB: usize = 1024 * KB;
const GB: usize = 1024 * MB;
const TAB_BYTES_PER_BUFFER: usize = 48 * KB;
const MANY_FILE_BYTES_PER_BUFFER: usize = KB;
const FIRST_VISIBLE_WINDOW_BYTES: usize = MB;
const SPLIT_BYTES_PER_TILE: usize = 128 * KB;
const VIEW_COUNT_BUFFER_BYTES: usize = MB;
const BASE_PASTE_BUFFER_BYTES: usize = MB;
const MEASUREMENT_REPETITIONS: usize = 3;
const UTF8_SAMPLE_LINE: &str =
    "Scratchpad edits UTF-8: café résumé 東京 Привет مرحبا 0123456789.\n";
const UTF8_SEARCH_UNIT: &str = "hay café 東京 Привет مرحبا hay\n";

#[derive(Serialize)]
struct CapacityEvent {
    scenario: &'static str,
    scenario_label: &'static str,
    workload_family: &'static str,
    step_index: usize,
    repeat_index: usize,
    workload_value: usize,
    workload_unit: &'static str,
    workload_label: String,
    setup_elapsed_ns: u128,
    elapsed_ns: u128,
    background_completion_ns: Option<u128>,
    measurement_scope: &'static str,
    metrics: CapacityMetricsSnapshot,
    memory_budget: MemoryBudgetSnapshot,
    status: &'static str,
    note: Option<String>,
}

#[derive(Clone)]
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
    emit_large_file_first_visible_sweep();
    emit_file_size_sweep();
    emit_text_layout_sweep();
    emit_many_file_first_visible_sweep();
    emit_many_file_count_sweep();
    emit_search_file_size_sweep();
    emit_search_target_count_sweep();
    emit_tab_count_sweep();
    emit_split_count_sweep();
    emit_view_count_sweep();
    emit_paste_size_sweep();
}

fn emit_large_file_first_visible_sweep() {
    let root = unique_probe_root("large-file-first-visible");
    std::fs::create_dir_all(&root).expect("create large-file fixture root");
    let fixture_chunk = exact_utf8_text_of_size(8 * MB);
    let descriptor = SweepDescriptor::bytes(
        "large_file_first_visible_ceiling",
        "Large-file first-visible window sweep",
        "file-load",
    );

    for (step_index, file_bytes) in [MB, 128 * MB, 512 * MB, GB].into_iter().enumerate() {
        let path = root.join(format!("visible_{file_bytes}.txt"));
        let setup_start = Instant::now();
        write_large_text_fixture(&path, file_bytes, &fixture_chunk);
        let setup_elapsed_ns = setup_start.elapsed().as_nanos();
        for repeat_index in 0..MEASUREMENT_REPETITIONS {
            emit_prepared_step(
                StepDescriptor {
                    scenario: descriptor.scenario,
                    scenario_label: descriptor.scenario_label,
                    workload_family: descriptor.workload_family,
                    step_index,
                    workload_value: file_bytes,
                    workload_unit: descriptor.workload_unit,
                    workload_label: (descriptor.workload_label)(file_bytes),
                },
                repeat_index,
                setup_elapsed_ns,
                || {
                    let window =
                        FileService::read_first_visible_window(&path, FIRST_VISIBLE_WINDOW_BYTES)
                            .expect("decode first visible file window");
                    black_box(window.text.len() + window.file_size_bytes as usize)
                },
            );
        }
    }

    let _ = std::fs::remove_dir_all(root);
}

fn emit_file_size_sweep() {
    emit_prepared_sweep(
        SweepDescriptor::bytes(
            "file_size_ceiling",
            "File size ceiling sweep",
            "capacity-stress",
        ),
        [MB, 8 * MB, 32 * MB, 128 * MB, 512 * MB, GB],
        utf8_text_of_size,
        |text| {
            let bytes = text.len();
            let buffer = BufferState::new(format!("file_size_{bytes}.txt"), text, None);
            buffer.line_count + buffer.document().piece_tree().len_bytes()
        },
    );
}

fn emit_text_layout_sweep() {
    emit_prepared_sweep(
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
        utf8_text_of_size,
        |text| run_text_layout_capacity_cycle(&text),
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
        for repeat_index in 0..MEASUREMENT_REPETITIONS {
            let setup_start = Instant::now();
            let mut manager = TabManager::new();
            manager.set_tabs(build_tabs(tab_count, TAB_BYTES_PER_BUFFER), 0);
            let setup_elapsed_ns = setup_start.elapsed().as_nanos();
            emit_prepared_step(
                StepDescriptor {
                    scenario: descriptor.scenario,
                    scenario_label: descriptor.scenario_label,
                    workload_family: descriptor.workload_family,
                    step_index,
                    workload_value: tab_count,
                    workload_unit: descriptor.workload_unit,
                    workload_label: (descriptor.workload_label)(tab_count),
                },
                repeat_index,
                setup_elapsed_ns,
                || black_box(run_tab_capacity_cycle(&mut manager)),
            );
        }
    }
}

fn emit_many_file_first_visible_sweep() {
    let root = unique_probe_root("many-file-first-visible");
    std::fs::create_dir_all(&root).expect("create many-file fixture root");
    let descriptor = SweepDescriptor::count(
        "many_file_first_visible_ceiling",
        "Many-file first-visible workspace sweep",
        "files",
        files_label,
    );

    for (step_index, file_count) in [2_048usize, 10_000, 50_000].into_iter().enumerate() {
        let fixture_root = root.join(format!("files_{file_count}"));
        let setup_start = Instant::now();
        std::fs::create_dir_all(&fixture_root).expect("create many-file fixture directory");
        let paths = (0..file_count)
            .map(|index| {
                let path = fixture_root.join(format!("file_{index}.txt"));
                std::fs::write(&path, format!("visible file {index}\n"))
                    .expect("write many-file fixture");
                path
            })
            .collect::<Vec<_>>();
        let setup_elapsed_ns = setup_start.elapsed().as_nanos();

        for repeat_index in 0..MEASUREMENT_REPETITIONS {
            reset_capacity_metrics();
            memory_budget::reset();
            let profile = run_many_file_first_visible_profile(paths.clone());
            black_box(profile.active_buffer_bytes + profile.tab_count_after_completion);
            emit_recorded_step(
                StepDescriptor {
                    scenario: descriptor.scenario,
                    scenario_label: descriptor.scenario_label,
                    workload_family: descriptor.workload_family,
                    step_index,
                    workload_value: file_count,
                    workload_unit: descriptor.workload_unit,
                    workload_label: (descriptor.workload_label)(file_count),
                },
                repeat_index,
                setup_elapsed_ns,
                profile.first_visible_ns,
                Some(profile.background_completion_ns),
                "first_visible_before_background_completion",
                Some(format!(
                    "background completion {:.3} ms; {} tabs installed",
                    profile.background_completion_ns as f64 / 1_000_000.0,
                    profile.tab_count_after_completion
                )),
            );
        }
    }

    let _ = std::fs::remove_dir_all(root);
}

fn emit_many_file_count_sweep() {
    emit_prepared_sweep(
        SweepDescriptor::count(
            "many_file_background_hydration_ceiling",
            "Many-file background hydration completion sweep",
            "files",
            files_label,
        ),
        [2_048usize, 10_000, 50_000],
        |file_count| {
            (0..file_count)
                .map(|index| {
                    (
                        format!("file_{index}.txt"),
                        utf8_text_of_size(MANY_FILE_BYTES_PER_BUFFER),
                        PathBuf::from(format!("file_{index}.txt")),
                    )
                })
                .collect::<Vec<_>>()
        },
        run_many_file_background_hydration_cycle,
    );
}

fn emit_search_file_size_sweep() {
    emit_prepared_sweep(
        SweepDescriptor::bytes(
            "search_file_size_ceiling",
            "Search file-size ceiling sweep",
            "capacity-stress",
        ),
        [MB, 64 * MB, 256 * MB, GB],
        search_text_of_size,
        |text| run_search_file_size_cycle(&text),
    );
}

fn emit_search_target_count_sweep() {
    emit_prepared_sweep(
        SweepDescriptor::count(
            "search_target_count_ceiling",
            "Search target-count ceiling sweep",
            "files",
            files_label,
        ),
        [100usize, 1_000, 10_000],
        |file_count| (file_count, search_text_of_size(4 * KB)),
        |(file_count, target)| run_search_target_count_cycle(file_count, &target),
    );
}

fn emit_split_count_sweep() {
    emit_prepared_sweep(
        SweepDescriptor::count(
            "split_count_ceiling",
            "Split count ceiling sweep",
            "splits",
            splits_label,
        ),
        [4usize, 32, 128, 512, 1_000],
        |split_count| build_tile_heavy_tab(split_count, SPLIT_BYTES_PER_TILE),
        run_split_capacity_cycle,
    );
}

fn emit_view_count_sweep() {
    emit_prepared_sweep(
        SweepDescriptor::count(
            "view_count_ceiling",
            "View count ceiling sweep",
            "views",
            views_label,
        ),
        [32usize, 128, 512, 1_000],
        build_view_heavy_tab,
        run_view_capacity_cycle,
    );
}

fn emit_paste_size_sweep() {
    emit_prepared_sweep(
        SweepDescriptor::bytes(
            "paste_size_ceiling",
            "Paste size ceiling sweep",
            "capacity-stress",
        ),
        [64 * KB, MB, 8 * MB, 64 * MB, 128 * MB, 256 * MB, 512 * MB],
        |insert_bytes| {
            (
                BufferState::new(
                    "paste_capacity.txt".to_owned(),
                    utf8_text_of_size(BASE_PASTE_BUFFER_BYTES),
                    None,
                ),
                utf8_text_of_size(insert_bytes),
            )
        },
        |(mut buffer, inserted)| run_paste_capacity_cycle(&mut buffer, &inserted),
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

fn emit_prepared_sweep<T, const N: usize>(
    descriptor: SweepDescriptor,
    values: [usize; N],
    prepare: impl Fn(usize) -> T,
    run: impl Fn(T) -> usize,
) {
    for (step_index, workload_value) in values.into_iter().enumerate() {
        let step = StepDescriptor {
            scenario: descriptor.scenario,
            scenario_label: descriptor.scenario_label,
            workload_family: descriptor.workload_family,
            step_index,
            workload_value,
            workload_unit: descriptor.workload_unit,
            workload_label: (descriptor.workload_label)(workload_value),
        };
        for repeat_index in 0..MEASUREMENT_REPETITIONS {
            let setup_start = Instant::now();
            let workload = prepare(workload_value);
            let setup_elapsed_ns = setup_start.elapsed().as_nanos();
            emit_prepared_step(step.clone(), repeat_index, setup_elapsed_ns, || {
                black_box(run(workload))
            });
        }
    }
}

fn emit_prepared_step(
    step: StepDescriptor,
    repeat_index: usize,
    setup_elapsed_ns: u128,
    run: impl FnOnce() -> usize,
) {
    emit_measured_step(
        step,
        repeat_index,
        setup_elapsed_ns,
        "prepared_operation",
        run,
    );
}

fn emit_recorded_step(
    step: StepDescriptor,
    repeat_index: usize,
    setup_elapsed_ns: u128,
    elapsed_ns: u128,
    background_completion_ns: Option<u128>,
    measurement_scope: &'static str,
    note: Option<String>,
) {
    let event = CapacityEvent {
        scenario: step.scenario,
        scenario_label: step.scenario_label,
        workload_family: step.workload_family,
        step_index: step.step_index,
        repeat_index,
        workload_value: step.workload_value,
        workload_unit: step.workload_unit,
        workload_label: step.workload_label,
        setup_elapsed_ns,
        elapsed_ns,
        background_completion_ns,
        measurement_scope,
        metrics: capacity_metrics_snapshot(),
        memory_budget: memory_budget::snapshot(),
        status: "ok",
        note,
    };
    println!(
        "{}",
        serde_json::to_string(&event).expect("serialize capacity event")
    );
    let _ = std::io::stdout().flush();
}

fn emit_measured_step(
    step: StepDescriptor,
    repeat_index: usize,
    setup_elapsed_ns: u128,
    measurement_scope: &'static str,
    run: impl FnOnce() -> usize,
) {
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
        repeat_index,
        workload_value: step.workload_value,
        workload_unit: step.workload_unit,
        workload_label: step.workload_label,
        setup_elapsed_ns,
        elapsed_ns,
        background_completion_ns: None,
        measurement_scope,
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

fn run_text_layout_capacity_cycle(text: &str) -> usize {
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
                let galley = layouter(ui, text, wrap_width);
                total_rows += galley.rows.len().max(1);
            }
        });
    });

    total_rows
}

fn run_tab_capacity_cycle(manager: &mut TabManager) -> usize {
    let mut operations = split_tabs_once(&mut manager.tabs);
    if manager.tabs.len() > 2 {
        let last_index = manager.tabs.len() - 1;
        operations += usize::from(manager.reorder_tab(1, last_index));
        operations += usize::from(manager.reorder_tab(last_index, 1));
    }
    operations + manager.tabs.len()
}

fn run_many_file_background_hydration_cycle(files: Vec<(String, String, PathBuf)>) -> usize {
    let buffers = files
        .into_iter()
        .map(|(name, text, path)| BufferState::new(name, text, Some(path)))
        .collect::<Vec<_>>();
    buffers
        .iter()
        .map(|buffer| buffer.line_count + buffer.document().piece_tree().len_bytes())
        .sum()
}

fn run_search_file_size_cycle(text: &str) -> usize {
    let program = search_capacity_program();
    search_program(black_box(text), &program).matches.len()
}

fn run_search_target_count_cycle(file_count: usize, target: &str) -> usize {
    let program = search_capacity_program();
    (0..file_count)
        .map(|_| search_program(black_box(target), &program).matches.len())
        .sum()
}

fn run_split_capacity_cycle(mut tab: WorkspaceTab) -> usize {
    let split_count = tab.layout.views.len();
    let _ = tab.rebalance_views_equally();
    let _ = tab.split_active_view(SplitAxis::Vertical);
    if tab.layout.views.len() > split_count
        && let Some(view_id) = tab.layout.views.last().map(|view| view.id)
    {
        let _ = tab.close_view(view_id);
    }
    tab.layout.views.len()
}

fn build_view_heavy_tab(view_count: usize) -> WorkspaceTab {
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
    tab
}

fn run_view_capacity_cycle(mut tab: WorkspaceTab) -> usize {
    let _ = tab.rebalance_views_equally();
    tab.layout.views.len()
}

fn run_paste_capacity_cycle(buffer: &mut BufferState, inserted: &str) -> usize {
    let midpoint = buffer.document().piece_tree().len_chars() / 2;
    buffer.document_mut().insert_direct(midpoint, inserted);
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

fn exact_utf8_text_of_size(target_bytes: usize) -> String {
    let mut text = utf8_text_of_size(target_bytes);
    text.extend(std::iter::repeat_n(
        ' ',
        target_bytes.saturating_sub(text.len()),
    ));
    text
}

fn write_large_text_fixture(path: &Path, target_bytes: usize, chunk: &str) {
    let file = std::fs::File::create(path).expect("create large text fixture");
    let mut writer = BufWriter::with_capacity(8 * MB, file);
    let mut remaining = target_bytes;
    while remaining >= chunk.len() {
        writer
            .write_all(chunk.as_bytes())
            .expect("write large text fixture chunk");
        remaining -= chunk.len();
    }
    if remaining > 0 {
        let tail = exact_utf8_text_of_size(remaining);
        writer
            .write_all(tail.as_bytes())
            .expect("write final large text fixture chunk");
    }
    writer.flush().expect("flush large text fixture");
}

fn unique_probe_root(label: &str) -> PathBuf {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "scratchpad-capacity-{label}-{}-{stamp}",
        std::process::id()
    ))
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
