use ropey::Rope;
use scratchpad::app::domain::buffer::PieceTreeLite;
use serde::Serialize;
use std::alloc::{GlobalAlloc, Layout, System};
use std::fs::File;
use std::hint::black_box;
use std::io::{BufReader, Write};
use std::ops::Range;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

const KB: usize = 1024;
const MB: usize = 1024 * KB;
const MARKER: &str = "needle";
const LOAD_SIZES: [usize; 3] = [32 * MB, 128 * MB, 512 * MB];
const INSERT_WORKLOADS: [(usize, usize); 4] = [
    (MB, 8 * MB),
    (MB, 64 * MB),
    (32 * MB, 8 * MB),
    (32 * MB, 64 * MB),
];
const PREVIEW_SIZES: [usize; 2] = [32 * MB, 128 * MB];
const LINE_LOOKUP_SIZES: [usize; 3] = [32 * MB, 128 * MB, 512 * MB];
const UNDO_HISTORY_WORKLOADS: [(usize, usize, usize); 2] =
    [(8 * MB, 256, 16 * KB), (32 * MB, 256, 16 * KB)];
const PREVIEW_MAX_CHARS: usize = 96;

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

#[derive(Clone)]
struct Measurement {
    storage: &'static str,
    elapsed_ns: u128,
    metrics: AllocationSnapshot,
    result_value: usize,
}

#[derive(Clone)]
struct WorkloadDescriptor {
    scenario: &'static str,
    scenario_label: &'static str,
    workload_family: &'static str,
    step_index: usize,
    primary_value: usize,
    primary_unit: &'static str,
    primary_label: String,
    secondary_value: Option<usize>,
    secondary_unit: Option<&'static str>,
    secondary_label: Option<String>,
}

#[derive(Serialize)]
struct MeasurementEvent {
    event: &'static str,
    scenario: &'static str,
    scenario_label: &'static str,
    workload_family: &'static str,
    step_index: usize,
    storage: &'static str,
    storage_impl: &'static str,
    primary_workload_value: usize,
    primary_workload_unit: &'static str,
    primary_workload_label: String,
    secondary_workload_value: Option<usize>,
    secondary_workload_unit: Option<&'static str>,
    secondary_workload_label: Option<String>,
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
    status: &'static str,
    note: Option<String>,
}

#[derive(Serialize)]
struct ComparisonEvent {
    event: &'static str,
    scenario: &'static str,
    scenario_label: &'static str,
    workload_family: &'static str,
    step_index: usize,
    primary_workload_value: usize,
    primary_workload_unit: &'static str,
    primary_workload_label: String,
    secondary_workload_value: Option<usize>,
    secondary_workload_unit: Option<&'static str>,
    secondary_workload_label: Option<String>,
    fastest_storage: &'static str,
    lower_allocated_storage: &'static str,
    lower_peak_storage: &'static str,
    piece_tree_elapsed_ratio_vs_rope: f64,
    piece_tree_allocated_ratio_vs_rope: Option<f64>,
    piece_tree_peak_ratio_vs_rope: Option<f64>,
    piece_tree_elapsed_ratio_vs_string: f64,
    piece_tree_allocated_ratio_vs_string: Option<f64>,
    piece_tree_peak_ratio_vs_string: Option<f64>,
    piece_tree_wins_time_vs_rope: bool,
    piece_tree_wins_allocations_vs_rope: bool,
    piece_tree_wins_peak_memory_vs_rope: bool,
}

#[derive(Serialize)]
struct DecisionEvent {
    event: &'static str,
    chosen_piece_tree_probe_impl: &'static str,
    compared_rope_impl: &'static str,
    compared_string_impl: &'static str,
    comparison_count: usize,
    undo_history_piece_tree_beats_rope: bool,
    preview_workloads_favor_rope: bool,
    line_lookup_workloads_favor_rope: bool,
    piece_tree_time_wins_vs_rope: usize,
    piece_tree_allocation_wins_vs_rope: usize,
    piece_tree_peak_wins_vs_rope: usize,
    summary: &'static str,
}

struct ProbeCorpus {
    text: String,
    match_range: Range<usize>,
    near_end_line_index: usize,
}

fn main() {
    let mut comparisons = Vec::new();
    emit_load_sweep(&mut comparisons);
    emit_insert_sweep(&mut comparisons);
    emit_preview_sweep(&mut comparisons);
    emit_line_lookup_sweep(&mut comparisons);
    emit_undo_history_sweep(&mut comparisons);
    emit_decision_summary(&comparisons);
}

fn emit_load_sweep(comparisons: &mut Vec<ComparisonEvent>) {
    let root = unique_probe_root("piece-tree-load");
    std::fs::create_dir_all(&root).expect("create piece-tree probe root");

    for (step_index, bytes) in LOAD_SIZES.into_iter().enumerate() {
        let path = root.join(format!("piece_tree_probe_load_{bytes}.txt"));
        write_plain_text_file(&path, bytes).expect("write piece-tree probe file");
        let workload = WorkloadDescriptor {
            scenario: "text_load",
            scenario_label: "File-backed text load",
            workload_family: "large-file-load",
            step_index,
            primary_value: bytes,
            primary_unit: "bytes",
            primary_label: human_bytes(bytes),
            secondary_value: None,
            secondary_unit: None,
            secondary_label: None,
        };

        let string_measurement = measure("string", || run_string_load(&path));
        let rope_measurement = measure("rope", || run_rope_load(&path));
        let piece_tree_measurement = measure("piece_tree", || run_piece_tree_load(&path));
        emit_measurement_triplet(
            &workload,
            &string_measurement,
            &rope_measurement,
            &piece_tree_measurement,
            comparisons,
        );
    }

    let _ = std::fs::remove_dir_all(root);
}

fn emit_insert_sweep(comparisons: &mut Vec<ComparisonEvent>) {
    for (step_index, (base_bytes, insert_bytes)) in INSERT_WORKLOADS.into_iter().enumerate() {
        let base_text = plain_text_of_size(base_bytes);
        let inserted = plain_text_of_size(insert_bytes);
        let midpoint = base_text.len() / 2;
        let workload = WorkloadDescriptor {
            scenario: "mid_document_insert",
            scenario_label: "Mid-document insert",
            workload_family: "edit-paste",
            step_index,
            primary_value: base_bytes,
            primary_unit: "bytes",
            primary_label: format!("base {}", human_bytes(base_bytes)),
            secondary_value: Some(insert_bytes),
            secondary_unit: Some("bytes"),
            secondary_label: Some(format!("insert {}", human_bytes(insert_bytes))),
        };

        let mut string_text = base_text.clone();
        let string_measurement = measure("string", || {
            string_text.insert_str(midpoint, inserted.as_str());
            black_box(string_text.len())
        });

        let mut rope = Rope::from_str(base_text.as_str());
        let rope_measurement = measure("rope", || {
            rope.insert(midpoint, inserted.as_str());
            black_box(rope.len_bytes())
        });

        let mut piece_tree = PieceTreeLite::from_string(base_text);
        let piece_tree_measurement = measure("piece_tree", || {
            piece_tree.insert(midpoint, inserted.as_str());
            black_box(piece_tree.len_bytes())
        });

        emit_measurement_triplet(
            &workload,
            &string_measurement,
            &rope_measurement,
            &piece_tree_measurement,
            comparisons,
        );
    }
}

fn emit_preview_sweep(comparisons: &mut Vec<ComparisonEvent>) {
    for (step_index, bytes) in PREVIEW_SIZES.into_iter().enumerate() {
        let corpus = preview_probe_corpus(bytes);
        let workload = WorkloadDescriptor {
            scenario: "search_preview_extraction",
            scenario_label: "Search preview extraction",
            workload_family: "search-preview",
            step_index,
            primary_value: bytes,
            primary_unit: "bytes",
            primary_label: human_bytes(bytes),
            secondary_value: Some(corpus.match_range.start),
            secondary_unit: Some("bytes"),
            secondary_label: Some("match near document tail".to_owned()),
        };

        let rope = Rope::from_str(corpus.text.as_str());
        let piece_tree = PieceTreeLite::from_string(corpus.text.clone());

        let string_measurement = measure("string", || {
            let (line, column, preview) =
                ascii_string_preview_for_match(corpus.text.as_str(), &corpus.match_range);
            black_box(line + column + preview.len())
        });
        let rope_measurement = measure("rope", || {
            let (line, column, preview) = rope_preview_for_match(&rope, &corpus.match_range);
            black_box(line + column + preview.len())
        });
        let piece_tree_measurement = measure("piece_tree", || {
            let (line, column, preview) = piece_tree.preview_for_match(&corpus.match_range);
            black_box(line + column + preview.len())
        });

        emit_measurement_triplet(
            &workload,
            &string_measurement,
            &rope_measurement,
            &piece_tree_measurement,
            comparisons,
        );
    }
}

fn emit_line_lookup_sweep(comparisons: &mut Vec<ComparisonEvent>) {
    for (step_index, bytes) in LINE_LOOKUP_SIZES.into_iter().enumerate() {
        let corpus = preview_probe_corpus(bytes);
        let line_index = corpus.near_end_line_index;
        let workload = WorkloadDescriptor {
            scenario: "line_lookup_near_end",
            scenario_label: "Line lookup near end of document",
            workload_family: "line-lookup",
            step_index,
            primary_value: bytes,
            primary_unit: "bytes",
            primary_label: human_bytes(bytes),
            secondary_value: Some(line_index),
            secondary_unit: Some("line-index"),
            secondary_label: Some(format!("line {line_index}")),
        };

        let rope = Rope::from_str(corpus.text.as_str());
        let piece_tree = PieceTreeLite::from_string(corpus.text.clone());

        let string_measurement = measure("string", || {
            let (line_start, line_len) = ascii_string_line_lookup(corpus.text.as_str(), line_index);
            black_box(line_start + line_len)
        });
        let rope_measurement = measure("rope", || {
            let (line_start, line_len) = rope_line_lookup(&rope, line_index);
            black_box(line_start + line_len)
        });
        let piece_tree_measurement = measure("piece_tree", || {
            let (line_start, line_len) = piece_tree.line_lookup(line_index);
            black_box(line_start + line_len)
        });

        emit_measurement_triplet(
            &workload,
            &string_measurement,
            &rope_measurement,
            &piece_tree_measurement,
            comparisons,
        );
    }
}

fn emit_undo_history_sweep(comparisons: &mut Vec<ComparisonEvent>) {
    for (step_index, (base_bytes, edit_count, insert_bytes)) in
        UNDO_HISTORY_WORKLOADS.into_iter().enumerate()
    {
        let workload = WorkloadDescriptor {
            scenario: "undo_heavy_edit_history",
            scenario_label: "Undo-heavy edit history",
            workload_family: "undo-history",
            step_index,
            primary_value: base_bytes,
            primary_unit: "bytes",
            primary_label: format!("base {}", human_bytes(base_bytes)),
            secondary_value: Some(edit_count),
            secondary_unit: Some("edits"),
            secondary_label: Some(format!(
                "{edit_count} inserts of {}",
                human_bytes(insert_bytes)
            )),
        };

        let string_measurement = measure("string", || {
            run_string_undo_history(base_bytes, edit_count, insert_bytes)
        });
        let rope_measurement = measure("rope", || {
            run_rope_undo_history(base_bytes, edit_count, insert_bytes)
        });
        let piece_tree_measurement = measure("piece_tree", || {
            run_piece_tree_undo_history(base_bytes, edit_count, insert_bytes)
        });

        emit_measurement_triplet(
            &workload,
            &string_measurement,
            &rope_measurement,
            &piece_tree_measurement,
            comparisons,
        );
    }
}

fn emit_measurement_triplet(
    workload: &WorkloadDescriptor,
    string_measurement: &Measurement,
    rope_measurement: &Measurement,
    piece_tree_measurement: &Measurement,
    comparisons: &mut Vec<ComparisonEvent>,
) {
    emit_measurement_event(workload, string_measurement);
    emit_measurement_event(workload, rope_measurement);
    emit_measurement_event(workload, piece_tree_measurement);

    let comparison = ComparisonEvent {
        event: "comparison",
        scenario: workload.scenario,
        scenario_label: workload.scenario_label,
        workload_family: workload.workload_family,
        step_index: workload.step_index,
        primary_workload_value: workload.primary_value,
        primary_workload_unit: workload.primary_unit,
        primary_workload_label: workload.primary_label.clone(),
        secondary_workload_value: workload.secondary_value,
        secondary_workload_unit: workload.secondary_unit,
        secondary_workload_label: workload.secondary_label.clone(),
        fastest_storage: best_elapsed_storage([
            string_measurement,
            rope_measurement,
            piece_tree_measurement,
        ]),
        lower_allocated_storage: best_allocated_storage([
            string_measurement,
            rope_measurement,
            piece_tree_measurement,
        ]),
        lower_peak_storage: best_peak_storage([
            string_measurement,
            rope_measurement,
            piece_tree_measurement,
        ]),
        piece_tree_elapsed_ratio_vs_rope: ratio_u128(
            piece_tree_measurement.elapsed_ns,
            rope_measurement.elapsed_ns,
        ),
        piece_tree_allocated_ratio_vs_rope: ratio_u64(
            piece_tree_measurement.metrics.allocated_bytes,
            rope_measurement.metrics.allocated_bytes,
        ),
        piece_tree_peak_ratio_vs_rope: ratio_u64(
            piece_tree_measurement.metrics.peak_live_bytes,
            rope_measurement.metrics.peak_live_bytes,
        ),
        piece_tree_elapsed_ratio_vs_string: ratio_u128(
            piece_tree_measurement.elapsed_ns,
            string_measurement.elapsed_ns,
        ),
        piece_tree_allocated_ratio_vs_string: ratio_u64(
            piece_tree_measurement.metrics.allocated_bytes,
            string_measurement.metrics.allocated_bytes,
        ),
        piece_tree_peak_ratio_vs_string: ratio_u64(
            piece_tree_measurement.metrics.peak_live_bytes,
            string_measurement.metrics.peak_live_bytes,
        ),
        piece_tree_wins_time_vs_rope: piece_tree_measurement.elapsed_ns
            < rope_measurement.elapsed_ns,
        piece_tree_wins_allocations_vs_rope: piece_tree_measurement.metrics.allocated_bytes
            < rope_measurement.metrics.allocated_bytes,
        piece_tree_wins_peak_memory_vs_rope: piece_tree_measurement.metrics.peak_live_bytes
            < rope_measurement.metrics.peak_live_bytes,
    };

    println!(
        "{}",
        serde_json::to_string(&comparison).expect("serialize phase0c comparison event")
    );
    let _ = std::io::stdout().flush();
    comparisons.push(comparison);
}

fn emit_measurement_event(workload: &WorkloadDescriptor, measurement: &Measurement) {
    let event = MeasurementEvent {
        event: "measurement",
        scenario: workload.scenario,
        scenario_label: workload.scenario_label,
        workload_family: workload.workload_family,
        step_index: workload.step_index,
        storage: measurement.storage,
        storage_impl: match measurement.storage {
            "string" => "std::string::String",
            "rope" => "ropey",
            "piece_tree" => "domain::PieceTreeLite(balanced internal nodes + char-aware metadata)",
            _ => "unknown",
        },
        primary_workload_value: workload.primary_value,
        primary_workload_unit: workload.primary_unit,
        primary_workload_label: workload.primary_label.clone(),
        secondary_workload_value: workload.secondary_value,
        secondary_workload_unit: workload.secondary_unit,
        secondary_workload_label: workload.secondary_label.clone(),
        elapsed_ns: measurement.elapsed_ns,
        allocated_bytes: measurement.metrics.allocated_bytes,
        deallocated_bytes: measurement.metrics.deallocated_bytes,
        live_bytes: measurement.metrics.live_bytes,
        peak_live_bytes: measurement.metrics.peak_live_bytes,
        allocation_count: measurement.metrics.allocation_count,
        deallocation_count: measurement.metrics.deallocation_count,
        reallocation_count: measurement.metrics.reallocation_count,
        result_value: measurement.result_value,
        result_unit: "items",
        result_label: format!("{} items", measurement.result_value),
        status: "ok",
        note: None,
    };

    println!(
        "{}",
        serde_json::to_string(&event).expect("serialize phase0c measurement event")
    );
    let _ = std::io::stdout().flush();
}

fn emit_decision_summary(comparisons: &[ComparisonEvent]) {
    let piece_tree_time_wins_vs_rope = comparisons
        .iter()
        .filter(|event| event.piece_tree_wins_time_vs_rope)
        .count();
    let piece_tree_allocation_wins_vs_rope = comparisons
        .iter()
        .filter(|event| event.piece_tree_wins_allocations_vs_rope)
        .count();
    let piece_tree_peak_wins_vs_rope = comparisons
        .iter()
        .filter(|event| event.piece_tree_wins_peak_memory_vs_rope)
        .count();
    let undo_history_piece_tree_beats_rope = comparisons
        .iter()
        .filter(|event| event.workload_family == "undo-history")
        .all(|event| event.piece_tree_wins_time_vs_rope);
    let preview_workloads_favor_rope = comparisons
        .iter()
        .filter(|event| event.workload_family == "search-preview")
        .all(|event| event.fastest_storage == "rope");
    let line_lookup_workloads_favor_rope = comparisons
        .iter()
        .filter(|event| event.workload_family == "line-lookup")
        .all(|event| event.fastest_storage == "rope");

    let summary = if undo_history_piece_tree_beats_rope
        && preview_workloads_favor_rope
        && line_lookup_workloads_favor_rope
    {
        "indexed piece-tree semantics beat rope on undo-heavy history, but rope still dominates the read-heavy preview and line-lookup workloads"
    } else if piece_tree_time_wins_vs_rope >= comparisons.len() / 2 {
        "indexed piece-tree storage is competitive enough with rope to justify deeper productization work"
    } else {
        "indexed piece-tree storage remains mixed against rope in this Phase 0c probe"
    };

    let event = DecisionEvent {
        event: "decision",
        chosen_piece_tree_probe_impl: "domain::PieceTreeLite(balanced internal nodes + char-aware metadata)",
        compared_rope_impl: "ropey",
        compared_string_impl: "std::string::String",
        comparison_count: comparisons.len(),
        undo_history_piece_tree_beats_rope,
        preview_workloads_favor_rope,
        line_lookup_workloads_favor_rope,
        piece_tree_time_wins_vs_rope,
        piece_tree_allocation_wins_vs_rope,
        piece_tree_peak_wins_vs_rope,
        summary,
    };

    println!(
        "{}",
        serde_json::to_string(&event).expect("serialize phase0c decision event")
    );
    let _ = std::io::stdout().flush();
}

fn run_string_load(path: &Path) -> usize {
    let text = std::fs::read_to_string(path).expect("read string load file");
    black_box(text.len())
}

fn run_rope_load(path: &Path) -> usize {
    let file = File::open(path).expect("open rope load file");
    let reader = BufReader::new(file);
    let rope = Rope::from_reader(reader).expect("read rope load file");
    black_box(rope.len_bytes())
}

fn run_piece_tree_load(path: &Path) -> usize {
    let text = std::fs::read_to_string(path).expect("read piece-tree load file");
    let piece_tree = PieceTreeLite::from_string(text);
    black_box(piece_tree.len_bytes())
}

fn run_string_undo_history(base_bytes: usize, edit_count: usize, insert_bytes: usize) -> usize {
    let mut text = plain_text_of_size(base_bytes);
    let inserted = plain_text_of_size(insert_bytes);
    let mut undo_log = Vec::with_capacity(edit_count);
    let mut seed = 0xC0FFEE_u64;

    for _ in 0..edit_count {
        seed = next_seed(seed);
        let offset = (seed as usize) % (text.len() + 1);
        text.insert_str(offset, inserted.as_str());
        undo_log.push((offset, inserted.len()));
    }

    while let Some((offset, len)) = undo_log.pop() {
        text.replace_range(offset..offset + len, "");
    }

    black_box(text.len())
}

fn run_rope_undo_history(base_bytes: usize, edit_count: usize, insert_bytes: usize) -> usize {
    let mut rope = Rope::from_str(plain_text_of_size(base_bytes).as_str());
    let inserted = plain_text_of_size(insert_bytes);
    let mut undo_log = Vec::with_capacity(edit_count);
    let mut seed = 0xC0FFEE_u64;

    for _ in 0..edit_count {
        seed = next_seed(seed);
        let offset = (seed as usize) % (rope.len_bytes() + 1);
        rope.insert(offset, inserted.as_str());
        undo_log.push((offset, inserted.len()));
    }

    while let Some((offset, len)) = undo_log.pop() {
        rope.remove(offset..offset + len);
    }

    black_box(rope.len_bytes())
}

fn run_piece_tree_undo_history(base_bytes: usize, edit_count: usize, insert_bytes: usize) -> usize {
    let mut piece_tree = PieceTreeLite::from_string(plain_text_of_size(base_bytes));
    let inserted = plain_text_of_size(insert_bytes);
    let mut undo_log = Vec::with_capacity(edit_count);
    let mut seed = 0xC0FFEE_u64;

    for _ in 0..edit_count {
        seed = next_seed(seed);
        let offset = (seed as usize) % (piece_tree.len_bytes() + 1);
        piece_tree.insert(offset, inserted.as_str());
        undo_log.push((offset, inserted.len()));
    }

    while let Some((offset, len)) = undo_log.pop() {
        piece_tree.remove_range(offset..offset + len);
    }

    black_box(piece_tree.len_bytes())
}

fn measure(run_label: &'static str, run: impl FnOnce() -> usize) -> Measurement {
    reset_allocation_counters();
    let start = Instant::now();
    let result = catch_unwind(AssertUnwindSafe(run));
    let elapsed_ns = start.elapsed().as_nanos();
    let metrics = allocation_snapshot();
    let result_value = match result {
        Ok(value) => value,
        Err(payload) => panic!(
            "{run_label} measurement panicked: {}",
            panic_message(payload)
        ),
    };

    Measurement {
        storage: run_label,
        elapsed_ns,
        metrics,
        result_value,
    }
}

fn ascii_string_preview_for_match(text: &str, range: &Range<usize>) -> (usize, usize, String) {
    let safe_start = range.start.min(text.len());
    let safe_end = range.end.min(text.len());

    let mut line_number = 1usize;
    let mut line_start = 0usize;
    for (index, byte) in text.as_bytes().iter().enumerate() {
        if index >= safe_start {
            break;
        }
        if *byte == b'\n' {
            line_number += 1;
            line_start = index + 1;
        }
    }

    let column_number = safe_start.saturating_sub(line_start) + 1;
    let mut line_end = safe_end;
    while line_end < text.len() && text.as_bytes()[line_end] != b'\n' {
        line_end += 1;
    }

    let preview = compact_preview(&text[line_start..line_end]);
    (line_number, column_number, preview)
}

fn rope_preview_for_match(rope: &Rope, range: &Range<usize>) -> (usize, usize, String) {
    let safe_start = range.start.min(rope.len_chars());
    let line_index = rope.char_to_line(safe_start);
    let line_start = rope.line_to_char(line_index);
    let line_text = rope
        .line(line_index)
        .to_string()
        .trim_end_matches('\n')
        .to_owned();
    let preview = compact_preview(&line_text);
    (
        line_index + 1,
        safe_start.saturating_sub(line_start) + 1,
        preview,
    )
}

fn ascii_string_line_lookup(text: &str, target_line: usize) -> (usize, usize) {
    let mut current_line = 0usize;
    let mut current_line_start = 0usize;
    let mut current_line_len = 0usize;

    for (index, byte) in text.as_bytes().iter().enumerate() {
        if current_line == target_line {
            if *byte == b'\n' {
                return (current_line_start, current_line_len);
            }
            current_line_len += 1;
        } else if *byte == b'\n' {
            current_line += 1;
            current_line_start = index + 1;
            current_line_len = 0;
        }
    }

    (current_line_start, current_line_len)
}

fn rope_line_lookup(rope: &Rope, target_line: usize) -> (usize, usize) {
    let safe_line = target_line.min(rope.len_lines().saturating_sub(1));
    let line_start = rope.line_to_char(safe_line);
    let line_len = rope
        .line(safe_line)
        .to_string()
        .trim_end_matches('\n')
        .len();
    (line_start, line_len)
}

fn compact_preview(line_text: &str) -> String {
    let trimmed = line_text.trim();
    let trimmed_chars = trimmed.chars().collect::<Vec<_>>();
    if trimmed_chars.len() <= PREVIEW_MAX_CHARS {
        return trimmed.to_owned();
    }

    let mut preview = trimmed_chars[..PREVIEW_MAX_CHARS]
        .iter()
        .collect::<String>();
    preview.push_str("...");
    preview
}

fn preview_probe_corpus(target_bytes: usize) -> ProbeCorpus {
    let filler = "abcdefghijklmnopqrstuvwxyz0123456789 filler filler filler filler filler filler\n";
    let marker_line =
        "preview context left left left needle right right right with enough trailing text\n";
    let near_end_line = "tail lookup line after preview marker for near-end line retrieval\n";
    let mut text = String::with_capacity(target_bytes + filler.len() + marker_line.len());
    let marker_insert_at = target_bytes.saturating_mul(9) / 10;
    let mut marker_inserted = false;

    while text.len() + filler.len() < target_bytes {
        if !marker_inserted && text.len() >= marker_insert_at {
            text.push_str(marker_line);
            marker_inserted = true;
        } else {
            text.push_str(filler);
        }
    }

    if !marker_inserted {
        text.push_str(marker_line);
    }

    for _ in 0..4 {
        text.push_str(near_end_line);
    }

    let match_start = text.find(MARKER).expect("marker present in preview corpus");
    let match_range = match_start..match_start + MARKER.len();
    let total_lines = count_newlines(text.as_bytes()) + 1;
    let near_end_line_index = total_lines.saturating_sub(3);

    ProbeCorpus {
        text,
        match_range,
        near_end_line_index,
    }
}

fn write_plain_text_file(path: &Path, target_bytes: usize) -> std::io::Result<()> {
    let line = b"The quick brown fox jumps over the lazy dog 0123456789.\n";
    let repeats = (target_bytes / line.len()).max(1);
    let mut file = File::create(path)?;
    for _ in 0..repeats {
        file.write_all(line)?;
    }
    file.flush()
}

fn plain_text_of_size(target_bytes: usize) -> String {
    let line = "The quick brown fox jumps over the lazy dog 0123456789.\n";
    let repeats = (target_bytes / line.len()).max(1);
    let mut text = String::with_capacity(repeats * line.len());
    for _ in 0..repeats {
        text.push_str(line);
    }
    debug_assert!(text.is_ascii());
    text
}

fn best_elapsed_storage(measurements: [&Measurement; 3]) -> &'static str {
    measurements
        .into_iter()
        .min_by_key(|measurement| measurement.elapsed_ns)
        .expect("measurements present")
        .storage
}

fn best_allocated_storage(measurements: [&Measurement; 3]) -> &'static str {
    measurements
        .into_iter()
        .min_by_key(|measurement| measurement.metrics.allocated_bytes)
        .expect("measurements present")
        .storage
}

fn best_peak_storage(measurements: [&Measurement; 3]) -> &'static str {
    measurements
        .into_iter()
        .min_by_key(|measurement| measurement.metrics.peak_live_bytes)
        .expect("measurements present")
        .storage
}

fn count_newlines(bytes: &[u8]) -> usize {
    bytes.iter().filter(|byte| **byte == b'\n').count()
}

fn ratio_u128(numerator: u128, denominator: u128) -> f64 {
    if denominator == 0 {
        return 1.0;
    }
    numerator as f64 / denominator as f64
}

fn ratio_u64(numerator: u64, denominator: u64) -> Option<f64> {
    (denominator != 0).then_some(numerator as f64 / denominator as f64)
}

fn next_seed(seed: u64) -> u64 {
    seed.wrapping_mul(6364136223846793005).wrapping_add(1)
}

fn unique_probe_root(label: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock before unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "scratchpad-piece-tree-phase0c-{label}-{}-{nanos}",
        std::process::id()
    ))
}

fn human_bytes(value: usize) -> String {
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
    let live = LIVE_BYTES.fetch_add(bytes, Ordering::Relaxed) + bytes;
    update_peak_live(live);
}

fn record_deallocation(bytes: u64) {
    DEALLOCATED_BYTES.fetch_add(bytes, Ordering::Relaxed);
    DEALLOCATION_COUNT.fetch_add(1, Ordering::Relaxed);
    LIVE_BYTES.fetch_sub(bytes, Ordering::Relaxed);
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
