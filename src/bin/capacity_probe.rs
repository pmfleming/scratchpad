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
use scratchpad::profile::process_metrics::{ProcessMeasurement, ProcessSnapshot, process_snapshot};
use scratchpad::profile::{
    GB, KB, MB, human_bytes, panic_payload_message as panic_message,
    run_many_file_first_visible_profile,
};
use serde::Serialize;
use std::hint::black_box;
use std::io::Write;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::time::Instant;

const TAB_BYTES_PER_BUFFER: usize = 48 * KB;
const SPLIT_BYTES_PER_TILE: usize = 128 * KB;
const VIEW_COUNT_BUFFER_BYTES: usize = MB;
const MEASUREMENT_REPETITIONS: usize = 3;

#[derive(Serialize)]
struct CapacityEvent {
    scenario: &'static str,
    scenario_label: &'static str,
    workload_family: &'static str,
    step_index: usize,
    repeat_index: usize,
    expected_repetition_count: usize,
    workload_value: usize,
    workload_unit: &'static str,
    workload_label: String,
    setup_elapsed_ns: u128,
    elapsed_ns: u128,
    background_completion_ns: Option<u128>,
    completed_workload_value: Option<usize>,
    measurement_scope: &'static str,
    measurement_contract: serde_json::Value,
    metrics: CapacityMetricsSnapshot,
    memory_budget: MemoryBudgetSnapshot,
    process_resources: ProcessMeasurement,
    working_set_bytes: Option<u64>,
    peak_working_set_bytes: Option<u64>,
    handle_count: Option<u64>,
    status: &'static str,
    note: Option<String>,
}

#[derive(Clone, Serialize)]
struct StepDescriptor {
    scenario: &'static str,
    scenario_label: &'static str,
    workload_family: &'static str,
    step_index: usize,
    workload_value: usize,
    workload_unit: &'static str,
    workload_label: String,
}

struct RecordedTiming {
    elapsed_ns: u128,
    background_completion_ns: Option<u128>,
    completed_workload_value: Option<usize>,
    process_before: ProcessSnapshot,
}

struct SweepDescriptor {
    scenario: &'static str,
    scenario_label: &'static str,
    workload_family: &'static str,
    workload_unit: &'static str,
    workload_label: fn(usize) -> String,
}

thread_local! { static CAPACITY_STORE: std::cell::RefCell<Option<scratchpad::app::services::session_store::SessionStore>> = const {std::cell::RefCell::new(None)}; }
fn read_stored_file(path: &Path) -> scratchpad::app::services::file_service::FileContent {
    CAPACITY_STORE.with(|slot| {
        let store = slot.borrow();
        let store = store.as_ref().unwrap();
        let file =
            FileService::read_file_in_store(path, store.chunk_store_handle().unwrap()).unwrap();
        store.flush_text_chunks().unwrap();
        file
    })
}

fn contract(scenario: &str) -> serde_json::Value {
    let (backing, production, path) = match scenario {
        "large_file_background_index_ceiling" => ("store", true, "FileService::read_file_in_store"),
        "large_file_first_visible_ceiling" => {
            ("bounded_window", true, "open_path_to_first_tessellation")
        }
        "many_file_first_visible_ceiling" | "many_file_background_hydration_ceiling" => (
            "owned",
            true,
            "FileController::open_paths_async/poll_background_io",
        ),
        "search_target_count_ceiling" => {
            ("owned", true, "ScratchpadApp::set_search_query/poll_search")
        }
        "search_file_size_ceiling" => (
            "store",
            true,
            "DocumentSnapshot::chunks_for_range/search_program",
        ),
        "paste_size_ceiling" => ("store", true, "ScratchpadApp::raw_input_hook/logic/ui"),
        _ => ("owned", false, "prepared_component"),
    };
    scratchpad::profile::fixtures::contract(backing, production, path)
}

fn main() {
    let directory = tempfile::tempdir().unwrap();
    CAPACITY_STORE.with(|slot| {
        *slot.borrow_mut() = Some(scratchpad::app::services::session_store::SessionStore::new(
            directory.path().join("store"),
        ))
    });
    let args: Vec<_> = std::env::args().collect();
    let selection = args
        .windows(2)
        .find(|p| p[0] == "--scenario")
        .map(|p| p[1].as_str());
    type CapacitySweep = (&'static [&'static str], fn());
    let sweeps: &[CapacitySweep] = &[
        (
            &[
                "large_file_first_visible_ceiling",
                "large_file_background_index_ceiling",
            ],
            emit_large_file_first_visible_sweep,
        ),
        (&["in_memory_text_ingest_ceiling"], emit_file_size_sweep),
        (&["text_layout_ceiling"], emit_text_layout_sweep),
        (
            &[
                "many_file_first_visible_ceiling",
                "many_file_background_hydration_ceiling",
            ],
            emit_many_file_first_visible_sweep,
        ),
        (&["search_file_size_ceiling"], emit_search_file_size_sweep),
        (
            &["search_target_count_ceiling"],
            emit_search_target_count_sweep,
        ),
        (&["tab_count_ceiling"], emit_tab_count_sweep),
        (&["split_count_ceiling"], emit_split_count_sweep),
        (&["view_count_ceiling"], emit_view_count_sweep),
        (&["paste_size_ceiling"], emit_paste_size_sweep),
    ];
    assert!(
        selection.is_none_or(|id| sweeps.iter().any(|(ids, _)| ids.contains(&id))),
        "unknown capacity scenario"
    );
    for (ids, run) in sweeps {
        if selection.is_none_or(|id| ids.contains(&id)) {
            run();
        }
    }
    CAPACITY_STORE.with(|slot| drop(slot.borrow_mut().take()));
}

fn selected(scenario: &str) -> bool {
    let args: Vec<_> = std::env::args().collect();
    args.windows(2)
        .find(|p| p[0] == "--scenario")
        .is_none_or(|p| p[1] == scenario)
}

fn started(step: &StepDescriptor, repeat_index: usize) {
    if !selected(step.scenario) {
        return;
    }
    let mut event = serde_json::to_value(step).unwrap();
    event["event"] = serde_json::json!("step_started");
    event["repeat_index"] = serde_json::json!(repeat_index);
    event["expected_repetition_count"] = serde_json::json!(MEASUREMENT_REPETITIONS);
    println!("{event}");
    std::io::stdout().flush().unwrap();
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
            if selected(descriptor.scenario) {
                started(
                    &StepDescriptor {
                        scenario: descriptor.scenario,
                        scenario_label: descriptor.scenario_label,
                        workload_family: descriptor.workload_family,
                        step_index,
                        workload_value: file_bytes,
                        workload_unit: descriptor.workload_unit,
                        workload_label: (descriptor.workload_label)(file_bytes),
                    },
                    repeat_index,
                );
                reset_capacity_metrics();
                memory_budget::reset();
                let process_before = process_snapshot();
                let (elapsed_ns, primitives) =
                    scratchpad::profile::run_file_first_render_preparation(&path);
                black_box(primitives);
                emit_recorded_step(
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
                RecordedTiming {
                    elapsed_ns,
                    background_completion_ns: None,
                    completed_workload_value: None,
                    process_before,
                },
                "open_path_to_first_tessellation",
                Some(
                    "Warm OS cache fixture; app setup and teardown excluded; GPU/present excluded"
                        .into(),
                ),
            );
            }
            let fresh_import = selected("large_file_background_index_ceiling")
                .then(|| tempfile::tempdir().unwrap());
            let previous_store = fresh_import.as_ref().map(|directory| {
                CAPACITY_STORE.with(|slot| {
                    slot.replace(Some(
                        scratchpad::app::services::session_store::SessionStore::new(
                            directory.path().join("store"),
                        ),
                    ))
                })
            });
            emit_prepared_step(
                StepDescriptor {
                    scenario: "large_file_background_index_ceiling",
                    scenario_label: "Large-file background indexing sweep",
                    workload_family: "file-load",
                    step_index,
                    workload_value: file_bytes,
                    workload_unit: descriptor.workload_unit,
                    workload_label: (descriptor.workload_label)(file_bytes),
                },
                repeat_index,
                setup_elapsed_ns,
                || {
                    let content = read_stored_file(&path);
                    black_box(
                        content.document.piece_tree().len_bytes()
                            + content.document.piece_tree().metrics().newlines,
                    )
                },
            );
            if let Some(previous) = previous_store {
                CAPACITY_STORE.with(|slot| drop(slot.replace(previous)));
            }
        }
    }

    let _ = std::fs::remove_dir_all(root);
}

fn emit_file_size_sweep() {
    emit_prepared_sweep(
        SweepDescriptor::bytes(
            "in_memory_text_ingest_ceiling",
            "In-memory text ingest diagnostic sweep",
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
    let background = selected("many_file_background_hydration_ceiling")
        && !selected("many_file_first_visible_ceiling");
    let descriptor = SweepDescriptor::count(
        if background {
            "many_file_background_hydration_ceiling"
        } else {
            "many_file_first_visible_ceiling"
        },
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
            let process_before = process_snapshot();
            started(
                &StepDescriptor {
                    scenario: descriptor.scenario,
                    scenario_label: descriptor.scenario_label,
                    workload_family: descriptor.workload_family,
                    step_index,
                    workload_value: file_count,
                    workload_unit: descriptor.workload_unit,
                    workload_label: (descriptor.workload_label)(file_count),
                },
                repeat_index,
            );
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
                RecordedTiming {
                    elapsed_ns: if background {
                        profile.background_completion_ns
                    } else {
                        profile.first_visible_ns
                    },
                    background_completion_ns: Some(profile.background_completion_ns),
                    completed_workload_value: Some(profile.installed_file_count),
                    process_before: process_before.clone(),
                },
                if background {
                    "production_open_paths_background_completion"
                } else {
                    "open_paths_to_first_tessellation_before_background_completion"
                },
                Some(format!(
                    "background completion {:.3} ms; {} tabs installed",
                    profile.background_completion_ns as f64 / 1_000_000.0,
                    profile.tab_count_after_completion
                )),
            );
            if selected("many_file_first_visible_ceiling")
                && selected("many_file_background_hydration_ceiling")
            {
                emit_recorded_step(
                    StepDescriptor {
                        scenario: "many_file_background_hydration_ceiling",
                        scenario_label: "Production many-file background completion",
                        workload_family: "many-files",
                        step_index,
                        workload_value: file_count,
                        workload_unit: "files",
                        workload_label: files_label(file_count),
                    },
                    repeat_index,
                    setup_elapsed_ns,
                    RecordedTiming {
                        elapsed_ns: profile.background_completion_ns,
                        background_completion_ns: None,
                        completed_workload_value: Some(profile.installed_file_count),
                        process_before,
                    },
                    "production_open_paths_background_completion",
                    None,
                );
            }
        }
    }

    let _ = std::fs::remove_dir_all(root);
}

fn emit_search_file_size_sweep() {
    let root = tempfile::tempdir().unwrap();
    for (step_index, bytes) in [MB, 64 * MB, 256 * MB, GB].into_iter().enumerate() {
        let path = root.path().join(format!("search-{bytes}.txt"));
        std::fs::write(&path, search_text_of_size(bytes)).unwrap();
        let document = read_stored_file(&path).document;
        let snapshot = document.snapshot();
        let program = search_capacity_program();
        for repeat_index in 0..MEASUREMENT_REPETITIONS {
            emit_prepared_step(
                StepDescriptor {
                    scenario: "search_file_size_ceiling",
                    scenario_label: "Store-backed search completion",
                    workload_family: "search",
                    step_index,
                    workload_value: bytes,
                    workload_unit: "bytes",
                    workload_label: human_bytes(bytes),
                },
                repeat_index,
                0,
                || {
                    let mut matches = 0;
                    for chunk in snapshot.chunks_for_range(0..snapshot.len_chars(), 256 * KB, 5, 5)
                    {
                        let (text, offset) = snapshot.search_text_cow(Some(chunk.window_range));
                        matches += search_program(&text, &program)
                            .matches
                            .into_iter()
                            .filter(|range| chunk.core_range.contains(&(offset + range.start)))
                            .count();
                    }
                    assert_eq!(matches, 1);
                    matches
                },
            );
        }
    }
}

fn emit_search_target_count_sweep() {
    for (step_index, count) in [100, 1_000, 10_000].into_iter().enumerate() {
        for repeat_index in 0..MEASUREMENT_REPETITIONS {
            let step = StepDescriptor {
                scenario: "search_target_count_ceiling",
                scenario_label: "Production all-tabs search completion",
                workload_family: "search",
                step_index,
                workload_value: count,
                workload_unit: "files",
                workload_label: files_label(count),
            };
            started(&step, repeat_index);
            let before = process_snapshot();
            let profile = scratchpad::profile::search_workflow_sample("all", count, 4 * KB);
            emit_recorded_step(
                step,
                repeat_index,
                profile["setup_elapsed_ns"].as_u64().unwrap() as u128,
                RecordedTiming {
                    elapsed_ns: profile["completion"].as_u64().unwrap() as u128,
                    background_completion_ns: None,
                    completed_workload_value: None,
                    process_before: before,
                },
                "query_command_to_completion",
                None,
            );
        }
    }
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
    for (step_index, bytes) in [8 * MB, 64 * MB, 128 * MB].into_iter().enumerate() {
        for repeat_index in 0..MEASUREMENT_REPETITIONS {
            let step = StepDescriptor {
                scenario: "paste_size_ceiling",
                scenario_label: "Production paste through publication",
                workload_family: "edit-paste",
                step_index,
                workload_value: bytes,
                workload_unit: "bytes",
                workload_label: human_bytes(bytes),
            };
            started(&step, repeat_index);
            let before = process_snapshot();
            let profile = scratchpad::profile::run_live_text_paste_profile(bytes);
            emit_recorded_step(step, repeat_index, 0, RecordedTiming {
                elapsed_ns: profile["publication_ns"].as_u64().unwrap() as u128,
                background_completion_ns: None, completed_workload_value: None, process_before: before,
            }, "production_paste_to_publication", Some("Raw input, background preparation, publication, layout and tessellation; no injected writer pause".into()));
        }
    }
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
    if !selected(step.scenario) {
        return;
    }
    let scope = match step.scenario {
        "large_file_background_index_ceiling" => "warm_cache_file_index_completion",
        "search_file_size_ceiling" | "search_target_count_ceiling" => {
            "prepared_search_component_completion"
        }
        _ => "prepared_operation",
    };
    emit_measured_step(step, repeat_index, setup_elapsed_ns, scope, run);
}

fn emit_recorded_step(
    step: StepDescriptor,
    repeat_index: usize,
    setup_elapsed_ns: u128,
    timing: RecordedTiming,
    measurement_scope: &'static str,
    note: Option<String>,
) {
    let RecordedTiming {
        elapsed_ns,
        background_completion_ns,
        completed_workload_value,
        process_before,
    } = timing;
    let process_after = process_snapshot();
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
        completed_workload_value,
        measurement_scope,
        measurement_contract: contract(step.scenario),
        metrics: capacity_metrics_snapshot(),
        memory_budget: memory_budget::snapshot(),
        working_set_bytes: process_after.working_set_bytes,
        peak_working_set_bytes: process_after.peak_working_set_bytes,
        handle_count: process_after.handle_count,
        process_resources: ProcessMeasurement::between(process_before, process_after),
        status: "ok",
        expected_repetition_count: MEASUREMENT_REPETITIONS,
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
    started(&step, repeat_index);
    reset_capacity_metrics();
    memory_budget::reset();
    let process_before = process_snapshot();
    let start = Instant::now();
    let result = catch_unwind(AssertUnwindSafe(run));
    let elapsed_ns = start.elapsed().as_nanos();
    let metrics = capacity_metrics_snapshot();
    let memory_budget_snapshot = memory_budget::snapshot();
    let process_after = process_snapshot();
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
        completed_workload_value: None,
        measurement_scope,
        measurement_contract: contract(step.scenario),
        metrics,
        memory_budget: memory_budget_snapshot,
        working_set_bytes: process_after.working_set_bytes,
        peak_working_set_bytes: process_after.peak_working_set_bytes,
        handle_count: process_after.handle_count,
        process_resources: ProcessMeasurement::between(process_before, process_after),
        status,
        expected_repetition_count: MEASUREMENT_REPETITIONS,
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

fn build_tabs(tab_count: usize, bytes_per_buffer: usize) -> Vec<WorkspaceTab> {
    (0..tab_count)
        .map(|index| {
            let buffer = BufferState::new(
                format!("tab_{index}.txt"),
                scratchpad::profile::fixtures::text(bytes_per_buffer, 0x7192_a335 ^ index as u64),
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
    scratchpad::profile::fixtures::text(target_bytes, 0x7192_a335)
}
fn exact_utf8_text_of_size(target_bytes: usize) -> String {
    utf8_text_of_size(target_bytes)
}
fn write_large_text_fixture(path: &Path, target_bytes: usize, _chunk: &str) {
    scratchpad::profile::fixtures::write(path, target_bytes).unwrap();
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
    let mut text = utf8_text_of_size(target_bytes.saturating_sub("needle café 東京\n".len()));
    text.push_str("needle café 東京\n");
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
