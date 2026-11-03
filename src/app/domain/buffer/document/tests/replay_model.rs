use super::{CursorRange, TextDocument, helpers::insert_isolated_edit};
use rand::{RngExt, SeedableRng, rngs::StdRng};

#[test]
fn seeded_history_replay_matches_independent_text_states_and_survives_restart() {
    for seed in 0..12 {
        let mut rng = StdRng::seed_from_u64(seed);
        let mut document = TextDocument::new("α\r\n尾".into());
        let mut states = std::collections::BTreeMap::from([
            (0, String::new()),
            (document.history_now(), document.extract_text()),
        ]);
        for step in 0..240 {
            if step < 40 || rng.random_range(0..4) == 0 {
                let mut model: Vec<char> = states[&document.history_now()].chars().collect();
                let start = rng.random_range(0..=model.len());
                let end = rng.random_range(start..=model.len());
                let inserted = format!("{step}λ\n");
                document.seal_history_group();
                document
                    .replace_char_ranges_with_undo(
                        &[(start..end, inserted.clone())],
                        CursorRange::default(),
                        CursorRange::default(),
                    )
                    .unwrap();
                model.splice(start..end, inserted.chars());
                states.insert(document.history_now(), model.into_iter().collect());
            } else {
                let target = *states
                    .keys()
                    .nth(rng.random_range(0..states.len()))
                    .unwrap();
                document.navigate_history(target).unwrap();
            }
            assert_eq!(
                document.extract_text(),
                states[&document.history_now()],
                "seed {seed}, step {step}"
            );
            assert_eq!(document.history_entries().len(), states.len() - 1);
            if step % 37 == 0 {
                let history = document.exported_history();
                let now = document.history_now();
                let tip = document.history_tip();
                document = TextDocument::new(states[&now].clone());
                document.restore_exported_history(history);
                assert_eq!((document.history_now(), document.history_tip()), (now, tip));
            }
        }
    }
}

#[test]
fn replay_across_a_barrier_is_atomic_in_both_directions() {
    let mut document = TextDocument::new(String::new());
    for i in 0..40 {
        insert_isolated_edit(&mut document, i, "λ");
    }
    let first = document.history_entries()[0].id;
    let last = document.history_entries()[39].id;
    document.history.entries[20].flags.replayable = false;
    let revision = document.revision_key();
    let history_revision = document.history_revision_counter();
    assert!(
        document
            .navigate_history(document.history.tree.parent(first).unwrap())
            .is_err()
    );
    assert_eq!(document.extract_text(), "λ".repeat(40));
    assert!(document.revision_key() == revision);
    assert_eq!(document.history_revision_counter(), history_revision);
    assert!(document.history_entries().iter().all(|e| !e.is_undone()));

    document.history.entries[20].flags.replayable = true;
    document
        .navigate_history(document.history.tree.parent(first).unwrap())
        .unwrap();
    document.history.entries[20].flags.replayable = false;
    assert!(document.navigate_history(last).is_err());
    assert_eq!(document.extract_text(), "");
    assert!(document.history_entries().iter().all(|e| e.is_undone()));
}

#[test]
fn large_branches_survive_cache_eviction_and_replay_does_not_append_payloads() {
    let mut document = TextDocument::new(String::new());
    let mut tips = Vec::new();
    for index in 0..20 {
        document.navigate_history(0).unwrap();
        let text = format!("branch-{index}: λ\r\n🦀").repeat(32_768);
        document.replace_exact_with_history(&text);
        tips.push((document.history_now(), text));
    }
    let allocated = document.piece_tree().add_storage_bytes();
    for _ in 0..2 {
        for (id, text) in &tips {
            document.navigate_history(*id).unwrap();
            assert_eq!(&document.extract_text(), text);
        }
    }
    assert_eq!(document.piece_tree().add_storage_bytes(), allocated);
    assert_eq!(document.history_entries().len(), tips.len());
}

#[test]
fn failed_history_import_preserves_original_text_and_all_original_spans() {
    let mut original = TextDocument::new("keep λ".into());
    super::helpers::insert_isolated_edit(&mut original, 6, "!");
    let before = original.exported_history();
    let now = original.history_now();
    let mut corrupt = before.clone();
    corrupt[0].fingerprint ^= 1;
    assert!(!original.restore_exported_history(corrupt));
    let mut ambiguous = before.clone();
    ambiguous[0].current = true;
    assert!(!original.restore_exported_history(ambiguous));
    let mut exhausted = before.clone();
    exhausted.last_mut().unwrap().id = u64::MAX;
    assert!(!original.restore_exported_history(exhausted));
    assert_eq!(original.exported_history(), before);
    assert_eq!(original.history_now(), now);
    original.undo_last_operation().unwrap();
    assert_eq!(original.extract_text(), "keep λ");
}

#[test]
#[ignore = "manual edit-tree latency profile"]
fn profile_edit_tree_navigation_and_capture() {
    use std::time::Instant;
    for count in [1_000, 10_000, 100_000] {
        let mut document = TextDocument::new(String::new());
        let start = Instant::now();
        for index in 0..count {
            document.replace_exact_with_history(if index % 2 == 0 { "α" } else { "β" });
        }
        let build = start.elapsed();
        let start = Instant::now();
        for _ in 0..1_000 {
            std::hint::black_box(document.history_snapshot());
        }
        let captures = start.elapsed();
        let start = Instant::now();
        for _ in 0..500 {
            document.undo_last_operation().unwrap();
            document.redo_last_operation().unwrap();
        }
        let nearby = start.elapsed();
        let start = Instant::now();
        for _ in 0..500 {
            let capture = document.history_snapshot();
            document.undo_last_operation().unwrap();
            document.redo_last_operation().unwrap();
            std::hint::black_box(capture);
        }
        let shared_nearby = start.elapsed();
        let start = Instant::now();
        document.navigate_history(0).unwrap();
        document.navigate_history(count).unwrap();
        let long = start.elapsed();
        eprintln!(
            "history nodes={count} build_ms={} capture_ns={} nearby_ns={} shared_nearby_ns={} full_roundtrip_ms={} payload_bytes={}",
            build.as_millis(),
            captures.as_nanos() / 1000,
            nearby.as_nanos() / 1000,
            shared_nearby.as_nanos() / 1000,
            long.as_millis(),
            document.piece_tree().add_storage_bytes()
        );
        assert_eq!(document.extract_text(), "β");
    }
}
