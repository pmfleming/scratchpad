//! Search-only residency diagnostics. Missing operations/rungs cannot pass promises.
use crate::app::domain::{BufferState, WorkspaceTab};
use crate::app::services::search::{SearchOptions, SearchProgram, search_program_interruptible};
use crate::app::services::{file_service::FileService, session_store::SessionStore};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{ops::Range, sync::atomic::AtomicU64, time::Instant};

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn result_digest(ranges: &[Range<usize>]) -> String {
    let mut hash = Sha256::new();
    for range in ranges {
        hash.update((range.start as u64).to_le_bytes());
        hash.update((range.end as u64).to_le_bytes());
    }
    format!("{:x}", hash.finalize())
}
fn fixture_text(bytes: usize, fixture: &str) -> String {
    assert!(bytes >= 64);
    let source = super::fixtures::text(bytes, 0x7188_3312);
    let mut text = match fixture {
        "ascii" => String::from_utf8(
            source
                .bytes()
                .map(|b| if b.is_ascii() { b } else { b'x' })
                .collect(),
        )
        .unwrap(),
        "unicode_crlf" => source.replace('\n', "\r\n"),
        _ => panic!("unsupported search fixture"),
    };
    let mut end = bytes - 9;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text.truncate(end);
    while text.len() < bytes - 9 {
        text.push(' ');
    }
    text.push_str("\nneedle\r\n");
    assert_eq!(text.len(), bytes);
    text
}

pub fn search_trial(bytes: usize, fixture: &str, rung: &str, launch: u64) -> Value {
    let revision = std::env::var("SCRATCHPAD_DIAGNOSTIC_REVISION").unwrap_or_else(|_| {
        String::from_utf8(
            std::process::Command::new("git")
                .args(["rev-parse", "HEAD"])
                .output()
                .unwrap()
                .stdout,
        )
        .unwrap()
        .trim()
        .to_owned()
    });
    search_trial_with_revision(bytes, fixture, rung, launch, &revision)
}

fn search_trial_with_revision(
    bytes: usize,
    fixture: &str,
    rung: &str,
    launch: u64,
    revision: &str,
) -> Value {
    assert!(
        [
            "owned_contiguous",
            "owned_piece_tree",
            "store_resident",
            "store_warm",
            "store_cold"
        ]
        .contains(&rung)
    );
    let text = fixture_text(bytes, fixture);
    let expected_start = text.chars().count() - 8;
    let expected: Vec<_> = std::iter::once(expected_start..expected_start + 6).collect();
    let mut trial = json!({"scenario":"search_file_size_ceiling", "workload_value":bytes,"workload_unit":"bytes", "fixture":fixture,
        "fixture_digest":digest(text.as_bytes()),"result_digest":"","result_count":0,"expected_result_count":1,
        "code_path":format!("search/{rung}"),"completion_endpoint":"all_ranges_returned","revision":revision,"rung":rung,
        "memory_profile_bytes":std::env::var("SCRATCHPAD_MEMORY_PROFILE").ok().and_then(|s|s.parse::<u64>().ok()),
        "host_memory_bytes":crate::app::services::chunk_store::total_memory(),"launch_index":launch,
        "metric_ns":null,"metric":"/elapsed_ns","timing_source":"native_wall","status":"unavailable",
        "unavailable_reason":null,"cancellation_checked":false,"observations":{},"memory":{},"measurement_quality":{},"instructions":{}});
    if rung == "store_cold" {
        trial["unavailable_reason"] = json!(
            "Physical page-cache eviction/read proof is not implemented for this search diagnostic; no cold timing inferred"
        );
        return trial;
    }
    let program = SearchProgram::compile(
        "needle",
        SearchOptions {
            match_case: true,
            ..Default::default()
        },
    )
    .unwrap();
    let cancelled = AtomicU64::new(2);
    let (ranges, elapsed, observations, memory) = if rung == "owned_contiguous" {
        assert!(search_program_interruptible(&text, &program, || false).is_none());
        let timer = Instant::now();
        let result = search_program_interruptible(&text, &program, || true)
            .unwrap()
            .matches;
        (
            result,
            timer.elapsed().as_nanos(),
            json!({"store_read_count":0}),
            json!({}),
        )
    } else if rung == "owned_piece_tree" {
        let buffer = BufferState::new("owned diagnostic".into(), text, None);
        let snapshot = buffer.document().snapshot();
        assert!(
            crate::app::app_state::profile_snapshot_search_controlled(
                &snapshot, &program, &cancelled
            )
            .is_none()
        );
        let timer = Instant::now();
        let result = super::search_snapshot_ranges(&snapshot, &program).unwrap();
        (
            result,
            timer.elapsed().as_nanos(),
            json!({"store_read_count":0}),
            json!({}),
        )
    } else {
        let root = tempfile::tempdir().unwrap();
        let session = SessionStore::new(root.path().join("session"));
        let store = session.chunk_store_handle().unwrap();
        store.set_limits(0, 0);
        if rung == "store_resident" {
            store.enable_scan_cache();
        }
        let (_, aggregate, owner, _) = store.scan_observation();
        if rung == "store_resident" && bytes > owner.min(aggregate) {
            trial["unavailable_reason"] = json!(
                "Full fixture does not fit the memory-derived scan budget; no resident timing inferred"
            );
            trial["observations"] =
                json!({"scan_aggregate_limit_bytes":aggregate,"scan_per_owner_limit_bytes":owner});
            return trial;
        }
        let path = root.path().join("fixture.txt");
        std::fs::write(&path, &text).unwrap();
        drop(text);
        let file = FileService::read_file_in_store(&path, store.clone()).unwrap();
        let tabs = vec![WorkspaceTab::new(
            FileService::build_buffer_from_file_content(&path, file, None),
        )];
        session.persist(&tabs, 0, 16.0, false).unwrap();
        let snapshot = tabs[0].active_buffer().document().snapshot();
        assert_eq!(snapshot.len_bytes(), bytes);
        assert!(
            crate::app::app_state::profile_snapshot_search_controlled(
                &snapshot, &program, &cancelled
            )
            .is_none()
        );
        // Explicit warm-up, excluded from timing. Warm means primed OS pages,
        // resident additionally requires all timed reads to avoid the store.
        let warmup_start = store.epoch().read_count();
        assert_eq!(
            super::search_snapshot_ranges(&snapshot, &program).unwrap(),
            expected
        );
        let reads = store.epoch().read_count();
        let warmup_reads = reads - warmup_start;
        let before = store.scan_observation();
        let viewport = store.cached_bytes();
        let timer = Instant::now();
        let result = super::search_snapshot_ranges(&snapshot, &program).unwrap();
        let elapsed = timer.elapsed().as_nanos();
        let after = store.scan_observation();
        let reads = store.epoch().read_count() - reads;
        if rung == "store_resident" && (reads != 0 || after.3 == before.3) {
            trial["unavailable_reason"] = json!(
                "Timed operation did not retain all reads (budget/pressure changed); timing excluded"
            );
            trial["observations"] =
                json!({"store_read_count":reads,"retained_read_count":after.3-before.3});
            return trial;
        }
        if rung == "store_warm" {
            assert!(before.0 == 0 && viewport == 0 && reads > 0);
        }
        let policy = serde_json::to_value(store.admission_observation()).unwrap();
        let mut memory = super::outcomes::memory(&policy);
        memory["scan_cache_bytes"] = json!(after.0);
        memory["scan_aggregate_limit_bytes"] = json!(after.1);
        memory["scan_per_owner_limit_bytes"] = json!(after.2);
        (
            result,
            elapsed,
            json!({"store_read_count":reads,"retained_read_count":after.3-before.3,
            "app_cache_empty_at_start":before.0==0 && viewport==0,"os_cache_primed":true,
            "scan_bytes_at_start":before.0,"scan_bytes_at_end":after.0,"warmup_store_read_count":warmup_reads}),
            memory,
        )
    };
    assert_eq!(
        ranges, expected,
        "exact character ranges across Unicode/CRLF and chunk boundaries"
    );
    trial["result_digest"] = json!(result_digest(&ranges));
    trial["result_count"] = json!(ranges.len());
    trial["metric_ns"] = json!(elapsed);
    trial["status"] = json!("ok");
    trial["cancellation_checked"] = json!(true);
    trial["observations"] = observations;
    trial["observations"]["cancellation_scope"] = json!("before_start_generation_mismatch");
    trial["memory"] = memory;
    trial
}

#[cfg(test)]
mod tests {
    #[test]
    fn search_rungs_preserve_exact_unicode_crlf_results_and_cancellation() {
        for fixture in ["ascii", "unicode_crlf"] {
            let mut expected = None;
            for rung in [
                "owned_contiguous",
                "owned_piece_tree",
                "store_resident",
                "store_warm",
            ] {
                // Unit tests run from source archives without Git or a checkout.
                let result = super::search_trial_with_revision(
                    16 * 1024 * 1024,
                    fixture,
                    rung,
                    0,
                    "test-revision",
                );
                assert_eq!(result["revision"], "test-revision");
                assert_eq!(result["status"], "ok", "{result}");
                let identity = (
                    result["fixture_digest"].clone(),
                    result["result_digest"].clone(),
                );
                if let Some(previous) = &expected {
                    assert_eq!(&identity, previous);
                }
                expected = Some(identity);
            }
        }
    }
}
