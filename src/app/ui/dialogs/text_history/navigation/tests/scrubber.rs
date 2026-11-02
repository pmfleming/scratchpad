use super::{app_with_history, harness, text};
use crate::app::{
    app_state::{ScratchpadApp, workspace::accessors},
    platform::PlatformProfile,
};
use eframe::egui;
use egui_kittest::{Harness, kittest::Queryable};

const CTRL: egui::Modifiers = egui::Modifiers {
    ctrl: true,
    command: true,
    ..egui::Modifiers::NONE
};

fn press(h: &mut Harness<'_, ScratchpadApp>, key: egui::Key) {
    h.key_press_modifiers(CTRL, key);
    h.run_steps(3);
}

#[test]
fn undo_redo_opens_scrubber_keeps_editor_focus_and_tracks_the_timeline() {
    for profile in [PlatformProfile::Windows, PlatformProfile::Hyprland] {
        let root = tempfile::tempdir().unwrap();
        let mut app = app_with_history(root.path());
        app.state.app_settings.platform.profile = profile;
        app.state.app_settings.ui.status_bar_visible = true;
        // An old saved override must no longer provide a dedicated entry point.
        app.state
            .app_settings
            .shortcuts
            .bindings
            .insert("open_history_scrubber".into(), "ctrl+alt+z".into());
        accessors::request_focus_for_active_view(&mut app);
        let mut h = harness(app);
        h.run_steps(3);
        assert!(h.query_by_label("Scrubber").is_none());
        assert!(h.query_by_label("History position").is_none());
        let editor_focus = h.ctx.memory(|m| m.focused());
        assert!(editor_focus.is_some());
        h.key_press_modifiers(CTRL | egui::Modifiers::ALT, egui::Key::Z);
        h.run_steps(3);
        assert_eq!(text(h.state()), "current");
        assert!(h.query_by_label("History position").is_none());
        for (key, expected, caption) in [
            (egui::Key::Z, "middle", "1 / 2"),
            (egui::Key::Z, "old", "0 / 2"),
            (egui::Key::Y, "middle", "1 / 2"),
            (egui::Key::Y, "current", "2 / 2"),
        ] {
            press(&mut h, key);
            assert_eq!(text(h.state()), expected);
            h.get_by_label(caption);
            h.get_by_label("History position");
            assert_eq!(h.ctx.memory(|m| m.focused()), editor_focus);
        }
        h.key_press(egui::Key::ArrowRight);
        h.run_steps(20);
        assert!(h.state().state.dialogs.history_scrubber.is_open());
        h.key_press(egui::Key::Escape);
        h.run_steps(3);
        assert!(h.query_by_label("History position").is_none());
        h.event(egui::Event::Text("!".into()));
        h.run_steps(3);
        assert!(text(h.state()).contains('!'));
        assert!(!h.state().state.dialogs.history_scrubber.is_open());
    }
}

#[test]
fn pointer_scrubbing_can_alternate_with_undo_redo_and_typing() {
    let root = tempfile::tempdir().unwrap();
    let mut app = app_with_history(root.path());
    accessors::request_focus_for_active_view(&mut app);
    let mut h = harness(app);
    h.run_steps(3);
    press(&mut h, egui::Key::Z);
    h.get_by_label("Redo one step").click();
    h.run_steps(3);
    assert_eq!(text(h.state()), "current");
    // Cancelling IME when leaving the editor must not steal scrubber focus back.
    h.event(egui::Event::Ime(egui::ImeEvent::Preedit {
        text: String::new(),
        active_range_chars: None,
    }));
    h.run_steps(3);
    assert!(
        h.ctx
            .memory(|m| m.has_focus(super::super::super::scrubber::focus_id()))
    );
    press(&mut h, egui::Key::Z);
    assert_eq!(text(h.state()), "middle");
    h.get_by_label("1 / 2");
    let rect = h.get_by_label("History position").rect();
    let start = rect.center();
    let end = egui::pos2(rect.left() + 9.0, rect.center().y);
    h.hover_at(start);
    h.drag_at(start);
    h.run_steps(2);
    h.hover_at(end);
    h.run_steps(3);
    assert_eq!(text(h.state()), "old");
    h.drop_at(end);
    h.run_steps(3);
    press(&mut h, egui::Key::Y);
    assert_eq!(text(h.state()), "middle");
    h.get_by_label("1 / 2");
    h.get_by_label("Redo one step").click();
    h.run_steps(3);
    assert_eq!(text(h.state()), "current");
    // Typing while the scrubber has focus returns to the editor without a lost keystroke.
    h.event(egui::Event::Text("!".into()));
    h.run_steps(3);
    assert!(text(h.state()).contains('!'));
    assert!(h.query_by_label("History position").is_none());
    press(&mut h, egui::Key::Z);
    assert_eq!(text(h.state()), "current");
    h.get_by_label("History position");
    h.get_by_label("Undo one step").click();
    h.run_steps(3);
    h.key_press(egui::Key::Escape);
    h.run_steps(3);
    assert!(!h.state().state.dialogs.history_scrubber.is_open());
    h.event(egui::Event::Paste("pasted".into()));
    h.run_steps(3);
    assert!(text(h.state()).contains("pasted"));
}

#[test]
fn undo_at_a_boundary_reopens_scrubber_and_edits_close_it() {
    let root = tempfile::tempdir().unwrap();
    let mut app = app_with_history(root.path());
    accessors::request_focus_for_active_view(&mut app);
    let mut h = harness(app);
    h.run_steps(3);
    press(&mut h, egui::Key::Z);
    press(&mut h, egui::Key::Z);
    h.key_press(egui::Key::Escape);
    h.run_steps(3);
    assert!(!h.state().state.dialogs.history_scrubber.is_open());
    press(&mut h, egui::Key::Z);
    assert_eq!(text(h.state()), "old");
    h.get_by_label("0 / 2");
    h.event(egui::Event::Paste("paste".into()));
    h.run_steps(3);
    assert!(!h.state().state.dialogs.history_scrubber.is_open());
    press(&mut h, egui::Key::Z);
    assert_eq!(text(h.state()), "old");
    assert!(h.state().state.dialogs.history_scrubber.is_open());
    // Menu-driven edits use the same lifecycle as native keyboard edits.
    assert!(
        crate::app::app_state::workspace::editing::insert_text_in_active_view(
            h.state_mut(),
            "insert"
        )
    );
    h.run_steps(3);
    assert!(!h.state().state.dialogs.history_scrubber.is_open());
    press(&mut h, egui::Key::Z);
    h.event(egui::Event::Ime(egui::ImeEvent::Preedit {
        text: "composing".into(),
        active_range_chars: None,
    }));
    h.run_steps(3);
    assert!(!h.state().state.dialogs.history_scrubber.is_open());
}

#[test]
fn the_last_editor_event_in_a_frame_controls_scrubber_visibility() {
    for edit_last in [true, false] {
        let root = tempfile::tempdir().unwrap();
        let mut app = app_with_history(root.path());
        accessors::request_focus_for_active_view(&mut app);
        let mut h = harness(app);
        h.run_steps(3);
        if !edit_last {
            h.event(egui::Event::Text("!".into()));
        }
        h.key_press_modifiers(CTRL, egui::Key::Z);
        if edit_last {
            h.event(egui::Event::Text("!".into()));
        }
        h.run_steps(3);
        assert_eq!(
            h.state().state.dialogs.history_scrubber.is_open(),
            !edit_last
        );
    }
}

#[test]
fn editing_closes_scrubber_even_when_aggregate_budget_evicts_history() {
    let root = tempfile::tempdir().unwrap();
    let mut app = app_with_history(root.path());
    accessors::request_focus_for_active_view(&mut app);
    let mut h = harness(app);
    h.run_steps(3);
    press(&mut h, egui::Key::Z);
    assert!(h.state().state.dialogs.history_scrubber.is_open());
    h.state_mut()
        .state
        .app_settings
        .history
        .budget
        .aggregate_byte_budget = 1;
    h.event(egui::Event::Text("!".into()));
    h.run_steps(3);
    assert!(text(h.state()).contains('!'));
    assert!(
        h.state()
            .tab_manager
            .active_tab()
            .unwrap()
            .active_buffer()
            .document()
            .history_entries()
            .is_empty()
    );
    assert!(!h.state().state.dialogs.history_scrubber.is_open());
}
