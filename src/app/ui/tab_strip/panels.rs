use super::actions::{show_caption_controls, show_primary_actions};
use super::entries::{show_tab_region, show_vertical_tab_region};
use super::layout::{
    AUTO_HIDE_PEEK_SIZE, auto_hide_panel_extent, horizontal_bar_visible,
    show_horizontal_edge_tab_list, vertical_panel_visible, vertical_tab_list_frame,
    vertical_tab_panel,
};
use super::outcome::apply_tab_outcome;
use super::HeaderLayout;
use crate::app::app_state::ScratchpadApp;
use crate::app::services::settings_store::TabListPosition;
use eframe::egui;
use std::time::Instant;

pub(crate) fn show_header(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    show_horizontal_tab_list(ui, app, TabListPosition::Top, "header");
}

pub(crate) fn show_vertical_tab_list(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    if let Some(side) = vertical_tab_side(app.tab_list_position()) {
        show_vertical_tab_panel(ui, app, side);
    }
}

pub(crate) fn show_bottom_tab_list(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    show_horizontal_tab_list(ui, app, TabListPosition::Bottom, "bottom_tab_list");
}

fn show_vertical_tab_panel(ui: &mut egui::Ui, app: &mut ScratchpadApp, side: TabListPosition) {
    app.overflow_popup_open = false;
    let now = Instant::now();
    let panel_visible = vertical_panel_visible(ui, app, side, now);
    let panel_width = auto_hide_panel_extent(panel_visible, app.vertical_tab_list_width());

    let panel_response = vertical_tab_panel(side, panel_visible)
        .default_size(panel_width)
        .size_range(vertical_tab_panel_size_range(panel_visible))
        .resizable(panel_visible)
        .frame(vertical_tab_list_frame(ui))
        .show_inside(ui, |ui| {
            if !panel_visible {
                return;
            }
            let outcome = show_vertical_tab_region(ui, app);
            apply_tab_outcome(app, outcome);
        });

    finalize_vertical_tab_panel(app, panel_visible, &panel_response.response);
}

fn vertical_tab_panel_size_range(panel_visible: bool) -> std::ops::RangeInclusive<f32> {
    if panel_visible {
        ScratchpadApp::VERTICAL_TAB_LIST_MIN_WIDTH..=ScratchpadApp::VERTICAL_TAB_LIST_MAX_WIDTH
    } else {
        AUTO_HIDE_PEEK_SIZE..=AUTO_HIDE_PEEK_SIZE
    }
}

fn finalize_vertical_tab_panel(
    app: &mut ScratchpadApp,
    panel_visible: bool,
    response: &egui::Response,
) {
    if !panel_visible {
        app.close_tab_list();
        return;
    }

    app.set_tab_list_width_from_layout(response.rect.width());
}

fn show_horizontal_tab_list(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    position: TabListPosition,
    panel_id: &'static str,
) {
    if app.tab_list_position() != position {
        return;
    }

    let ctx = ui.ctx().clone();
    let bar_visible = horizontal_bar_visible(ui, app, position, Instant::now());
    show_horizontal_edge_tab_list(ui, position, panel_id, true, bar_visible, |ui| {
        let outcome = show_horizontal_tab_bar(&ctx, ui, app);
        apply_tab_outcome(app, outcome);
    });
}

fn show_horizontal_tab_bar(
    ctx: &egui::Context,
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
) -> super::TabStripOutcome {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        show_primary_actions(ui, app);

        ui.add_space(8.0);
        let layout = HeaderLayout::measure(app, ui.available_width(), 4.0, true);
        let outcome = show_tab_region(ctx, ui, app, &layout);

        ui.add_space(8.0);
        show_caption_controls(ctx, ui, app, &layout);
        outcome
    })
    .inner
}

fn vertical_tab_side(position: TabListPosition) -> Option<TabListPosition> {
    match position {
        TabListPosition::Left | TabListPosition::Right => Some(position),
        TabListPosition::Top | TabListPosition::Bottom => None,
    }
}