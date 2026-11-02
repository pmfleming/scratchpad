use super::{app_with_history, displayed_text, harness};
use crate::app::{
    app_state::ScratchpadApp,
    domain::{BufferState, CursorRange, WorkspaceTab},
    platform::PlatformProfile,
};
use eframe::egui;
use egui_kittest::kittest::Queryable;

fn active_buffer(app: &ScratchpadApp) -> &BufferState {
    app.tab_manager.active_tab().unwrap().active_buffer()
}

#[test]
fn first_open_click_uses_replaced_documents_history_even_when_counters_match() {
    let root = tempfile::tempdir().unwrap();
    let mut app = app_with_history(root.path());
    let restored = app.tab_manager.active_tab().unwrap().clone();
    let mut previous = BufferState::new("previous".into(), "old".into(), None);
    previous
        .replace_char_ranges_with_undo(
            &[(0..3, "previous edit".into())],
            CursorRange::default(),
            CursorRange::default(),
        )
        .unwrap();
    previous.undo_last_text_operation().unwrap();
    assert_eq!(
        previous.document().history_revision_counter(),
        restored
            .active_buffer()
            .document()
            .history_revision_counter()
    );
    app.tab_manager
        .active_tab_mut()
        .unwrap()
        .active_buffer_mut()
        .install_history_document(previous.document().clone());
    // The scrubber can populate the shared cache before a background restore
    // replaces a document in the same buffer, with the same revision counter.
    crate::app::app_state::workspace::mutation::cached_text_history_entries(&mut app);
    assert!(app.tab_manager.replace_restored_tab(0, restored));
    let mut h = harness(app);
    h.run_steps(3);
    h.key_press_modifiers(egui::Modifiers::CTRL, egui::Key::H);
    h.run_steps(3);
    h.get_by_label("State: Oldest retained state").click();
    h.run_steps(3);
    assert_eq!(active_buffer(h.state()).text(), "old");
}

#[test]
fn opening_history_focuses_navigation_before_any_pointer_interaction() {
    let root = tempfile::tempdir().unwrap();
    let mut app = app_with_history(root.path());
    app.state.app_settings.ui.status_bar_visible = true;
    let mut h = harness(app);
    h.run_steps(3);
    let id = active_buffer(h.state()).id;
    h.key_press_modifiers(egui::Modifiers::CTRL, egui::Key::H);
    h.run_steps(3);
    h.key_press(egui::Key::ArrowDown);
    h.run_steps(3);
    assert_eq!(displayed_text(&h, id), "middle");
    assert_eq!(active_buffer(h.state()).text(), "current");
    h.key_press(egui::Key::Escape);
    h.run_steps(3);
    assert_eq!(displayed_text(&h, id), "current");
    h.get_by_role_and_label(
        egui::Role::Label,
        egui_phosphor::regular::CLOCK_COUNTER_CLOCKWISE,
    )
    .click();
    h.run_steps(3);
    h.key_press(egui::Key::ArrowDown);
    h.run_steps(3);
    assert_eq!(displayed_text(&h, id), "middle");
    assert_eq!(active_buffer(h.state()).text(), "current");
}

#[test]
fn first_row_click_without_preview_focus_accepts_pointer_press_and_release() {
    let root = tempfile::tempdir().unwrap();
    let mut h = harness(app_with_history(root.path()));
    h.run_steps(3);
    h.key_press_modifiers(egui::Modifiers::CTRL, egui::Key::H);
    h.run_steps(3);
    let id = active_buffer(h.state()).id;
    let pos = h
        .get_by_label("State: Oldest retained state")
        .rect()
        .center();
    h.hover_at(pos);
    h.drag_at(pos);
    h.run_steps(2);
    h.drop_at(pos);
    h.run_steps(3);
    assert_eq!(active_buffer(h.state()).text(), "old");
    h.key_press(egui::Key::ArrowUp);
    h.run_steps(3);
    assert_eq!(displayed_text(&h, id), "middle");
}

#[test]
fn first_open_previews_and_confirms_before_and_after_switching_files() {
    for shortcut in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let mut app = app_with_history(root.path());
        app.state.app_settings.platform.profile = PlatformProfile::Hyprland;
        app.state.app_settings.ui.status_bar_visible = true;
        let first = app.tab_manager.active_tab().unwrap().clone();
        let first_id = first.active_buffer().id;
        let mut second = BufferState::new("other.txt".into(), "before".into(), None);
        second
            .replace_char_ranges_with_undo(
                &[(0..6, "after".into())],
                CursorRange::default(),
                CursorRange::default(),
            )
            .unwrap();
        let second_id = second.id;
        app.tab_manager
            .set_tabs(vec![first, WorkspaceTab::new(second)], 0);
        if shortcut {
            crate::app::services::session_manager::persist_session_now(&mut app).unwrap();
            drop(app);
            app = crate::app::app_state::test_support::test_app(root.path());
        }
        let first_picker = format!(
            "{}  ·  2",
            app.tab_manager.tabs.as_slice()[0].active_buffer().name
        );
        let mut h = harness(app);
        h.run_steps(3);
        if shortcut {
            h.key_press_modifiers(egui::Modifiers::CTRL, egui::Key::H);
        } else {
            h.get_by_role_and_label(
                egui::Role::Label,
                egui_phosphor::regular::CLOCK_COUNTER_CLOCKWISE,
            )
            .click();
        }
        h.run_steps(3);
        for (id, original, latest, picker) in [
            (first_id, "old", "current", None),
            (second_id, "before", "after", Some("other.txt  ·  1")),
            (first_id, "old", "current", Some(first_picker.as_str())),
        ] {
            if let Some(label) = picker {
                h.get_by_label("Choose file").click();
                h.run_steps(3);
                h.get_by_label(label).click();
                h.run_steps(3);
            }
            assert!(h.state().state.dialogs.text_history.is_open());
            assert_eq!(active_buffer(h.state()).id, id);
            h.hover_at(
                h.get_by_label("State: Oldest retained state")
                    .rect()
                    .center(),
            );
            h.run_steps(4);
            assert_eq!(
                displayed_text(&h, id),
                original,
                "shortcut={shortcut}, picker={picker:?}"
            );
            assert_eq!(active_buffer(h.state()).text(), latest);
            h.get_by_label("State: Oldest retained state").click();
            h.run_steps(3);
            assert_eq!(active_buffer(h.state()).text(), original);
            h.key_press(egui::Key::Home);
            h.run_steps(3);
            assert_eq!(displayed_text(&h, id), latest);
            let summary = active_buffer(h.state())
                .document()
                .history_entries()
                .last()
                .unwrap()
                .summary
                .clone();
            h.get_all_by_label(&format!("State: {summary}"))
                .next()
                .unwrap()
                .click();
            h.run_steps(3);
            assert_eq!(active_buffer(h.state()).text(), latest);
        }
        h.key_press(egui::Key::Escape);
        h.run_steps(3);
        h.key_press_modifiers(egui::Modifiers::CTRL, egui::Key::H);
        h.run_steps(3);
        h.hover_at(
            h.get_by_label("State: Oldest retained state")
                .rect()
                .center(),
        );
        h.run_steps(4);
        assert_eq!(displayed_text(&h, first_id), "old");
        assert_eq!(
            std::fs::read_to_string(root.path().join("note.txt")).unwrap(),
            "old"
        );
    }
}
