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
    for (key, expected) in [
        (egui::Key::ArrowUp, "middle"),
        (egui::Key::ArrowDown, "old"),
        (egui::Key::Home, "current"),
        (egui::Key::End, "old"),
    ] {
        h.key_press(key);
        h.run_steps(3);
        assert_eq!(text(h.state()), expected);
        assert_eq!(h.state().tab_manager.active_tab_index, 1);
    }
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
    assert!(h.query_by_label("other  ·  0").is_none());
    assert_eq!(
        h.get_by_label("note.txt  ·  2").rect().height(),
        crate::app::theme::TAB_HEIGHT
    );
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
    assert_eq!(h.state().tab_manager.active_tab_index, 0);
    assert_eq!(
        h.state()
            .tab_manager
            .active_tab()
            .unwrap()
            .active_buffer()
            .id,
        id
    );
    let latest = h.state().tab_manager.tabs.as_slice()[0]
        .active_buffer()
        .document()
        .history_entries()[1]
        .summary
        .clone();
    h.get_all_by_role_and_label(egui::Role::Button, &format!("State: {latest}"))
        .next()
        .unwrap()
        .click();
    h.run_steps(3);
    assert_eq!(text(h.state()), "current");
    assert_eq!(
        crate::app::ui::editor_content::native_editor::history_feedback::active_offset(
            &h.ctx,
            h.state().tab_manager.active_tab().unwrap().active_buffer(),
        ),
        Some(0)
    );
}

#[test]
fn file_picker_keeps_redo_history_and_distinguishes_duplicate_names() {
    let root = tempfile::tempdir().unwrap();
    let mut app = app_with_history(root.path());
    let first = app.tab_manager.tabs.as_slice()[0].clone();
    let mut second = BufferState::new("note.txt".into(), "old".into(), None);
    second
        .replace_char_ranges_with_undo(
            &[(0..3, "new".into())],
            CursorRange::default(),
            CursorRange::default(),
        )
        .unwrap();
    let second_id = second.id;
    let unchanged = WorkspaceTab::new(BufferState::new(
        "unchanged.txt".into(),
        "text".into(),
        None,
    ));
    app.tab_manager
        .set_tabs(vec![first, WorkspaceTab::new(second), unchanged], 0);
    assert!(jump(&egui::Context::default(), &mut app, second_id, 0));
    app.state.dialogs.text_history.open();
    let mut h = harness(app);
    h.run_steps(3);
    h.get_by_label("Choose file").click();
    h.run_steps(3);
    assert!(h.query_by_label("unchanged.txt  ·  0").is_none());
    h.get_by_label("note.txt  ·  2");
    h.get_by_label("note.txt  ·  1").click();
    h.run_steps(3);
    assert_eq!(
        super::super::scope(&h.ctx, h.state()),
        super::super::HistoryTab::File(second_id)
    );
    assert!(h.state().state.dialogs.text_history.is_open());
    assert!(!egui::Popup::is_any_open(&h.ctx));
    assert_eq!(
        h.state()
            .tab_manager
            .active_tab()
            .unwrap()
            .active_buffer()
            .id,
        second_id
    );
    // Re-selecting the same History scope must still return from another editor tab.
    h.state_mut().tab_manager.set_active_tab_index_clamped(0);
    h.run_steps(3);
    h.get_by_label("Choose file").click();
    h.run_steps(3);
    h.get_by_label("note.txt  ·  1").click();
    h.run_steps(3);
    assert_eq!(
        h.state()
            .tab_manager
            .active_tab()
            .unwrap()
            .active_buffer()
            .id,
        second_id
    );
}

#[test]
fn choosing_a_history_file_activates_its_split_view() {
    let root = tempfile::tempdir().unwrap();
    let mut app = app_with_history(root.path());
    let mut tab = app.tab_manager.tabs.as_slice()[0].clone();
    let id = tab.active_buffer().id;
    let original_view = tab.layout.active_view_id();
    tab.open_buffer_as_split(
        BufferState::new("other.txt".into(), "other".into(), None),
        crate::app::domain::SplitAxis::Vertical,
        false,
        0.5,
    )
    .unwrap();
    app.tab_manager.set_tabs(vec![tab], 0);
    app.state.dialogs.text_history.open();
    let mut h = harness(app);
    h.run_steps(3);
    h.get_by_label("Choose file").click();
    h.run_steps(3);
    h.get_by_label("note.txt  ·  2").click();
    h.run_steps(3);
    let tab = h.state().tab_manager.active_tab().unwrap();
    assert_eq!(tab.layout.active_view_id(), original_view);
    assert_eq!(tab.active_buffer().id, id);
}

#[test]
fn large_history_counts_and_long_names_do_not_expand_the_panel() {
    let root = tempfile::tempdir().unwrap();
    let mut app = app_with_history(root.path());
    let buffer = app.tab_manager.tabs.as_mut_slice()[0].active_buffer_mut();
    buffer.name = "a-very-long-filename-".repeat(40);
    for index in 0..130 {
        let end = buffer.current_file_length().chars;
        buffer
            .replace_char_ranges_with_undo(
                &[(0..end, format!("change {index}"))],
                CursorRange::default(),
                CursorRange::default(),
            )
            .unwrap();
    }
    assert!(buffer.document().history_entries().len() >= 130);
    app.state.dialogs.text_history.open();
    let mut h = harness(app);
    h.run_steps(3);
    let width = |h: &Harness<'_, ScratchpadApp>| {
        h.ctx.memory(|memory| {
            memory
                .area_rect(crate::app::ui::widget_ids::area_id("text_history_window"))
                .unwrap()
                .width()
        })
    };
    let initial = width(&h);
    h.run_steps(35);
    assert!(
        (width(&h) - initial).abs() < 1.0,
        "{initial} -> {}",
        width(&h)
    );
    assert!(width(&h) <= super::super::super::common::HISTORY_DIALOG_SIZE.x + 2.0);
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
    app.state.app_settings.platform.profile = crate::app::platform::PlatformProfile::Windows;
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
fn history_escape_yields_to_search_rename_and_settings() {
    use crate::app::app_state::{settings_state, workspace::accessors};
    for owner in ["search", "rename", "settings"] {
        let root = tempfile::tempdir().unwrap();
        let mut app = app_with_history(root.path());
        app.state.app_settings.platform.profile = crate::app::platform::PlatformProfile::Hyprland;
        let mut h = harness(app);
        h.run_steps(3);
        let app = h.state_mut();
        match owner {
            "search" => crate::app::commands::handle_command(
                app,
                crate::app::commands::AppCommand::Search(crate::app::commands::SearchCommand::Open),
            ),
            "rename" => {
                accessors::begin_tab_rename(app, 0);
                false
            }
            _ => crate::app::commands::handle_command(
                app,
                crate::app::commands::AppCommand::Settings(
                    crate::app::commands::SettingsCommand::OpenSettings,
                ),
            ),
        };
        app.state.dialogs.text_history.open();
        h.run_steps(3);
        h.key_press(egui::Key::Escape);
        h.run_steps(3);
        assert!(
            h.state().state.dialogs.text_history.is_open(),
            "{owner} should own Escape"
        );
        match owner {
            "search" => assert!(!h.state().state.search_state.open()),
            "rename" => assert!(!accessors::tab_rename_matches_slot(h.state(), 0)),
            _ => assert!(!settings_state::showing_settings(h.state())),
        }
    }
}

#[test]
fn linux_history_opens_from_click_and_dismisses_without_a_close_button() {
    use crate::app::platform::PlatformProfile;
    for profile in [PlatformProfile::LinuxGeneric, PlatformProfile::Hyprland] {
        let root = tempfile::tempdir().unwrap();
        let mut app = app_with_history(root.path());
        app.state.app_settings.platform.profile = profile;
        app.state.app_settings.ui.status_bar_visible = true;
        let mut h = harness(app);
        h.run_steps(3);
        h.get_by_role_and_label(
            egui::Role::Label,
            egui_phosphor::regular::CLOCK_COUNTER_CLOCKWISE,
        )
        .click();
        h.run_steps(3);
        assert!(h.state().state.dialogs.text_history.is_open());
        assert!(h.query_by_label("Close history").is_none());

        h.get_by_label("Choose file").click();
        h.run_steps(3);
        h.key_press(egui::Key::Escape);
        h.run_steps(3);
        assert!(!egui::Popup::is_any_open(&h.ctx));
        assert!(h.state().state.dialogs.text_history.is_open());
        h.key_press(egui::Key::Escape);
        h.run_steps(3);
        assert!(!h.state().state.dialogs.text_history.is_open());

        h.get_by_role_and_label(
            egui::Role::Label,
            egui_phosphor::regular::CLOCK_COUNTER_CLOCKWISE,
        )
        .click();
        h.run_steps(3);
        // An editor click is outside the panel and must dismiss it.
        let pos = egui::pos2(100.0, 300.0);
        h.hover_at(pos);
        h.drag_at(pos);
        h.run_steps(2);
        h.drop_at(pos);
        h.run_steps(3);
        assert!(!h.state().state.dialogs.text_history.is_open());
    }
}

#[test]
fn history_resizes_and_remembers_its_size_when_reopened() {
    let root = tempfile::tempdir().unwrap();
    let mut app = app_with_history(root.path());
    app.state.app_settings.platform.profile = crate::app::platform::PlatformProfile::Hyprland;
    app.state.dialogs.text_history.open();
    let mut h = harness(app);
    h.run_steps(3);
    let rect = |h: &Harness<'_, ScratchpadApp>| {
        h.ctx.memory(|m| {
            m.area_rect(crate::app::ui::widget_ids::area_id("text_history_window"))
                .unwrap()
        })
    };
    let before = rect(&h);
    let rail_before = h.get_by_label("History states").rect();
    let start = before.right_bottom() - egui::vec2(2.0, 2.0);
    let end = start - egui::vec2(120.0, 140.0);
    h.hover_at(start);
    h.run_steps(2);
    h.drag_at(start);
    h.run_steps(2);
    h.hover_at(end);
    h.run_steps(3);
    h.drop_at(end);
    h.run_steps(3);
    let after = rect(&h);
    assert!(
        after.width() < before.width() - 80.0,
        "{before:?} -> {after:?}"
    );
    assert!(
        after.height() < before.height() - 100.0,
        "{before:?} -> {after:?}"
    );
    assert!(h.get_by_label("History states").rect().height() < rail_before.height() - 100.0);
    assert!(h.state().state.dialogs.text_history.is_open());
    h.key_press(egui::Key::Escape);
    h.run_steps(3);
    h.state_mut().state.dialogs.text_history.open();
    h.run_steps(3);
    assert!((rect(&h).size() - after.size()).length() < 2.0);
}

#[test]
fn scrubber_drag_replays_live_with_feedback_and_cancels_when_the_document_changes() {
    let root = tempfile::tempdir().unwrap();
    let mut app = app_with_history(root.path());
    crate::app::app_state::search_controller::open_search(&mut app);
    crate::app::app_state::search_controller::set_search_query(&mut app, "current");
    app.state.dialogs.history_scrubber.open();
    let mut h = harness(app);
    h.run_steps(3);
    let rect = h.get_by_label("History position").rect();
    let start = egui::pos2(rect.right() - 9.0, rect.center().y);
    let end = egui::pos2(rect.left() + 9.0, rect.center().y);
    let middle = rect.center();
    let feedback = |h: &Harness<'_, ScratchpadApp>| {
        crate::app::ui::editor_content::native_editor::history_feedback::active_offset(
            &h.ctx,
            h.state().tab_manager.active_tab().unwrap().active_buffer(),
        )
    };
    let press = |pos, pressed| egui::Event::PointerButton {
        pos,
        pressed,
        button: egui::PointerButton::Primary,
        modifiers: egui::Modifiers::NONE,
    };
    h.event(egui::Event::PointerMoved(start));
    h.event(press(start, true));
    h.run_steps(2);
    let mut search_generation = None;
    for (position, expected) in [
        (middle, "middle"),
        (end, "old"),
        (start, "current"),
        (end, "old"),
    ] {
        h.event(egui::Event::PointerMoved(position));
        h.run_steps(2);
        assert_eq!(
            text(h.state()),
            expected,
            "dragging must replay before release"
        );
        assert_eq!(feedback(&h), Some(0));
        assert!(super::super::history_scrubbing(&h.ctx, h.state()));
        let generation = h.state().state.search_state.runtime.requested_generation;
        assert_eq!(
            *search_generation.get_or_insert(generation),
            generation,
            "search should wait until the drag ends"
        );
    }
    let revision = h
        .state()
        .tab_manager
        .active_tab()
        .unwrap()
        .active_buffer()
        .document()
        .history_revision_counter();
    h.run_steps(3);
    h.event(press(end, false));
    h.run_steps(3);
    assert_eq!(text(h.state()), "old");
    assert!(!super::super::history_scrubbing(&h.ctx, h.state()));
    assert!(h.state().state.search_state.runtime.requested_generation > search_generation.unwrap());
    assert_eq!(
        h.state()
            .tab_manager
            .active_tab()
            .unwrap()
            .active_buffer()
            .document()
            .history_revision_counter(),
        revision,
        "holding and releasing must not replay again"
    );
    assert_eq!(
        std::fs::read_to_string(root.path().join("note.txt")).unwrap(),
        "old"
    );
    h.event(egui::Event::PointerMoved(end));
    h.event(press(end, true));
    h.run_steps(2);
    h.event(egui::Event::PointerMoved(start));
    h.run_steps(2);
    assert_eq!(text(h.state()), "current");
    h.state_mut().tab_manager.tabs.as_mut_slice()[0]
        .active_buffer_mut()
        .replace_text("new edit".into());
    h.run_steps(2);
    h.event(egui::Event::PointerMoved(end));
    h.run_steps(2);
    h.event(press(end, false));
    h.run_steps(3);
    assert_eq!(
        text(h.state()),
        "new edit",
        "stale drag must not replay over edits"
    );
}

#[test]
fn scrubber_follows_tabs_and_a_tab_switch_cancels_a_live_drag() {
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
    assert_eq!(text(h.state()), "old");
    h.state_mut().tab_manager.set_active_tab_index_clamped(1);
    h.run_steps(2);
    h.event(egui::Event::PointerButton {
        pos: end,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::NONE,
    });
    h.run_steps(3);
    assert_eq!(text(h.state()), "old");
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
    assert_eq!(text(h.state()), "old");
}

#[test]
fn clear_requires_confirmation_and_cancel_preserves_history() {
    let root = tempfile::tempdir().unwrap();
    let mut app = app_with_history(root.path());
    app.state.dialogs.text_history.open();
    let mut h = harness(app);
    h.run_steps(3);
    assert!(h.query_by_label("History options").is_none());
    h.get_by_label("Switch to changed file").click();
    h.run_steps(2);
    assert!(!super::super::read_follow_focus(&h.ctx));
    h.get_by_label("Clear undo/redo history").click();
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
    h.get_by_label("Clear undo/redo history").click();
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
    h.get_by_label("Choose file").click();
    h.run_steps(3);
    h.get_by_label("No files with changes");
}

#[test]
fn clearing_history_respects_scope_and_keeps_the_confirmed_target() {
    use super::super::{HistoryTab, scope_key, write_active_tab};
    for mode in ["current", "picked", "all"] {
        let root = tempfile::tempdir().unwrap();
        let mut app = app_with_history(root.path());
        let first = app.tab_manager.tabs.as_slice()[0].clone();
        let mut other = BufferState::new("other.txt".into(), "before".into(), None);
        other
            .replace_char_ranges_with_undo(
                &[(0..6, "after".into())],
                CursorRange::default(),
                CursorRange::default(),
            )
            .unwrap();
        let other_id = other.id;
        app.tab_manager
            .set_tabs(vec![first, WorkspaceTab::new(other)], 0);
        app.state.dialogs.text_history.open();
        let mut h = harness(app);
        match mode {
            "all" => write_active_tab(&h.ctx, HistoryTab::AllFiles),
            "picked" => {
                h.ctx.data_mut(|d| d.insert_temp(scope_key(), other_id));
            }
            _ => {}
        }
        h.run_steps(3);
        h.get_by_label("Clear undo/redo history").click();
        h.run_steps(2);
        let description = match mode {
            "all" => "Clear undo/redo history in all open files?",
            "picked" => "Clear undo/redo history in other.txt?",
            _ => "Clear undo/redo history in note.txt?",
        };
        h.get_by_label(description);
        // Changing the current editor after confirmation must not change its target.
        h.state_mut().tab_manager.set_active_tab_index_clamped(1);
        h.run_steps(2);
        h.get_by_label(description);
        h.get_by_role_and_label(egui::Role::Button, "Clear history")
            .click();
        h.run_steps(3);
        let tabs = h.state().tab_manager.tabs.as_slice();
        assert_eq!(
            tabs[0].active_buffer().document().history_entries().len(),
            if mode == "picked" { 2 } else { 0 }
        );
        assert_eq!(
            tabs[1].active_buffer().document().history_entries().len(),
            if mode == "current" { 1 } else { 0 }
        );
        assert_eq!(tabs[0].active_buffer().text(), "current");
        assert_eq!(tabs[1].active_buffer().text(), "after");
    }
}
