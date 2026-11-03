use super::super::common::{HISTORY_DIALOG_SIZE, history_dialog_header};
use crate::app::{
    app_state::{ScratchpadApp, workspace::accessors},
    theme,
    ui::{callout, settings, widget_ids},
};
use eframe::egui;

pub(super) fn show(
    ctx: &egui::Context,
    app: &mut ScratchpadApp,
    add_contents: impl FnOnce(&mut egui::Ui, &mut ScratchpadApp),
) -> bool {
    let screen = ctx.content_rect();
    let dismissible = super::dismissible_history(app);
    let popup_open = egui::Popup::is_any_open(ctx);
    let layer = widget_ids::area_layer_id(egui::Order::Foreground, "text_history_window");
    let was_visible = ctx.memory(|m| m.areas().visible_last_frame(&layer));
    let frame = egui::Frame::NONE
        .fill(app.state.app_settings.editor_background_color())
        .stroke(egui::Stroke::new(
            1.0,
            theme::border_for_visuals(&ctx.global_style().visuals),
        ))
        .inner_margin(egui::Margin::symmetric(12, 8));
    let mut close = false;
    let response = egui::Window::new("History")
        .id(layer.id)
        .order(layer.order)
        .title_bar(false)
        .resizable(true)
        .default_size(HISTORY_DIALOG_SIZE)
        .min_size(egui::vec2(320.0, 240.0))
        .frame(frame)
        .default_pos(egui::pos2(
            (screen.right() - HISTORY_DIALOG_SIZE.x - 24.0).max(screen.left()),
            screen.top() + 72.0,
        ))
        .show(ctx, |ui| {
            widget_ids::feature_scope(ui, "text_history_dialog", |ui| {
                settings::apply_dialog_typography(ui);
                callout::apply_spacing(ui);
                ui.visuals_mut().panel_fill = frame.fill;
                ui.visuals_mut().window_fill = frame.fill;
                if dismissible {
                    ui.label(
                        egui::RichText::new("History")
                            .size(18.0)
                            .color(callout::text(ui)),
                    );
                } else {
                    close = history_dialog_header(
                        ui,
                        "text_history.header",
                        "Close history",
                        "History",
                        18.0,
                    );
                }
                add_contents(ui, app);
            });
        });
    if let Some(response) = response {
        ctx.data_mut(|d| d.insert_temp(super::rect_key(), response.response.rect));
        callout::mark_scroll_blocker_if_hovered(ctx, &response.response);
        // Ignore the opening click and interactions with the panel's menus.
        close |= dismissible
            && was_visible
            && !popup_open
            && !egui::Popup::is_any_open(ctx)
            && !app.state.dialogs.any_modal_open()
            && accessors::pending_action(app).is_none()
            && response.response.clicked_elsewhere();
    }
    close
}
