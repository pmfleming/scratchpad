use super::show_encoding_window;
use crate::app::app_state::{
    ScratchpadApp, frame,
    test_support::{disk_buffer, test_app, ui_harness, wait_for_background_io, write_file},
};
use crate::app::domain::WorkspaceTab;
use eframe::egui;
use egui_kittest::{
    Harness,
    kittest::{NodeT, Queryable},
};

struct Fixture {
    _root: tempfile::TempDir,
    path: std::path::PathBuf,
}

fn app_with_file(dirty: bool) -> (Fixture, ScratchpadApp) {
    let root = tempfile::tempdir().unwrap();
    let path = write_file(root.path(), "notes.txt", "text");
    let mut app = test_app(root.path());
    let mut buffer = disk_buffer(&path, "text");
    buffer.is_dirty = dirty;
    app.tab_manager.tabs.as_mut_slice()[0] = WorkspaceTab::new(buffer);
    frame::open_encoding_dialog(&mut app);
    (Fixture { _root: root, path }, app)
}

fn harness(app: ScratchpadApp) -> Harness<'static, ScratchpadApp> {
    ui_harness(egui::vec2(1100.0, 900.0), app, |ui, app| {
        show_encoding_window(ui.ctx(), app);
    })
}

fn press(harness: &mut Harness<'static, ScratchpadApp>, key: egui::Key) {
    harness.key_press(key);
    harness.run_steps(2);
}

#[test]
fn a_clean_saved_file_opens_in_reopen_mode_on_its_encoding() {
    let (_fixture, app) = app_with_file(false);
    let mut harness = harness(app);
    harness.get_by_role_and_label(egui::Role::Button, "Reopen as UTF-8");
    harness.get_by_label_contains("notes.txt · UTF-8");
    assert!(harness.query_by_label("UTF-8 with BOM").is_none());
    harness.get_by_label("Shift_JIS").click();
    harness.run_steps(2);
    harness.get_by_role_and_label(egui::Role::Button, "Reopen as Shift_JIS");
}

#[test]
fn a_modified_file_opens_in_save_mode_and_explains_why_reopen_is_unavailable() {
    let (_fixture, app) = app_with_file(true);
    let mut harness = harness(app);
    harness.get_by_role_and_label(egui::Role::Button, "Save as UTF-8");
    harness
        .get_by_role_and_label(egui::Role::Button, "Reopen")
        .click();
    harness.run_steps(2);
    let reopen = harness.get_by_role_and_label(egui::Role::Button, "Reopen as UTF-8");
    assert!(reopen.accesskit_node().is_disabled());
    harness.get_by_label_contains("Save or discard changes");
}

#[test]
fn arrows_choose_an_encoding_and_escape_closes() {
    let (_fixture, app) = app_with_file(false);
    let mut harness = harness(app);
    press(&mut harness, egui::Key::ArrowDown);
    assert_eq!(harness.state().state.dialogs.encoding.choice, "UTF-16LE");
    press(&mut harness, egui::Key::End);
    assert_eq!(harness.state().state.dialogs.encoding.choice, "EUC-KR");
    press(&mut harness, egui::Key::Home);
    assert_eq!(harness.state().state.dialogs.encoding.choice, "UTF-8");
    press(&mut harness, egui::Key::Escape);
    assert!(!harness.state().state.dialogs.encoding.is_open());
}

#[test]
fn the_bom_toggle_appears_for_unicode_saves_and_writes_a_bom() {
    let (fixture, app) = app_with_file(true);
    let mut harness = harness(app);
    harness.get_by_label("Shift_JIS").click();
    harness.run_steps(2);
    assert!(harness.query_by_label("Shift_JIS with BOM").is_none());
    harness.get_by_label("UTF-8").click();
    harness.run_steps(2);
    harness.get_by_label("UTF-8 with BOM").click();
    harness.run_steps(2);
    harness
        .get_by_role_and_label(egui::Role::Button, "Save as UTF-8")
        .click();
    harness.run_steps(2);
    assert!(!harness.state().state.dialogs.encoding.is_open());
    wait_for_background_io(harness.state_mut());
    assert_eq!(std::fs::read(&fixture.path).unwrap(), b"\xEF\xBB\xBFtext");
}
