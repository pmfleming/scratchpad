use crate::app::app_state::ScratchpadApp;
use crate::app::commands::AppCommand;
use eframe::egui;

pub(crate) fn handle_shortcuts(app: &mut ScratchpadApp, ctx: &egui::Context) {
    handle_file_shortcuts(app, ctx);
    handle_view_shortcuts(app, ctx);
    handle_tab_shortcuts(app, ctx);
}

fn handle_file_shortcuts(app: &mut ScratchpadApp, ctx: &egui::Context) {
    if ctx.input_mut(|input| input.consume_key(egui::Modifiers::CTRL, egui::Key::N)) {
        app.handle_command(AppCommand::NewTab);
    }
    if ctx.input_mut(|input| input.consume_key(egui::Modifiers::CTRL, egui::Key::O)) {
        app.handle_command(AppCommand::OpenFile);
    }
    if ctx.input_mut(|input| input.consume_key(egui::Modifiers::CTRL, egui::Key::S)) {
        app.handle_command(AppCommand::SaveFile);
    }
}

fn handle_view_shortcuts(app: &mut ScratchpadApp, ctx: &egui::Context) {
    if ctx.input_mut(|input| {
        input.consume_key(egui::Modifiers::CTRL, egui::Key::Equals)
            || input.consume_key(egui::Modifiers::CTRL, egui::Key::Plus)
    }) {
        app.font_size = (app.font_size + 1.0).min(72.0);
        app.mark_session_dirty();
    }
    if ctx.input_mut(|input| input.consume_key(egui::Modifiers::CTRL, egui::Key::Minus)) {
        app.font_size = (app.font_size - 1.0).max(8.0);
        app.mark_session_dirty();
    }
    if ctx.input_mut(|input| input.consume_key(egui::Modifiers::CTRL, egui::Key::Num0)) {
        app.font_size = 14.0;
        app.mark_session_dirty();
    }
}

fn handle_tab_shortcuts(app: &mut ScratchpadApp, ctx: &egui::Context) {
    if ctx.input_mut(|input| input.consume_key(egui::Modifiers::CTRL, egui::Key::W))
        && !app.tabs.is_empty()
    {
        app.handle_command(AppCommand::RequestCloseTab {
            index: app.active_tab_index,
        });
    }
}
