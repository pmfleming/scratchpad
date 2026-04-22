#![forbid(unsafe_code)]
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eframe::egui;
use scratchpad::ScratchpadApp;
use scratchpad::app::fonts;
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

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_decorations(false)
            .with_inner_size([960.0, 640.0])
            .with_min_inner_size([400.0, 300.0]),
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
            let app = ScratchpadApp::with_runtime_startup_options(startup_options);
            let _ = fonts::apply_editor_fonts(&cc.egui_ctx, app.editor_font());
            app.apply_theme_to_context(&cc.egui_ctx);
            cc.egui_ctx.options_mut(|o| o.zoom_with_keyboard = false);
            Ok(Box::new(app))
        }),
    )
}
