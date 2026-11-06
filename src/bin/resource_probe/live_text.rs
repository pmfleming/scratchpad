//! Prepared production-path baselines missing from the initial stage 0 run.
use super::events::{StepDescriptor, emit_prepared_operation};
use super::*;
use scratchpad::app::services::session_store::SessionMaintenanceFixture;
use std::time::Instant;

pub(super) fn run() {
    let directory = tempfile::tempdir().expect("isolated live-text baselines");
    let path = directory.path().join("large.txt");
    write_utf8_text_file(&path, 64 * MB).expect("large Unicode file");
    let document = read_probe_file(&path)
        .expect("file-backed document")
        .document;
    super::FILE_PROBE_STORE
        .with(|slot| slot.borrow().as_ref().unwrap().flush_text_chunks().unwrap());
    let storage_configuration = super::FILE_PROBE_STORE.with(|slot| {
        slot.borrow()
            .as_ref()
            .unwrap()
            .profile_storage_configuration()
            .unwrap()
    });
    let snapshot = document.snapshot();
    let program = search_capacity_program();
    for index in 0..3 {
        emit_prepared_operation(
            descriptor("file_backed_search", index, 64 * MB),
            serde_json::json!({"scope":"production snapshot chunk windows and search matcher; single-thread scan", "query":"needle", "storage_configuration":storage_configuration}),
            || {
                let mut matches = 0;
                for chunk in snapshot.chunk_iter_for_range(0..snapshot.len_chars(), 256 * KB, 5, 5)
                {
                    let (text, offset) = snapshot.search_text_cow(Some(chunk.window_range));
                    matches += search_program(&text, &program)
                        .matches
                        .into_iter()
                        .filter(|range| chunk.core_range.contains(&(offset + range.start)))
                        .count();
                }
                assert_eq!(matches, 0, "hex fixture has no needle");
                StepOutcome::items(matches)
            },
        );
    }
    let mut fixture = SessionMaintenanceFixture::new(
        directory.path().join("typing"),
        session_save::fixture(50 * MB),
        0,
    )
    .expect("typing fixture");
    for saving in [false, true] {
        for index in 0..3 {
            emit_prepared_operation(
                descriptor(
                    if saving {
                        "typing_during_save"
                    } else {
                        "typing_without_save"
                    },
                    index,
                    50 * MB,
                ),
                serde_json::json!({"saving":saving,"samples":128,"scope":"capture excluded; mutation plus UI storage-health query"}),
                || {
                    let timer = Instant::now();
                    let (mut times, save_ns) =
                        fixture.typing_sample(saving).expect("typing/sample");
                    times.sort_unstable();
                    let mut result = StepOutcome::items(times.len());
                    result.phase_elapsed_ns = serde_json::json!({"typing_p50_ns":times[times.len()/2],"typing_p99_ns":times[(times.len()*99).div_ceil(100)-1],"typing_max_ns":times.last(),"save_ns":save_ns,"sample_ns":timer.elapsed().as_nanos()});
                    result
                },
            );
        }
    }
}

fn descriptor(scenario: &'static str, index: usize, bytes: usize) -> StepDescriptor {
    StepDescriptor {
        scenario,
        scenario_label: scenario,
        workload_family: "store-workflows",
        focus: "concurrency",
        step_index: index,
        workload_value: bytes,
        workload_unit: "bytes",
        workload_label: format!("{} MiB", bytes / MB),
    }
}

/// Production import, first-window paint, verified reads and incremental writes.
pub(super) fn imports() {
    for size in [64 * MB, 2 * GB] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("large.txt");
        write_utf8_text_file(&path, size).unwrap();
        let first = Instant::now();
        let paint = run_file_backed_open_first_visible_paint_cycle(&path);
        let first_paint_ns = first.elapsed().as_nanos();
        let session = SessionStore::new(directory.path().join("session"));
        let store = session.chunk_store_handle().unwrap();
        let start = Instant::now();
        let file = FileService::read_file_in_store(&path, store.clone()).unwrap();
        let import_ns = start.elapsed().as_nanos();
        let state = FileService::read_disk_state(&path).ok();
        let buffer = FileService::build_buffer_from_file_content(&path, file, state);
        let mut tabs = vec![WorkspaceTab::new(buffer)];
        session.persist(&tabs, 0, 14., false).unwrap();
        store.set_limits(0, 0);
        store.set_limits(16 * MB, 4 * MB);
        let tree = tabs[0].active_buffer().document().piece_tree();
        let mut reads = Vec::new();
        for index in 0..256 {
            let position = (index * (tree.len_chars() - 1)) / 256;
            let tick = Instant::now();
            assert!(
                !tree
                    .try_spans_for_range(position..position + 1)
                    .next()
                    .unwrap()
                    .unwrap()
                    .text
                    .is_empty()
            );
            reads.push(tick.elapsed().as_nanos());
        }
        reads.sort_unstable();
        let before = store.hashed_bytes();
        session_save::insert(tabs[0].active_buffer_mut(), 0, "one edit λ");
        let profile = session.persist_profiled(&tabs, 0, 14., false).unwrap();
        let hashed_new = store.hashed_bytes() - before;
        assert!(hashed_new <= (256 * KB) as u64);
        assert!(store.peak_pending_bytes() <= 32 * MB + 256 * KB);
        println!(
            "{}",
            serde_json::json!({"scenario":"store_import","bytes":size,"first_paint_ns":first_paint_ns,"first_paint_result":paint.result_value,"import_ns":import_ns,"read_p99_ns":reads[253],"read_max_ns":reads[255],"peak_pending_bytes":store.peak_pending_bytes(),"cached_bytes":store.cached_bytes(),"new_body_bytes_hashed_after_edit":hashed_new,"edit_save_ns":profile.total_ns})
        );
    }
}

/// Standard lens rows for import cost and responsiveness against real writes.
pub(super) fn storage_workflows() {
    if super::selected("production_paste") {
        for (index, bytes) in [64 * MB, 128 * MB].into_iter().enumerate() {
            emit_prepared_operation(
                descriptor("production_paste", index, bytes),
                serde_json::json!({"writer_pause_ms":0}),
                || {
                    let profile = scratchpad::profile::run_live_text_paste_profile(bytes);
                    let mut outcome =
                        StepOutcome::items(profile["frames"].as_u64().unwrap() as usize);
                    outcome.phase_elapsed_ns = profile;
                    outcome
                },
            );
        }
    }

    if super::selected("store_import") {
        for (index, bytes) in [64 * MB, GB].into_iter().enumerate() {
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("import.txt");
            write_utf8_text_file(&path, bytes).unwrap();
            emit_prepared_operation(
                descriptor("store_import", index, bytes),
                serde_json::json!({"fresh_store":true}),
                || run_large_utf8_load_cycle(&path),
            );
        }
    }
    if super::selected("ui_during_store_writes") {
        emit_prepared_operation(
            descriptor("ui_during_store_writes", 0, 64 * MB),
            serde_json::json!({"writer_pause_ms":200,"seed_bytes":3_u64 * GB as u64}),
            || {
                let profile = scratchpad::profile::run_live_text_io_profile(64 * MB);
                assert!(profile["queued_typing_preserved"] == true);
                assert!(profile["concurrent_write_bytes"].as_u64().unwrap() >= 3_u64 * GB as u64);
                let mut outcome = StepOutcome::items(profile["frames"].as_u64().unwrap() as usize);
                outcome.phase_elapsed_ns = profile;
                outcome
            },
        );
    }
}

pub(super) fn aged_storage() {
    let ids = [
        "cold_reads_during_save",
        "aged_store_save",
        "aged_store_restore",
        "aged_store_close",
        "aged_store_forget",
    ];
    if !super::any_selected(&ids) {
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let mut fixture = SessionMaintenanceFixture::new(
        root.path().join("aged"),
        session_save::fixture(50 * MB),
        128,
    )
    .unwrap();
    fixture.seed_aged(3 * GB, 128).unwrap();
    let database_bytes = fixture.database_bytes();
    let storage_configuration = fixture.storage_configuration().unwrap();
    assert!(database_bytes >= 3 * GB as u64);
    for id in ids {
        if !super::selected(id) {
            continue;
        }
        let repeats = if id == "cold_reads_during_save" { 1 } else { 3 };
        for index in 0..repeats {
            // Each forget starts with a new recorded edit, so it cannot become a no-op.
            if id == "aged_store_forget" {
                fixture.prepare("incremental_save").unwrap();
            }
            emit_prepared_operation(
                descriptor(id, index, 3 * GB),
                serde_json::json!({"database_bytes":database_bytes,"seed_bytes":3 * GB,
                    "history_revisions":128,"initial_archives":128,"fixture_revision":2,
                    "storage_configuration":storage_configuration,
                    "edit_preparation":"recorded_single_character_insert"}),
                || {
                    let mut outcome = StepOutcome::items(1);
                    outcome.phase_elapsed_ns = if id == "cold_reads_during_save" {
                        fixture.cold_reads_during_saves().unwrap()
                    } else {
                        fixture.aged_operation(id).unwrap();
                        serde_json::json!({})
                    };
                    outcome
                },
            );
        }
    }
    fixture.verify().unwrap();
}
