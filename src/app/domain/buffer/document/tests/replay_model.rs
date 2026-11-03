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
