use super::show_pending_action_modal;
use crate::app::app_state::{ScratchpadApp, test_support::test_app, workspace::accessors};
use crate::app::domain::{BufferFreshness, BufferState, PendingAction, WorkspaceTab};
use eframe::egui;
use egui_kittest::{Harness, kittest::Queryable};

fn harness(app: ScratchpadApp) -> Harness<'static, ScratchpadApp> {
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1100.0, 800.0))
        .with_context_setup(|ctx| {
            crate::app::fonts::apply_editor_fonts(
                ctx,
                &crate::app::fonts::EditorFontSelection::scratchpad(
                    crate::app::fonts::EditorFontPreset::Standard,
                ),
                4.0,
            )
            .unwrap();
            ctx.options_mut(|o| o.warn_on_id_clash = true);
        })
        .build_ui_state(|ui, app| show_pending_action_modal(ui.ctx(), app), app);
    harness.run_steps(3);
    harness
}

/// A dirty tab for `path`; the file itself is written only when `disk` is set.
fn app_with_tab(
    root: &std::path::Path,
    disk: Option<&str>,
    freshness: BufferFreshness,
) -> ScratchpadApp {
    let path = root.join("notes.txt");
    if let Some(text) = disk {
        std::fs::write(&path, text).unwrap();
    }
    let mut app = test_app(root);
    let mut buffer = BufferState::new("notes.txt".into(), "one\nTWO\nthree".into(), Some(path));
    buffer.is_dirty = true;
    buffer.freshness = freshness;
    app.tab_manager.tabs.as_mut_slice()[0] = WorkspaceTab::new(buffer);
    app
}

fn press(harness: &mut Harness<'static, ScratchpadApp>, key: egui::Key) {
    harness.key_press(key);
    harness.run_steps(2);
}

#[test]
fn unsaved_changes_close_without_saving_or_cancel_with_escape() {
    let root = tempfile::tempdir().unwrap();
    let mut app = app_with_tab(root.path(), None, BufferFreshness::InSync);
    accessors::set_pending_action(&mut app, Some(PendingAction::CloseTab(0)));
    let mut harness = harness(app);
    harness.get_by_role_and_label(egui::Role::Window, "Unsaved changes");
    harness.get_by_label_contains("notes.txt");
    harness.get_by_label("Save");
    press(&mut harness, egui::Key::Escape);
    assert!(accessors::pending_action(harness.state()).is_none());
    assert_eq!(harness.state().tab_manager.tabs.as_slice().len(), 1);

    accessors::set_pending_action(harness.state_mut(), Some(PendingAction::CloseTab(0)));
    harness.run_steps(2);
    harness.get_by_label("Don't save").click();
    harness.run_steps(2);
    // Closing waits for the session write; let it finish.
    crate::app::app_state::test_support::wait_for_background_io(harness.state_mut());
    harness.run_steps(2);
    assert!(accessors::pending_action(harness.state()).is_none());
    assert!(
        harness
            .state()
            .tab_manager
            .tabs
            .iter()
            .all(|tab| tab.active_buffer().name != "notes.txt")
    );
}

#[test]
fn an_untitled_file_offers_save_with_a_picker() {
    let root = tempfile::tempdir().unwrap();
    let mut app = test_app(root.path());
    let mut buffer = BufferState::new("Untitled".into(), "draft".into(), None);
    buffer.is_dirty = true;
    app.tab_manager.tabs.as_mut_slice()[0] = WorkspaceTab::new(buffer);
    accessors::set_pending_action(&mut app, Some(PendingAction::CloseTab(0)));
    let harness = harness(app);
    harness.get_by_label("Save…");
}

#[test]
fn a_disk_change_shows_the_first_changed_lines() {
    let root = tempfile::tempdir().unwrap();
    let app = app_with_tab(
        root.path(),
        Some("one\ntwo\nthree"),
        BufferFreshness::ConflictOnDisk,
    );
    let view_id = app.tab_manager.tabs.as_slice()[0].layout.active_view_id();
    let mut app = app;
    accessors::set_pending_action(
        &mut app,
        Some(PendingAction::SaveConflict {
            tab_index: 0,
            view_id,
        }),
    );
    let mut harness = harness(app);
    harness.get_by_role_and_label(egui::Role::Window, "Changed on disk");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while harness.query_by_label("+ TWO").is_none() && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(10));
        harness.run_steps(1);
    }
    harness.get_by_label("Ln 2");
    harness.get_by_label("- two");
    harness.get_by_label("+ TWO");
    harness.get_by_label("Compare").click();
    harness.run_steps(3);
    harness.get_by_label("This tab");
    press(&mut harness, egui::Key::Escape);
    assert!(accessors::pending_action(harness.state()).is_none());
}

#[test]
fn a_missing_file_can_be_kept_for_later() {
    let root = tempfile::tempdir().unwrap();
    let mut app = app_with_tab(root.path(), None, BufferFreshness::MissingOnDisk);
    let view_id = app.tab_manager.tabs.as_slice()[0].layout.active_view_id();
    accessors::set_pending_action(
        &mut app,
        Some(PendingAction::SaveConflict {
            tab_index: 0,
            view_id,
        }),
    );
    let mut harness = harness(app);
    harness.get_by_role_and_label(egui::Role::Window, "Missing on disk");
    harness.get_by_label("Save as…");
    harness.get_by_label("Recreate");
    harness.get_by_label("Cancel").click();
    harness.run_steps(2);
    assert!(accessors::pending_action(harness.state()).is_none());
    assert_eq!(harness.state().tab_manager.tabs.as_slice().len(), 1);
}
