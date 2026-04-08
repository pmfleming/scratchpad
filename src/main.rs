#![forbid(unsafe_code)]
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eframe::egui;
use scratchpad::ScratchpadApp;

fn main() -> eframe::Result<()> {
    if let Err(error) = scratchpad::app::logging::init() {
        eprintln!("failed to initialize logging: {error}");
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
            let mut fonts = egui::FontDefinitions::default();
            egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
            cc.egui_ctx.set_fonts(fonts);

            let mut visuals = egui::Visuals::dark();
            visuals.widgets.noninteractive.fg_stroke.color = scratchpad::app::theme::TEXT_PRIMARY;
            visuals.widgets.inactive.fg_stroke.color = scratchpad::app::theme::TEXT_PRIMARY;
            visuals.widgets.hovered.fg_stroke.color = scratchpad::app::theme::TEXT_PRIMARY;
            visuals.widgets.active.fg_stroke.color = scratchpad::app::theme::TEXT_PRIMARY;
            visuals.widgets.open.fg_stroke.color = scratchpad::app::theme::TEXT_PRIMARY;
            cc.egui_ctx.set_visuals(visuals);
            cc.egui_ctx.options_mut(|o| o.zoom_with_keyboard = false);
            Ok(Box::new(ScratchpadApp::default()))
        }),
    )
}
