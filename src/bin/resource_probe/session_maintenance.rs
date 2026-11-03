//! Emits raw production-path samples; the Performance Lens owns analysis/validation.
use super::events::{StepDescriptor, StepOutcome, emit_prepared_operation};
use scratchpad::app::services::session_store::SessionMaintenanceFixture;

pub(super) fn run() {
    let root = super::unique_probe_root("session-maintenance");
    let mut fixture = SessionMaintenanceFixture::new(
        root.clone(),
        super::session_save::fixture(50 * super::MB),
        128,
    )
    .expect("seed maintenance fixture");
    for scenario in [
        "stored_usage",
        "closed_token_lookup",
        "closed_archive_append",
        "save_after_branch",
        "save_after_navigation",
        "incremental_save",
    ] {
        for step in 0..9 {
            fixture
                .prepare(scenario)
                .expect("prepare maintenance sample");
            emit_prepared_operation(
                StepDescriptor {
                    scenario,
                    scenario_label: "Session maintenance with 50 MiB live text and 128+ archives",
                    workload_family: "session-persistence",
                    focus: "maintenance",
                    step_index: step,
                    workload_value: 50 * super::MB,
                    workload_unit: "bytes",
                    workload_label: format!("{scenario}; warm database; FULL synchronous"),
                },
                serde_json::json!({"fixture_revision": 1, "buffer_bytes": 50 * super::MB, "initial_archives": 128, "synchronous": "FULL", "setup_excluded": true}),
                || {
                    let (action, save, hashed) =
                        fixture.sample(scenario).expect("maintenance operation");
                    let mut outcome = StepOutcome::items(hashed as usize);
                    outcome.result_unit = "snapshot-bytes-hashed";
                    outcome.result_label = format!("{hashed} incremental snapshot bytes hashed");
                    outcome.phase_elapsed_ns =
                        serde_json::json!({"action_ns": action, "save_ns": save});
                    outcome
                },
            );
        }
    }
    fixture.verify().expect("verify maintenance session");
    drop(fixture);
    std::fs::remove_dir_all(root).expect("remove maintenance fixture");
}
