use crate::app::app_state::ScratchpadApp;
use crate::app::commands::AppCommand;
use crate::app::domain::SplitAxis;
use eframe::egui;

const DEFAULT_SPLIT_RATIO: f32 = 0.5;

pub(crate) fn handle_shortcuts(app: &mut ScratchpadApp, ctx: &egui::Context) {
    handle_global_shortcuts(app, ctx);
    handle_file_shortcuts(app, ctx);
    handle_view_shortcuts(app, ctx);
    handle_tile_shortcuts(app, ctx);
    handle_tab_shortcuts(app, ctx);
}

fn handle_global_shortcuts(app: &mut ScratchpadApp, ctx: &egui::Context) {
    if ctx.input_mut(|input| input.consume_key(egui::Modifiers::CTRL, egui::Key::Comma)) {
        app.handle_command(AppCommand::OpenSettings);
        return;
    }

    if app.showing_settings()
        && ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
    {
        app.handle_command(AppCommand::CloseSettings);
    }
}

fn handle_file_shortcuts(app: &mut ScratchpadApp, ctx: &egui::Context) {
    let tile_file_modifiers = egui::Modifiers {
        ctrl: true,
        shift: true,
        ..Default::default()
    };

    if ctx.input_mut(|input| input.consume_key(tile_file_modifiers, egui::Key::O)) {
        app.handle_command(AppCommand::OpenFileHere);
        return;
    }
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
        app.set_font_size(app.font_size() + 1.0);
    }
    if ctx.input_mut(|input| input.consume_key(egui::Modifiers::CTRL, egui::Key::Minus)) {
        app.set_font_size(app.font_size() - 1.0);
    }
    if ctx.input_mut(|input| input.consume_key(egui::Modifiers::CTRL, egui::Key::Num0))
        && let Some(tab) = app.active_tab_mut()
    {
        let next_visible = !tab.line_numbers_visible();
        tab.set_line_numbers_visible(next_visible);
        app.mark_session_dirty();
    }
}

fn handle_tab_shortcuts(app: &mut ScratchpadApp, ctx: &egui::Context) {
    if ctx.input_mut(|input| input.consume_key(egui::Modifiers::CTRL, egui::Key::W)) {
        if app.showing_settings() {
            app.handle_command(AppCommand::CloseSettings);
        } else if !app.tabs().is_empty() {
            app.handle_command(AppCommand::RequestCloseTab {
                index: app.active_tab_index(),
            });
        }
    }
}

fn handle_tile_shortcuts(app: &mut ScratchpadApp, ctx: &egui::Context) {
    let modifiers = egui::Modifiers {
        ctrl: true,
        shift: true,
        ..Default::default()
    };

    if ctx.input_mut(|input| input.consume_key(modifiers, egui::Key::W))
        && let Some(tab) = app.active_tab()
        && tab.root_pane.leaf_count() > 1
    {
        app.handle_command(AppCommand::CloseView {
            view_id: tab.active_view_id,
        });
        return;
    }

    let split = ctx.input_mut(|input| {
        if input.consume_key(modifiers, egui::Key::ArrowUp) {
            Some((SplitAxis::Horizontal, true))
        } else if input.consume_key(modifiers, egui::Key::ArrowDown) {
            Some((SplitAxis::Horizontal, false))
        } else if input.consume_key(modifiers, egui::Key::ArrowLeft) {
            Some((SplitAxis::Vertical, true))
        } else if input.consume_key(modifiers, egui::Key::ArrowRight) {
            Some((SplitAxis::Vertical, false))
        } else {
            None
        }
    });

    if let Some((axis, new_view_first)) = split {
        app.handle_command(AppCommand::SplitActiveView {
            axis,
            new_view_first,
            ratio: DEFAULT_SPLIT_RATIO,
        });
    }
}
