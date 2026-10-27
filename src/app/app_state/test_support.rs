//! Shared headless fixtures; callers own the temporary directory lifetime.
use super::ScratchpadApp;
use crate::app::services::{session_store::SessionStore, settings_store::SettingsStore};
use crate::app::startup::StartupOptions;
use eframe::{App, egui};
use std::{path::Path, time::Duration};

pub(crate) fn test_app(root: &Path) -> ScratchpadApp {
    let mut app = ScratchpadApp::with_stores_and_startup(
        SessionStore::new(root.join("session")),
        SettingsStore::new(root.join("settings")),
        StartupOptions::default(),
    );
    app.set_session_persist_on_drop(false);
    app
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
