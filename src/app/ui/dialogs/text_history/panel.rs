use super::{HistoryUi, style, style::window};
use crate::app::{app_state::ScratchpadApp, ui::widget_ids};
use eframe::egui;

const HISTORY_DIALOG_SIZE: egui::Vec2 = egui::vec2(440.0, 580.0);
const HISTORY_MIN_SIZE: egui::Vec2 = egui::vec2(320.0, 240.0);

pub(super) fn show(
    ctx: &egui::Context,
    app: &mut ScratchpadApp,
    state: &mut HistoryUi,
    add_contents: impl FnOnce(&mut egui::Ui, &mut ScratchpadApp, &mut HistoryUi),
) -> bool {
    let layer = widget_ids::area_layer_id(egui::Order::Foreground, "text_history_window");
    let outside = window::OutsideClick::before_show(ctx, app, layer);
    let frame = style::window_frame(ctx, app.state.app_settings.editor_background_color());
    let mut close = false;
    let response = window::resizable(
        ctx,
        "History",
        layer,
        frame,
        HISTORY_DIALOG_SIZE,
        HISTORY_MIN_SIZE,
    )
    .show(ctx, |ui| {
        window::contents(ui, "text_history_dialog", frame.fill, |ui| {
            ui.horizontal(|ui| {
                window::title(ui, "History");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if outside.needs_close_button() {
                        close =
                            window::close_button(ui, "history.close", "Close history").clicked();
                    }
                    scope_switch(ctx, ui, state);
                });
            });
            add_contents(ui, app, state);
        });
    });
    if let Some(response) = response {
        ctx.data_mut(|d| d.insert_temp(super::rect_key(), response.response.rect));
        // Ignore the opening click and interactions with the panel's menus.
        close |=
            outside.after_show(ctx, app, &response.response) && !app.state.dialogs.any_modal_open();
    }
    close
}

/// Timeline lists every file; This file lists the active one. Switching starts over.
fn scope_switch(ctx: &egui::Context, ui: &mut egui::Ui, state: &mut HistoryUi) {
    let old_scope = state.all_files;
    ui.selectable_value(&mut state.all_files, true, "Timeline");
    ui.selectable_value(&mut state.all_files, false, "This file");
    if old_scope != state.all_files {
        state.search.clear();
        state.forget = None;
        state.restore_focus = true;
        egui::Popup::close_id(ctx, super::recent::picker_id());
    }
}
