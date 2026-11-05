//! Prepared production-path baselines missing from the initial stage 0 run.
use super::events::{StepDescriptor, emit_prepared_operation};
use super::*;
use scratchpad::app::services::session_store::SessionMaintenanceFixture;
use std::time::Instant;

pub(super) fn run() {
    let directory = tempfile::tempdir().expect("isolated live-text baselines");
    let path = directory.path().join("large.txt");
    write_utf8_text_file(&path, 64 * MB).expect("large Unicode file");
    let document = FileService::read_file(&path)
        .expect("file-backed document")
        .document;
    assert!(document.piece_tree().is_file_backed());
    let snapshot = document.snapshot();
    let program = SearchProgram::compile(
        "café",
        SearchOptions {
            mode: SearchMode::PlainText,
            match_case: true,
            whole_word: false,
        },
    )
    .expect("query");
    for index in 0..3 {
        emit_prepared_operation(
            descriptor("file_backed_search", index, 64 * MB),
            serde_json::json!({"scope":"production snapshot chunk windows and search matcher; single-thread scan", "query":"café"}),
            || {
                let mut matches = 0;
                for chunk in snapshot.chunks_for_range(0..snapshot.len_chars(), 256 * KB, 5, 5) {
                    let (text, offset) = snapshot.search_text_cow(Some(chunk.window_range));
                    matches += search_program(&text, &program)
                        .matches
                        .into_iter()
                        .filter(|range| chunk.core_range.contains(&(offset + range.start)))
                        .count();
                }
                let remainder = 64 * MB % UTF8_SAMPLE_LINE.len();
                let expected = 64 * MB / UTF8_SAMPLE_LINE.len()
                    + UTF8_SAMPLE_LINE[..utf8_prefix_len(UTF8_SAMPLE_LINE, remainder)]
                        .matches("café")
                        .count();
                assert_eq!(matches, expected);
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
        workload_family: "live-text-stage-zero",
        focus: "concurrency",
        step_index: index,
        workload_value: bytes,
        workload_unit: "bytes",
        workload_label: format!("{} MiB", bytes / MB),
    }
}
