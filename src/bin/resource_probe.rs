use scratchpad::app::domain::{BufferState, SearchHighlightState, SplitAxis, WorkspaceTab};
use scratchpad::app::services::file_service::FileService;
use scratchpad::app::services::search::{SearchMode, SearchOptions, SearchProgram, search_program};
use scratchpad::app::services::session_store::SessionStore;
use scratchpad::app::ui::editor_content::{EditorHighlightStyle, build_layouter};
use serde::Serialize;
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
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

static ALLOCATED_BYTES: AtomicU64 = AtomicU64::new(0);
static DEALLOCATED_BYTES: AtomicU64 = AtomicU64::new(0);
static LIVE_BYTES: AtomicU64 = AtomicU64::new(0);
static PEAK_LIVE_BYTES: AtomicU64 = AtomicU64::new(0);
static ALLOCATION_COUNT: AtomicU64 = AtomicU64::new(0);
static DEALLOCATION_COUNT: AtomicU64 = AtomicU64::new(0);
static REALLOCATION_COUNT: AtomicU64 = AtomicU64::new(0);

struct TrackingAllocator;

#[global_allocator]
static GLOBAL_ALLOCATOR: TrackingAllocator = TrackingAllocator;

unsafe impl GlobalAlloc for TrackingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            record_allocation(layout.size() as u64);
        }
        ptr
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc_zeroed(layout) };
        if !ptr.is_null() {
            record_allocation(layout.size() as u64);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if !ptr.is_null() {
            record_deallocation(layout.size() as u64);
        }
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let new_ptr = unsafe { System.realloc(ptr, layout, new_size) };
        if !new_ptr.is_null() {
            REALLOCATION_COUNT.fetch_add(1, Ordering::Relaxed);
            let old_size = layout.size() as u64;
            let new_size = new_size as u64;
            if new_size >= old_size {
                let delta = new_size - old_size;
                if delta > 0 {
                    record_allocation(delta);
                }
            } else {
                let delta = old_size - new_size;
                if delta > 0 {
                    record_deallocation(delta);
                }
            }
        }
        new_ptr
    }
}

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
struct AllocationSnapshot {
    allocated_bytes: u64,
    deallocated_bytes: u64,
    live_bytes: u64,
    peak_live_bytes: u64,
    allocation_count: u64,
    deallocation_count: u64,
    reallocation_count: u64,
}

fn main() {
    emit_file_backed_open_allocations();
    emit_many_file_resource_tracking();
    emit_search_resource_tracking();
    emit_paste_allocations();
    emit_tab_count_resource_tracking();
    emit_view_count_resource_tracking();
    emit_session_persist_restore_costs();
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
        write_plain_text_file(&path, bytes).expect("write probe file");
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
    for (step_index, bytes) in [64 * MB, 256 * MB].into_iter().enumerate() {
        emit_step(
            StepDescriptor {
                scenario: "search_file_size_resource_tracking",
                scenario_label: "Search file-size allocation tracking",
                workload_family: "search",
                focus: "allocation",
                step_index,
                workload_value: bytes,
                workload_unit: "bytes",
                workload_label: human_bytes(bytes),
            },
            || run_search_file_size_cycle(bytes),
        );
    }

    for (step_index, file_count) in [1_000usize, 10_000].into_iter().enumerate() {
        emit_step(
            StepDescriptor {
                scenario: "search_target_resource_tracking",
                scenario_label: "Search target-count allocation tracking",
                workload_family: "search",
                focus: "allocation",
                step_index,
                workload_value: file_count,
                workload_unit: "files",
                workload_label: format!("{file_count} files"),
            },
            || run_search_target_count_cycle(file_count),
        );
    }
}

fn emit_paste_allocations() {
    for (step_index, insert_bytes) in [8 * MB, 64 * MB, 128 * MB].into_iter().enumerate() {
        emit_step(
            StepDescriptor {
                scenario: "paste_allocation",
                scenario_label: "Paste allocation profile",
                workload_family: "edit-paste",
                focus: "allocation",
                step_index,
                workload_value: insert_bytes,
                workload_unit: "bytes",
                workload_label: human_bytes(insert_bytes),
            },
            || run_paste_cycle(insert_bytes),
        );
    }
}

fn emit_many_file_resource_tracking() {
    for (step_index, file_count) in [1_000usize, 10_000, 50_000].into_iter().enumerate() {
        emit_step(
            StepDescriptor {
                scenario: "many_file_resource_tracking",
                scenario_label: "Many-file allocation and workspace tracking",
                workload_family: "many-files",
                focus: "memory",
                step_index,
                workload_value: file_count,
                workload_unit: "files",
                workload_label: format!("{file_count} files"),
            },
            || run_many_file_count_cycle(file_count),
        );
    }
}

fn emit_tab_count_resource_tracking() {
    for (step_index, tab_count) in [128usize, 512, 4_096, 10_000].into_iter().enumerate() {
        emit_step(
            StepDescriptor {
                scenario: "tab_count_resource_tracking",
                scenario_label: "Tab count working-set and page-fault tracking",
                workload_family: "tab-management",
                focus: "memory",
                step_index,
                workload_value: tab_count,
                workload_unit: "tabs",
                workload_label: format!("{tab_count} tabs"),
            },
            || run_tab_count_cycle(tab_count),
        );
    }
}

fn emit_view_count_resource_tracking() {
    for (step_index, view_count) in [128usize, 512, 1_000].into_iter().enumerate() {
        emit_step(
            StepDescriptor {
                scenario: "view_count_resource_tracking",
                scenario_label: "View count allocation and layout tracking",
                workload_family: "split-layout",
                focus: "memory",
                step_index,
                workload_value: view_count,
                workload_unit: "views",
                workload_label: format!("{view_count} views"),
            },
            || run_view_count_cycle(view_count),
        );
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
        Err(payload) => (
            "panic",
            StepOutcome::items(0),
            Some(panic_message(payload)),
        ),
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
        plain_text_of_size(PASTE_RESOURCE_BASE_BYTES),
        None,
    );
    let inserted = plain_text_of_size(insert_bytes);
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
                plain_text_of_size(MANY_FILE_BYTES_PER_BUFFER),
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
        plain_text_of_size(VIEW_COUNT_BUFFER_BYTES),
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
    store.persist(tabs, 0, 14.0, true).expect("persist session");
    StepOutcome::items_with_manifest(black_box(tabs.len()), session_manifest_size(store))
}

fn run_session_restore_cycle(store: &SessionStore) -> StepOutcome {
    let restored = store
        .load()
        .expect("load persisted session")
        .expect("restored session present");
    StepOutcome::items_with_manifest(black_box(restored.tabs.len()), session_manifest_size(store))
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

fn write_plain_text_file(path: &Path, target_bytes: usize) -> std::io::Result<()> {
    let line = b"The quick brown fox jumps over the lazy dog 0123456789.\n";
    let mut file = std::fs::File::create(path)?;
    let repeats = target_bytes / line.len();
    for _ in 0..repeats {
        file.write_all(line)?;
    }
    let remaining = target_bytes % line.len();
    if remaining > 0 {
        file.write_all(&line[..remaining])?;
    }
    file.flush()
}

fn file_backed_open_max_bytes() -> usize {
    std::env::var("SCRATCHPAD_FILE_BACKED_OPEN_MAX_BYTES")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(2 * GB)
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
    let unit = "hay hay hay hay\n";
    let repeats = (target_bytes / unit.len()).max(1);
    let mut text = String::with_capacity(repeats * unit.len());
    for _ in 0..repeats {
        text.push_str(unit);
    }
    text.push_str("needle\n");
    text
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

fn record_allocation(bytes: u64) {
    ALLOCATED_BYTES.fetch_add(bytes, Ordering::Relaxed);
    ALLOCATION_COUNT.fetch_add(1, Ordering::Relaxed);
    let live = add_live_bytes(bytes);
    update_peak_live(live);
}

fn record_deallocation(bytes: u64) {
    DEALLOCATED_BYTES.fetch_add(bytes, Ordering::Relaxed);
    DEALLOCATION_COUNT.fetch_add(1, Ordering::Relaxed);
    subtract_live_bytes(bytes);
}

fn add_live_bytes(bytes: u64) -> u64 {
    let mut current = LIVE_BYTES.load(Ordering::Relaxed);
    loop {
        let next = current.saturating_add(bytes);
        match LIVE_BYTES.compare_exchange(current, next, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return next,
            Err(observed) => current = observed,
        }
    }
}

fn subtract_live_bytes(bytes: u64) {
    let mut current = LIVE_BYTES.load(Ordering::Relaxed);
    loop {
        let next = current.saturating_sub(bytes);
        match LIVE_BYTES.compare_exchange(current, next, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return,
            Err(observed) => current = observed,
        }
    }
}

fn update_peak_live(candidate: u64) {
    let mut current = PEAK_LIVE_BYTES.load(Ordering::Relaxed);
    while candidate > current {
        match PEAK_LIVE_BYTES.compare_exchange(
            current,
            candidate,
            Ordering::Relaxed,
            Ordering::Relaxed,
        ) {
            Ok(_) => break,
            Err(observed) => current = observed,
        }
    }
}

fn reset_allocation_counters() {
    ALLOCATED_BYTES.store(0, Ordering::Relaxed);
    DEALLOCATED_BYTES.store(0, Ordering::Relaxed);
    LIVE_BYTES.store(0, Ordering::Relaxed);
    PEAK_LIVE_BYTES.store(0, Ordering::Relaxed);
    ALLOCATION_COUNT.store(0, Ordering::Relaxed);
    DEALLOCATION_COUNT.store(0, Ordering::Relaxed);
    REALLOCATION_COUNT.store(0, Ordering::Relaxed);
}

fn allocation_snapshot() -> AllocationSnapshot {
    AllocationSnapshot {
        allocated_bytes: ALLOCATED_BYTES.load(Ordering::Relaxed),
        deallocated_bytes: DEALLOCATED_BYTES.load(Ordering::Relaxed),
        live_bytes: LIVE_BYTES.load(Ordering::Relaxed),
        peak_live_bytes: PEAK_LIVE_BYTES.load(Ordering::Relaxed),
        allocation_count: ALLOCATION_COUNT.load(Ordering::Relaxed),
        deallocation_count: DEALLOCATION_COUNT.load(Ordering::Relaxed),
        reallocation_count: REALLOCATION_COUNT.load(Ordering::Relaxed),
    }
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
