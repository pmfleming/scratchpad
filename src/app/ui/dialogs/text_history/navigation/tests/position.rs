use super::{app_with_history, harness};
use crate::app::{app_state::test_support::test_app, services::session_manager};
use eframe::egui;
use egui_kittest::kittest::{NodeT, Queryable};

#[test]
fn clicking_history_positions_preserves_both_sides_and_the_redo_tail_after_restart() {
    let root = tempfile::tempdir().unwrap();
    let mut app = app_with_history(root.path());
    let buffer = app
        .tab_manager
        .active_tab_mut()
        .unwrap()
        .active_buffer_mut();
    let ids: Vec<_> = buffer
        .document()
        .history_entries()
        .iter()
        .map(|e| e.id)
        .collect();
    buffer.undo_last_text_operation().unwrap();
    app.state.dialogs.text_history.open();
    let mut h = harness(app);
    h.run_steps(3);
    for position in [0, 2, 1, 0, 2, 1] {
        let mut rows: Vec<_> = h
            .get_all_by_role(egui::Role::Button)
            .filter(|node| {
                node.accesskit_node()
                    .label()
                    .is_some_and(|label| label.starts_with("State: "))
            })
            .collect();
        rows.sort_by(|a, b| b.rect().top().total_cmp(&a.rect().top()));
        assert_eq!(rows.len(), 3, "all original states must remain visible");
        rows[position].click();
        drop(rows);
        h.run_steps(3);
        let buffer = h.state().tab_manager.active_tab().unwrap().active_buffer();
        assert_eq!(buffer.text(), ["old", "middle", "current"][position]);
        let entries = buffer.document().history_entries();
        assert_eq!(entries.iter().map(|e| e.id).collect::<Vec<_>>(), ids);
        for (index, entry) in entries.iter().enumerate() {
            assert_eq!(entry.is_undone(), index >= position);
        }
    }
    session_manager::persist_session_now(h.state_mut()).unwrap();
    drop(h);
    let mut restored = test_app(root.path());
    let buffer = restored
        .tab_manager
        .active_tab_mut()
        .unwrap()
        .active_buffer_mut();
    assert_eq!(buffer.text(), "middle");
    assert_eq!(
        buffer
            .document()
            .history_entries()
            .iter()
            .map(|e| e.id)
            .collect::<Vec<_>>(),
        ids
    );
    buffer.redo_last_text_operation().unwrap();
    assert_eq!(buffer.text(), "current");
    buffer.undo_last_text_operation().unwrap();
    assert_eq!(buffer.text(), "middle");
    assert_eq!(
        std::fs::read_to_string(root.path().join("note.txt")).unwrap(),
        "old"
    );
}
