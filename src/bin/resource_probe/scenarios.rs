use super::events::{
    StepDescriptor, StepOutcome, WorkloadSpec, emit_step, emit_workload_steps, human_bytes,
};
use super::*;

pub(super) fn run_all() {
    emit_large_utf8_load_peak_memory();
    emit_file_backed_open_allocations();
    emit_edited_buffer_search_preview_rendering();
    emit_provenance_retained_memory();
    emit_anchor_heavy_view_editing();
    emit_fragmented_long_session_mutations();
    emit_many_file_resource_tracking();
    emit_many_file_lazy_open_tracking();
    emit_search_resource_tracking();
    emit_paste_allocations();
    emit_tab_count_resource_tracking();
    emit_targeted_tab_phase_probes();
    emit_tab_strip_frame_tracking();
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

    emit_workload_steps(
        [128usize, 1_000],
        WorkloadSpec {
            scenario: "search_app_result_tracking",
            scenario_label: "Search app result storage tracking",
            workload_family: "search",
            focus: "result-storage",
            workload_unit: "tabs",
        },
        run_search_app_result_cycle,
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

fn emit_many_file_lazy_open_tracking() {
    let root = unique_probe_root("many-file-lazy-open");
    std::fs::create_dir_all(&root).expect("create many-file lazy-open root");

    for (step_index, file_count) in [1_000usize, 10_000].into_iter().enumerate() {
        let step_root = root.join(format!("files_{file_count}"));
        std::fs::create_dir_all(&step_root).expect("create many-file lazy-open step root");
        let paths = (0..file_count)
            .map(|index| {
                let path = step_root.join(format!("lazy_{index}.txt"));
                std::fs::write(&path, "lazy open probe\n").expect("write lazy-open probe file");
                path
            })
            .collect::<Vec<_>>();
        emit_step(
            StepDescriptor {
                scenario: "many_file_lazy_open_tracking",
                scenario_label: "Many-file lazy open tracking",
                workload_family: "many-files",
                focus: "lazy-open",
                step_index,
                workload_value: file_count,
                workload_unit: "files",
                workload_label: format!("{file_count} files"),
            },
            || run_many_file_lazy_open_cycle(&paths),
        );
    }

    let _ = std::fs::remove_dir_all(root);
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

fn emit_targeted_tab_phase_probes() {
    for (step_index, tab_count) in [128usize, 512, 4_096, 10_000].into_iter().enumerate() {
        emit_step(
            StepDescriptor {
                scenario: "tab_build_targeted",
                scenario_label: "Tab build targeted path",
                workload_family: "tab-management",
                focus: "tab-build",
                step_index,
                workload_value: tab_count,
                workload_unit: "tabs",
                workload_label: format!("{tab_count} tabs"),
            },
            || StepOutcome::items(black_box(build_tabs(tab_count, TAB_BYTES_PER_BUFFER).len())),
        );

        let mut split_tabs = build_tabs(tab_count, TAB_BYTES_PER_BUFFER);
        emit_step(
            StepDescriptor {
                scenario: "tab_split_targeted",
                scenario_label: "Tab split targeted path",
                workload_family: "tab-management",
                focus: "tab-split",
                step_index,
                workload_value: tab_count,
                workload_unit: "tabs",
                workload_label: format!("{tab_count} tabs"),
            },
            || StepOutcome::items(black_box(split_tabs_once(&mut split_tabs))),
        );

        let mut combine_tabs_set = build_tabs(tab_count, TAB_BYTES_PER_BUFFER);
        split_tabs_once(&mut combine_tabs_set);
        emit_step(
            StepDescriptor {
                scenario: "tab_combine_targeted",
                scenario_label: "Tab combine targeted path",
                workload_family: "tab-management",
                focus: "tab-combine",
                step_index,
                workload_value: tab_count,
                workload_unit: "tabs",
                workload_label: format!("{tab_count} tabs"),
            },
            || StepOutcome::items(black_box(combine_first_tabs(&mut combine_tabs_set))),
        );
    }
}

fn emit_tab_strip_frame_tracking() {
    emit_workload_steps(
        [128usize, 1_000, 10_000],
        WorkloadSpec {
            scenario: "tab_strip_frame_rendering",
            scenario_label: "Tab strip frame rendering",
            workload_family: "tab-management",
            focus: "tab-strip-frame",
            workload_unit: "tabs",
        },
        run_tab_strip_frame_cycle,
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
