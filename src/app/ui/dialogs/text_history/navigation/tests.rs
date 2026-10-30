use super::super::{snapshots, states::States};
use crate::app::{
    app_state::{ScratchpadApp, test_support::test_app},
    domain::{BufferState, CursorRange, WorkspaceTab},
    fonts::{EditorFontPreset, EditorFontSelection, apply_editor_fonts},
};
use eframe::{App, egui};
use egui_kittest::{Harness, kittest::Queryable};

fn app_with_history(root: &std::path::Path) -> ScratchpadApp {
    let path = root.join("note.txt");
    std::fs::write(&path, "old").unwrap();
    let mut buffer = BufferState::new("note.txt".into(), "old".into(), Some(path));
    for (end, text) in [(3, "middle"), (6, "current")] {
        buffer
            .replace_char_ranges_with_undo(
                &[(0..end, text.into())],
                CursorRange::default(),
                CursorRange::default(),
            )
            .unwrap();
    }
    assert_eq!(buffer.document().history_entries().len(), 2);
    let mut app = test_app(root);
    app.tab_manager.set_tabs(vec![WorkspaceTab::new(buffer)], 0);
    app
}
fn text(app: &ScratchpadApp) -> String {
    app.tab_manager.tabs.as_slice()[0].active_buffer().text()
}
fn jump(ctx: &egui::Context, app: &mut ScratchpadApp, id: u64, k: usize) -> bool {
    let entries = snapshots::state_entries(ctx, app);
    let states = States::new(&entries, id);
    snapshots::go_to_state(ctx, app, id, states.after(k))
}
fn harness(app: ScratchpadApp) -> Harness<'static, ScratchpadApp> {
    Harness::builder()
        .with_size(egui::vec2(1200.0, 850.0))
        .with_context_setup(|ctx| {
            apply_editor_fonts(
                ctx,
                &EditorFontSelection::scratchpad(EditorFontPreset::Standard),
                4.0,
            )
            .unwrap();
        })
        .build_ui_state(
            |ui, app| App::ui(app, ui, &mut eframe::Frame::_new_kittest()),
            app,
        )
}

#[test]
fn state_jumps_are_after_edits_noop_at_current_and_never_write_disk() {
    let root = tempfile::tempdir().unwrap();
    let mut app = app_with_history(root.path());
    let ctx = egui::Context::default();
    let id = app.tab_manager.tabs.as_slice()[0].active_buffer().id;
    for (k, expected) in [
        (1, "middle"),
        (1, "middle"),
        (0, "old"),
        (2, "current"),
        (0, "old"),
        (1, "middle"),
    ] {
        assert!(jump(&ctx, &mut app, id, k));
        assert_eq!(text(&app), expected);
        assert_eq!(
            app.tab_manager.tabs.as_slice()[0]
                .active_buffer()
                .document()
                .history_entries()
                .len(),
            2
        );
        assert_eq!(
            std::fs::read_to_string(root.path().join("note.txt")).unwrap(),
            "old"
        );
    }
    assert!(!snapshots::go_to_state(&ctx, &mut app, id, Some(u64::MAX)));
    assert_eq!(text(&app), "middle");
}

#[test]
fn other_file_jumps_can_leave_the_active_file_untouched() {
    let root = tempfile::tempdir().unwrap();
    let mut app = app_with_history(root.path());
    let first = app.tab_manager.tabs.as_slice()[0].clone();
    let id = first.active_buffer().id;
    let other = WorkspaceTab::new(BufferState::new("other".into(), "untouched".into(), None));
    app.tab_manager.set_tabs(vec![first, other], 1);
    app.state.dialogs.text_history.open();
    let mut h = harness(app);
    super::super::write_follow_focus(&h.ctx, false);
    h.run_steps(3);
    h.get_by_label("All files").click();
    h.run_steps(3);
    let oldest: Vec<_> = h
        .get_all_by_role_and_label(egui::Role::Button, "State: Oldest retained state")
        .collect();
    assert_eq!(oldest.len(), 2);
    oldest[0].click();
    h.run_steps(3);
    assert_eq!(text(h.state()), "old");
    assert_eq!(h.state().tab_manager.active_tab_index, 1);
    assert_eq!(
        h.state()
            .tab_manager
            .active_tab()
            .unwrap()
            .active_buffer()
            .text(),
        "untouched"
    );
    h.get_by_label("Choose file").click();
    h.run_steps(3);
    h.get_by_label("note.txt  ·  2").click();
    h.run_steps(3);
    assert_eq!(
        h.get_all_by_role_and_label(egui::Role::Button, "State: Oldest retained state")
            .count(),
        1
    );
    assert_eq!(
        h.state().tab_manager.tabs.as_slice()[0].active_buffer().id,
        id
    );
}

#[test]
fn rail_clicks_and_focused_keys_follow_screen_direction_without_stealing_editor_keys() {
    let root = tempfile::tempdir().unwrap();
    let mut app = app_with_history(root.path());
    app.state.dialogs.text_history.open();
    let summary = app.tab_manager.tabs.as_slice()[0]
        .active_buffer()
        .document()
        .history_entries()[0]
        .summary
        .clone();
    let mut h = harness(app);
    h.run_steps(3);
    // Opening the nonmodal History panel leaves arrow keys with the editor.
    h.key_press(egui::Key::ArrowDown);
    h.run_steps(2);
    assert_eq!(text(h.state()), "current");
    h.get_all_by_role_and_label(egui::Role::Button, &format!("State: {summary}"))
        .last()
        .unwrap()
        .click();
    h.run_steps(3);
    assert_eq!(text(h.state()), "middle");
    assert!(
        h.ctx
            .memory(|m| m.has_focus(super::super::list::focus_id())),
        "focus: {:?}",
        h.ctx.memory(|m| m.focused())
    );
    h.key_press(egui::Key::ArrowDown);
    h.run_steps(3);
    assert_eq!(text(h.state()), "old");
    h.key_press(egui::Key::ArrowUp);
    h.run_steps(3);
    assert_eq!(text(h.state()), "middle");
    h.key_press(egui::Key::Home);
    h.run_steps(3);
    assert_eq!(text(h.state()), "current");
    h.key_press(egui::Key::End);
    h.run_steps(3);
    assert_eq!(text(h.state()), "old");
    h.key_press(egui::Key::Escape);
    h.run_steps(2);
    assert!(!h.state().state.dialogs.text_history.is_open());
}

#[test]
fn scrubber_steps_only_active_file_and_closes_independently() {
    let root = tempfile::tempdir().unwrap();
    let mut app = app_with_history(root.path());
    app.state.dialogs.text_history.open();
    app.state.dialogs.history_scrubber.open();
    let mut h = harness(app);
    h.run_steps(3);
    h.get_by_label("Undo one step").click();
    h.run_steps(3);
    assert_eq!(text(h.state()), "middle");
    h.key_press(egui::Key::ArrowLeft);
    h.run_steps(3);
    assert_eq!(text(h.state()), "old");
    h.key_press(egui::Key::End);
    h.run_steps(3);
    assert_eq!(text(h.state()), "current");
    h.get_by_label("Close history").click();
    h.run_steps(3);
    assert!(!h.state().state.dialogs.text_history.is_open());
    assert!(h.state().state.dialogs.history_scrubber.is_open());
    h.get_by_label("Undo one step").click();
    h.run_steps(3);
    assert_eq!(text(h.state()), "middle");
    h.key_press(egui::Key::Escape);
    h.run_steps(3);
    assert!(!h.state().state.dialogs.history_scrubber.is_open());
}

#[test]
fn scrubber_drag_previews_then_commits_and_cancels_when_the_document_changes() {
    let root = tempfile::tempdir().unwrap();
    let mut app = app_with_history(root.path());
    app.state.dialogs.history_scrubber.open();
    let mut h = harness(app);
    h.run_steps(3);
    let rect = h.get_by_label("History position").rect();
    let start = egui::pos2(rect.right() - 9.0, rect.center().y);
    let end = egui::pos2(rect.left() + 9.0, rect.center().y);
    let press = |pos, pressed| egui::Event::PointerButton {
        pos,
        pressed,
        button: egui::PointerButton::Primary,
        modifiers: egui::Modifiers::NONE,
    };
    h.event(egui::Event::PointerMoved(start));
    h.event(press(start, true));
    h.run_steps(2);
    h.event(egui::Event::PointerMoved(end));
    h.run_steps(2);
    assert_eq!(text(h.state()), "current", "dragging must not mutate text");
    h.event(press(end, false));
    h.run_steps(3);
    assert_eq!(text(h.state()), "old");
    h.event(egui::Event::PointerMoved(end));
    h.event(press(end, true));
    h.run_steps(2);
    h.event(egui::Event::PointerMoved(start));
    h.run_steps(2);
    assert_eq!(text(h.state()), "old");
    h.state_mut().tab_manager.tabs.as_mut_slice()[0]
        .active_buffer_mut()
        .replace_text("new edit".into());
    h.run_steps(2);
    h.event(press(start, false));
    h.run_steps(3);
    assert_eq!(
        text(h.state()),
        "new edit",
        "stale drag must not replay over edits"
    );
}

#[test]
fn scrubber_follows_tabs_and_a_tab_switch_cancels_a_pending_drag() {
    let root = tempfile::tempdir().unwrap();
    let mut app = app_with_history(root.path());
    let first = app.tab_manager.tabs.as_slice()[0].clone();
    let mut other = BufferState::new("other".into(), "before".into(), None);
    other
        .replace_char_ranges_with_undo(
            &[(0..6, "after".into())],
            CursorRange::default(),
            CursorRange::default(),
        )
        .unwrap();
    app.tab_manager
        .set_tabs(vec![first, WorkspaceTab::new(other)], 0);
    app.state.dialogs.history_scrubber.open();
    let mut h = harness(app);
    h.run_steps(3);
    let rect = h.get_by_label("History position").rect();
    let start = egui::pos2(rect.right() - 9.0, rect.center().y);
    let end = egui::pos2(rect.left() + 9.0, rect.center().y);
    h.event(egui::Event::PointerMoved(start));
    h.event(egui::Event::PointerButton {
        pos: start,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::NONE,
    });
    h.run_steps(2);
    h.event(egui::Event::PointerMoved(end));
    h.run_steps(2);
    h.state_mut().tab_manager.set_active_tab_index_clamped(1);
    h.run_steps(2);
    h.event(egui::Event::PointerButton {
        pos: end,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::NONE,
    });
    h.run_steps(3);
    assert_eq!(text(h.state()), "current");
    assert_eq!(
        h.state()
            .tab_manager
            .active_tab()
            .unwrap()
            .active_buffer()
            .text(),
        "after"
    );
    h.get_by_label("Undo one step").click();
    h.run_steps(3);
    assert_eq!(
        h.state()
            .tab_manager
            .active_tab()
            .unwrap()
            .active_buffer()
            .text(),
        "before"
    );
    assert_eq!(text(h.state()), "current");
}

#[test]
fn clear_requires_confirmation_and_cancel_preserves_history() {
    let root = tempfile::tempdir().unwrap();
    let mut app = app_with_history(root.path());
    app.state.dialogs.text_history.open();
    let mut h = harness(app);
    h.run_steps(3);
    h.get_by_label("History options").click();
    h.run_steps(2);
    h.get_by_label("Clear history…").click();
    h.run_steps(2);
    assert_eq!(
        h.state().tab_manager.tabs.as_slice()[0]
            .active_buffer()
            .document()
            .history_entries()
            .len(),
        2
    );
    h.get_by_label("Cancel").click();
    h.run_steps(2);
    h.get_by_label("History options").click();
    h.run_steps(2);
    h.get_by_label("Clear history…").click();
    h.run_steps(2);
    h.get_by_role_and_label(egui::Role::Button, "Clear history")
        .click();
    h.run_steps(3);
    assert!(
        h.state().tab_manager.tabs.as_slice()[0]
            .active_buffer()
            .document()
            .history_entries()
            .is_empty()
    );
    assert_eq!(text(h.state()), "current");
}
