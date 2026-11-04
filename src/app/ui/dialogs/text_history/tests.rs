use super::*;
use crate::app::{
    app_state::test_support::test_app,
    domain::{BufferState, CursorRange, WorkspaceTab},
    fonts::{EditorFontPreset, EditorFontSelection, apply_editor_fonts},
};
use eframe::App;
use egui_kittest::{Harness, kittest::Queryable};

fn app(root: &std::path::Path) -> ScratchpadApp {
    let mut app = test_app(root);
    let mut buffer = BufferState::new("note.txt".into(), "base".into(), None);
    for text in ["one", "two"] {
        let len = buffer.document().piece_tree().len_chars();
        buffer
            .replace_char_ranges_with_undo(
                &[(0..len, text.into())],
                crate::app::domain::CursorRange::default(),
                crate::app::domain::CursorRange::default(),
            )
            .unwrap();
    }
    app.tab_manager.set_tabs(vec![WorkspaceTab::new(buffer)], 0);
    app.state.dialogs.text_history.open();
    app
}
fn harness(app: ScratchpadApp) -> Harness<'static, ScratchpadApp> {
    Harness::builder()
        .allow_missing_glyphs()
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
fn text(app: &ScratchpadApp) -> String {
    app.tab_manager.active_tab().unwrap().active_buffer().text()
}

#[test]
fn now_is_visible_hover_is_inert_and_click_selects_a_state() {
    let root = tempfile::tempdir().unwrap();
    let mut h = harness(app(root.path()));
    h.run_steps(3);
    assert!(h.query_by_label("Now · Replace match").is_some());
    let root_row = h.get_by_label("Empty document");
    h.hover_at(root_row.rect().center());
    h.run_steps(3);
    assert_eq!(text(h.state()), "two");
    h.get_by_label("Empty document").click();
    h.run_steps(3);
    assert_eq!(text(h.state()), "");
    assert!(h.query_by_label("Now · Empty document").is_some());
    h.key_press(egui::Key::ArrowRight);
    h.run_steps(3);
    assert_eq!(text(h.state()), "base");
    h.key_press(egui::Key::Escape);
    h.run_steps(3);
    assert!(!h.state().state.dialogs.text_history.is_open());
    assert_eq!(text(h.state()), "base");
}

#[test]
fn branches_remain_selectable_and_recent_order_ignores_navigation() {
    let root = tempfile::tempdir().unwrap();
    let mut app = app(root.path());
    let buffer = app
        .tab_manager
        .active_tab_mut()
        .unwrap()
        .active_buffer_mut();
    let id = buffer.id;
    let before = buffer.document().last_edit();
    buffer.navigate_history(1).unwrap();
    assert_eq!(before, buffer.document().last_edit());
    buffer
        .replace_char_ranges_with_undo(
            &[(0..4, "alternate".into())],
            crate::app::domain::CursorRange::default(),
            crate::app::domain::CursorRange::default(),
        )
        .unwrap();
    let mut h = harness(app);
    h.run_steps(3);
    assert_eq!(h.query_all_by_label("Collapse branch").count(), 2);
    let ctx = h.ctx.clone();
    assert!(navigate(&ctx, h.state_mut(), id, 3));
    h.run_steps(3);
    assert_eq!(text(h.state()), "two");
    assert!(navigate(&ctx, h.state_mut(), id, 4));
    h.run_steps(3);
    assert_eq!(text(h.state()), "alternate");
}

#[test]
fn history_typing_delivers_first_event_to_editor() {
    let root = tempfile::tempdir().unwrap();
    let mut h = harness(app(root.path()));
    h.run_steps(3);
    h.event(egui::Event::Text("X".into()));
    h.run_steps(3);
    assert!(text(h.state()).contains('X'));
}

#[test]
fn scrubber_shows_only_the_selected_route_and_opens_without_moving_now() {
    let root = tempfile::tempdir().unwrap();
    let mut app = app(root.path());
    app.state.dialogs.text_history.close();
    let buffer = app
        .tab_manager
        .active_tab_mut()
        .unwrap()
        .active_buffer_mut();
    buffer.navigate_history(1).unwrap();
    buffer
        .replace_char_ranges_with_undo(
            &[(0..4, "branch".into())],
            crate::app::domain::CursorRange::default(),
            crate::app::domain::CursorRange::default(),
        )
        .unwrap();
    buffer.undo_last_text_operation().unwrap();
    let mut h = harness(app);
    h.run_steps(3);
    assert_eq!(text(h.state()), "base");
    assert!(h.query_by_label("Now · 1 of 2 changes").is_some());
    assert!(h.state().state.dialogs.history_scrubber.is_open());
    let before = h
        .state()
        .tab_manager
        .active_tab()
        .unwrap()
        .active_buffer()
        .document()
        .history_now();
    h.run_steps(3);
    assert_eq!(
        before,
        h.state()
            .tab_manager
            .active_tab()
            .unwrap()
            .active_buffer()
            .document()
            .history_now()
    );
}

#[test]
fn forget_requires_confirmation_and_keeps_current_text_as_a_new_initial_edit() {
    let root = tempfile::tempdir().unwrap();
    let mut h = harness(app(root.path()));
    h.run_steps(3);
    h.get_by_label("Forget history…").click();
    h.run_steps(2);
    assert_eq!(text(h.state()), "two");
    assert_eq!(
        h.state()
            .tab_manager
            .active_tab()
            .unwrap()
            .active_buffer()
            .document()
            .history_entries()
            .len(),
        3
    );
    h.get_by_label("Forget this file's history").click();
    h.run_steps(2);
    let document = h
        .state()
        .tab_manager
        .active_tab()
        .unwrap()
        .active_buffer()
        .document();
    assert_eq!(document.history_entries().len(), 1);
    assert_eq!(document.history_now(), 1);
    assert_eq!(text(h.state()), "two");
}

#[test]
#[ignore = "manual virtualized History rendering profile"]
fn profile_edit_tree_history_rendering() {
    use std::time::Instant;
    for count in [1_000, 10_000] {
        let root = tempfile::tempdir().unwrap();
        let mut app = app(root.path());
        let document = app
            .tab_manager
            .active_tab_mut()
            .unwrap()
            .active_buffer_mut()
            .document_mut();
        for index in 0..count {
            document.replace_exact_with_history(if index % 2 == 0 { "α" } else { "β" });
        }
        let start = Instant::now();
        let mut h = harness(app);
        h.run_steps(3);
        let opening = start.elapsed();
        let start = Instant::now();
        h.run_steps(20);
        eprintln!(
            "history_ui nodes={count} opening_ms={} warm_frame_us={}",
            opening.as_millis(),
            start.elapsed().as_micros() / 20
        );
    }
}

#[test]
fn newest_rows_are_first_and_all_files_filters_by_history_not_now() {
    let root = tempfile::tempdir().unwrap();
    let mut app = app(root.path());
    let blank = BufferState::new("empty.txt".into(), String::new(), None);
    let mut undone = BufferState::new("undone.txt".into(), "retained".into(), None);
    undone.undo_last_text_operation().unwrap();
    let undone_id = undone.id;
    let original = app.tab_manager.active_tab().unwrap().clone();
    app.tab_manager.set_tabs(
        vec![
            original,
            WorkspaceTab::new(blank),
            WorkspaceTab::new(undone),
        ],
        0,
    );
    let mut h = harness(app);
    h.run_steps(3);
    assert!(
        h.get_by_label("Now · Replace match").rect().top()
            < h.get_by_label("Empty document").rect().top()
    );
    h.get_by_label("All files").click();
    h.run_steps(3);
    assert!(
        h.query_by_role_and_label(egui::accesskit::Role::Button, "empty.txt")
            .is_none()
    );
    assert!(
        h.query_by_role_and_label(egui::accesskit::Role::Button, "note.txt")
            .is_some()
    );
    h.get_by_role_and_label(egui::accesskit::Role::Button, "undone.txt")
        .click();
    h.run_steps(3);
    let state = h
        .ctx
        .data(|d| d.get_temp::<HistoryUi>(state_key()))
        .unwrap();
    assert!(state.all_files);
    assert!(state.expanded.contains(&undone_id));
    assert_eq!(state.selected, Some(undone_id));
    assert_eq!(active_buffer(h.state()), Some(undone_id));
    assert_eq!(text(h.state()), "");
    assert!(
        h.query_by_role_and_label(egui::accesskit::Role::Button, "undone.txt")
            .is_some()
    );
    assert!(h.query_by_label("Now · Empty document").is_some());
    h.state_mut().tab_manager.set_active_tab_index_clamped(0);
    h.run_steps(3);
    h.get_by_role_and_label(egui::accesskit::Role::Button, "undone.txt")
        .click();
    h.run_steps(3);
    assert_eq!(active_buffer(h.state()), Some(undone_id));
    assert_eq!(text(h.state()), "");
}

fn scrubber_app(root: &std::path::Path) -> ScratchpadApp {
    let mut app = app(root);
    app.state.dialogs.text_history.close();
    app.tab_manager
        .active_tab_mut()
        .unwrap()
        .active_buffer_mut()
        .undo_last_text_operation()
        .unwrap();
    app
}

#[test]
fn scrubber_transport_moves_real_now_and_open_history_keeps_it() {
    let root = tempfile::tempdir().unwrap();
    let mut h = harness(scrubber_app(root.path()));
    h.run_steps(3);
    assert_eq!(text(h.state()), "one");
    assert!(!h.ctx.memory(|m| m.has_focus(scrubber::focus_id())));
    for (button, expected) in [
        ("Go to start (Home)", ""),
        ("Next change (→)", "base"),
        ("Go to end (End)", "two"),
        ("Previous change (←)", "one"),
    ] {
        h.get_by_label(button).click();
        h.run_steps(3);
        assert_eq!(text(h.state()), expected);
        assert!(h.ctx.memory(|m| m.has_focus(scrubber::focus_id())));
    }
    assert!(h.query_by_label("Close scrubber").is_none());
    h.get_by_label("Open in History").click();
    h.run_steps(3);
    assert!(h.state().state.dialogs.text_history.is_open());
    assert!(!h.state().state.dialogs.history_scrubber.is_open());
    assert_eq!(text(h.state()), "one");
    h.key_press(egui::Key::Escape);
    h.run_steps(3);
    assert_eq!(text(h.state()), "one");
}

#[test]
fn scrubber_hover_is_inert_and_drag_updates_editor_before_release() {
    let root = tempfile::tempdir().unwrap();
    let mut h = harness(scrubber_app(root.path()));
    h.run_steps(3);
    let track = h.get_by_label("History position").rect();
    assert!(track.width() > 300.0);
    let start = egui::pos2(track.left() + 7.0, track.center().y);
    let end = egui::pos2(track.right() - 7.0, track.center().y);
    h.hover_at(start);
    h.run_steps(3);
    assert_eq!(text(h.state()), "one");
    h.drag_at(start);
    h.run_steps(3);
    assert_eq!(text(h.state()), "");
    assert!(history_scrubbing(&h.ctx, h.state()));
    h.hover_at(end);
    h.run_steps(3);
    assert_eq!(text(h.state()), "two");
    let tab = h.state().tab_manager.active_tab().unwrap();
    let buffer = tab.active_buffer();
    assert_eq!(buffer.document().history_now(), 3);
    assert_eq!(buffer.document().history_entries().len(), 3);
    assert_eq!(
        tab.layout
            .active_view()
            .unwrap()
            .latest_display_snapshot_revision,
        Some(buffer.document_revision())
    );
    assert!(h.ctx.input(|i| i.pointer.any_down()));
    h.drop_at(end);
    h.run_steps(3);
    assert!(!history_scrubbing(&h.ctx, h.state()));
    assert_eq!(text(h.state()), "two");
    h.key_press(egui::Key::Escape);
    h.run_steps(3);
    assert!(!h.state().state.dialogs.history_scrubber.is_open());
    assert_eq!(text(h.state()), "two");
}

#[test]
fn scrubber_drag_cancels_on_file_switch_even_without_an_initial_seek() {
    let root = tempfile::tempdir().unwrap();
    let mut app = scrubber_app(root.path());
    let original = app.tab_manager.active_tab().unwrap().clone();
    let other = WorkspaceTab::new(BufferState::new(
        "other.txt".into(),
        "untouched".into(),
        None,
    ));
    app.tab_manager.set_tabs(vec![original, other], 0);
    app.state.dialogs.history_scrubber.open();
    let mut h = harness(app);
    h.run_steps(3);
    let track = h.get_by_label("History position").rect();
    let thumb = egui::pos2(
        track.left() + 7.0 + (track.width() - 14.0) * 2.0 / 3.0,
        track.center().y,
    );
    h.hover_at(thumb);
    h.drag_at(thumb);
    h.run_steps(3);
    assert_eq!(text(h.state()), "one");
    h.state_mut().tab_manager.set_active_tab_index_clamped(1);
    h.hover_at(egui::pos2(track.left() + 7.0, track.center().y));
    h.run_steps(3);
    assert_eq!(text(h.state()), "untouched");
    assert!(!history_scrubbing(&h.ctx, h.state()));
    h.drop_at(thumb);
    h.run_steps(3);
    h.get_by_label("Go to start (Home)").click();
    h.run_steps(3);
    assert_eq!(text(h.state()), "");
}

#[test]
fn editing_from_scrubber_preserves_first_event_and_old_continuation() {
    let root = tempfile::tempdir().unwrap();
    let mut h = harness(scrubber_app(root.path()));
    h.run_steps(3);
    h.get_by_label("Go to start (Home)").click();
    h.run_steps(3);
    h.event(egui::Event::Text("X".into()));
    h.run_steps(3);
    assert_eq!(text(h.state()), "X");
    assert!(!h.state().state.dialogs.history_scrubber.is_open());
    let buffer = h
        .state_mut()
        .tab_manager
        .active_tab_mut()
        .unwrap()
        .active_buffer_mut();
    buffer.navigate_history(3).unwrap();
    assert_eq!(buffer.text(), "two");
}

#[test]
fn scroll_to_now_changes_only_the_viewport() {
    let root = tempfile::tempdir().unwrap();
    let mut app = app(root.path());
    for i in 0..80 {
        app.tab_manager
            .active_tab_mut()
            .unwrap()
            .active_buffer_mut()
            .document_mut()
            .replace_exact_with_history(&format!("change {i}"));
    }
    let mut h = harness(app);
    h.run_steps(3);
    let now = h.get_by_label("Now · Replace with \"change 79\"").rect();
    h.hover_at(now.center());
    h.event(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: egui::vec2(0.0, -2500.0),
        modifiers: egui::Modifiers::NONE,
        phase: egui::TouchPhase::Move,
        source: egui::MouseWheelSource::Wheel,
    });
    h.run_steps(10);
    let before = text(h.state());
    h.get_by_label("Scroll to Now").click();
    h.run_steps(10);
    assert_eq!(text(h.state()), before);
    assert!(h.query_by_label("Scroll to Now").is_none());
    assert!(
        h.query_by_label("Now · Replace with \"change 79\"")
            .is_some()
    );
}

#[test]
fn whitespace_labels_are_display_only() {
    let root = tempfile::tempdir().unwrap();
    let mut app = app(root.path());
    let buffer = app
        .tab_manager
        .active_tab_mut()
        .unwrap()
        .active_buffer_mut();
    buffer
        .replace_char_ranges_with_undo(
            &[(3..3, "\r\n".into())],
            crate::app::domain::CursorRange::default(),
            crate::app::domain::CursorRange::default(),
        )
        .unwrap();
    let summary = buffer
        .document()
        .history_entries()
        .last()
        .unwrap()
        .summary
        .clone();
    let mut h = harness(app);
    h.run_steps(3);
    assert!(h.query_by_label("Now · Insert newline").is_some());
    assert_eq!(
        h.state()
            .tab_manager
            .active_tab()
            .unwrap()
            .active_buffer()
            .document()
            .history_entries()
            .last()
            .unwrap()
            .summary,
        summary
    );
}

#[test]
fn scrubber_accessibility_actions_use_the_same_navigation() {
    let root = tempfile::tempdir().unwrap();
    let mut h = harness(scrubber_app(root.path()));
    h.run_steps(3);
    h.get_by_label("History position").set_value_accesskit(0.0);
    h.run_steps(3);
    assert_eq!(text(h.state()), "");
    h.get_by_label("History position").increment_accesskit();
    h.run_steps(3);
    assert_eq!(text(h.state()), "base");
    h.get_by_label("History position").decrement_accesskit();
    h.run_steps(3);
    assert_eq!(text(h.state()), "");
}

#[test]
fn file_picker_search_and_enter_restore_history_navigation_focus() {
    let root = tempfile::tempdir().unwrap();
    let mut app = app(root.path());
    let original = app.tab_manager.active_tab().unwrap().clone();
    let other = BufferState::new("other.txt".into(), "other text".into(), None);
    let id = other.id;
    app.tab_manager
        .set_tabs(vec![original, WorkspaceTab::new(other)], 0);
    let mut h = harness(app);
    h.run_steps(3);
    h.get_by_label("Choose history file").click();
    h.run_steps(3);
    h.event(egui::Event::Text("other".into()));
    h.run_steps(3);
    assert!(egui::Popup::is_any_open(&h.ctx));
    assert!(
        h.query_by_role_and_label(egui::accesskit::Role::Button, "other.txt")
            .is_some()
    );
    h.key_press(egui::Key::Enter);
    h.run_steps(3);
    assert!(!egui::Popup::is_any_open(&h.ctx));
    assert_eq!(active_buffer(h.state()), Some(id));
    assert_eq!(text(h.state()), "other text");
    assert!(h.ctx.memory(|m| m.has_focus(focus_id())));
    h.key_press(egui::Key::Home);
    h.run_steps(3);
    assert_eq!(text(h.state()), "");
}

#[test]
fn collapsed_branch_is_display_only_and_reveals_now_when_navigated_into() {
    let root = tempfile::tempdir().unwrap();
    let mut app = app(root.path());
    let buffer = app
        .tab_manager
        .active_tab_mut()
        .unwrap()
        .active_buffer_mut();
    let id = buffer.id;
    buffer.navigate_history(1).unwrap();
    buffer
        .replace_char_ranges_with_undo(
            &[(0..4, "alternate".into())],
            CursorRange::default(),
            CursorRange::default(),
        )
        .unwrap();
    let mut h = harness(app);
    h.run_steps(3);
    h.get_all_by_label("Collapse branch")
        .last()
        .unwrap()
        .click();
    h.run_steps(3);
    assert_eq!(text(h.state()), "alternate");
    assert!(h.query_by_label("Expand branch").is_some());
    let ctx = h.ctx.clone();
    assert!(navigate(&ctx, h.state_mut(), id, 3));
    h.run_steps(3);
    assert_eq!(text(h.state()), "two");
    assert!(h.query_by_label("Expand branch").is_none());
    assert!(h.query_by_label("Now · Replace match").is_some());
    h.get_all_by_label("Collapse branch")
        .last()
        .unwrap()
        .click();
    h.run_steps(3);
    assert!(h.query_by_label("Now · Replace match").is_none());
    h.get_by_label("Scroll to Now").click();
    h.run_steps(5);
    assert_eq!(text(h.state()), "two");
    assert!(h.query_by_label("Now · Replace match").is_some());
}
