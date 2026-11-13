//! Shared headless fixtures; callers own the temporary directory lifetime.
use super::ScratchpadApp;
use crate::app::domain::BufferState;
use crate::app::fonts::{EditorFontPreset, EditorFontSelection, apply_editor_fonts};
use crate::app::services::file_service::FileService;
use crate::app::services::{session_store::SessionStore, settings_store::SettingsStore};
use crate::app::startup::StartupOptions;
use eframe::{App, egui};
use egui_kittest::Harness;
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

pub(crate) fn test_app(root: &Path) -> ScratchpadApp {
    let mut app = ScratchpadApp::with_stores_and_startup(
        SessionStore::new(root.join("session")),
        SettingsStore::new(root.join("settings")),
        StartupOptions::default(),
    );
    app.set_session_persist_on_drop(false);
    app
}

/// Writes `text` to `root/name` and returns the file path.
pub(crate) fn write_file(root: &Path, name: &str, text: &str) -> PathBuf {
    let path = root.join(name);
    std::fs::write(&path, text).unwrap();
    path
}

/// A clean buffer for an existing file, synchronized with its on-disk state.
pub(crate) fn disk_buffer(path: &Path, text: &str) -> BufferState {
    let name = path.file_name().unwrap().to_string_lossy().into_owned();
    let mut buffer = BufferState::new(name, text.into(), Some(path.to_path_buf()));
    buffer.sync_to_disk_state(FileService::read_disk_state(path).ok());
    buffer
}

/// Installs the standard editor fonts used by rendered UI tests.
pub(crate) fn apply_test_fonts(ctx: &egui::Context) {
    apply_editor_fonts(
        ctx,
        &EditorFontSelection::scratchpad(EditorFontPreset::Standard),
        4.0,
    )
    .unwrap();
}

/// A kittest harness rendering `render` with standard fonts and ID-clash
/// warnings, already stepped past first-frame layout.
pub(crate) fn ui_harness<'a>(
    size: egui::Vec2,
    app: ScratchpadApp,
    render: impl FnMut(&mut egui::Ui, &mut ScratchpadApp) + 'a,
) -> Harness<'a, ScratchpadApp> {
    let mut harness = Harness::builder()
        .with_size(size)
        .with_context_setup(|ctx| {
            apply_test_fonts(ctx);
            ctx.options_mut(|o| o.warn_on_id_clash = true);
        })
        .build_ui_state(render, app);
    harness.run_steps(3);
    harness
}

pub(crate) fn logic(
    app: &mut ScratchpadApp,
    ctx: &egui::Context,
    input: &egui::RawInput,
) -> egui::LogicOutput {
    ctx.run_logic(input, |ctx| {
        App::logic(app, ctx, &mut eframe::Frame::_new_kittest());
    })
}

pub(crate) fn close_input() -> egui::RawInput {
    let mut input = egui::RawInput::default();
    input
        .viewports
        .get_mut(&egui::ViewportId::ROOT)
        .unwrap()
        .events
        .push(egui::ViewportEvent::Close);
    input
}

pub(crate) fn wait_for_background_io(app: &mut ScratchpadApp) {
    assert!(
        app.wait_for_background_io_idle_timeout(Duration::from_secs(5)),
        "background work did not finish"
    );
}
