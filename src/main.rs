#![forbid(unsafe_code)]
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eframe::egui;
use scratchpad::ScratchpadApp;
use scratchpad::app::services::session_store::SessionStore;
use scratchpad::app::services::settings_store::{
    DEFAULT_WINDOW_INNER_SIZE, MIN_WINDOW_INNER_SIZE, SettingsStore, WindowState,
};
use scratchpad::app::startup::StartupOptions;

fn main() -> eframe::Result<()> {
    let startup_action = scratchpad::app::startup::parse_startup_action_from_env();
    match &startup_action {
        scratchpad::app::startup::StartupAction::Help => {
            println!("{}", scratchpad::app::startup::USAGE_TEXT);
            return Ok(());
        }
        scratchpad::app::startup::StartupAction::Version => {
            println!("scratchpad {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        _ => {}
    }

    let session_store = SessionStore::default();
    let settings_store = SettingsStore::new(session_store.root().to_path_buf());
    let startup_settings = settings_store.load().ok().flatten().unwrap_or_default();

    let options = eframe::NativeOptions {
        viewport: viewport_builder_from_window_state(&startup_settings.window_state),
        persist_window: false,
        persistence_path: Some(session_store.root().join("eframe-state.ron")),
        ..Default::default()
    };

    eframe::run_native(
        "Scratchpad",
        options,
        Box::new(|cc| {
            let startup_options = match &startup_action {
                scratchpad::app::startup::StartupAction::Run(options) => options.clone(),
                scratchpad::app::startup::StartupAction::Help
                | scratchpad::app::startup::StartupAction::Version => StartupOptions::default(),
            };
            let mut app = ScratchpadApp::with_stores_and_runtime_startup(
                session_store,
                settings_store,
                startup_options,
            );
            app.prepare_context_before_first_frame(&cc.egui_ctx);
            cc.egui_ctx.options_mut(|o| o.zoom_with_keyboard = false);
            Ok(Box::new(app))
        }),
    )
}

fn viewport_builder_from_window_state(window_state: &WindowState) -> egui::ViewportBuilder {
    let mut viewport = egui::ViewportBuilder::default()
        .with_decorations(false)
        .with_visible(false)
        .with_inner_size(window_state.inner_size.unwrap_or(DEFAULT_WINDOW_INNER_SIZE))
        .with_min_inner_size(MIN_WINDOW_INNER_SIZE);

    if let Some(position) = window_state.position {
        viewport = viewport.with_position(egui::pos2(position[0], position[1]));
    }

    viewport
}
