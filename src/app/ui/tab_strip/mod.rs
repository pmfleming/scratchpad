pub mod actions;
mod entries;
pub mod layout;
mod outcome;
pub mod tab_cell;

use crate::app::app_state::ScratchpadApp;
use crate::app::services::settings_store::TabListPosition;
use crate::app::theme::*;
use eframe::egui;
use std::collections::HashSet;
use std::time::Instant;

pub(crate) use actions::{show_caption_controls, show_primary_actions};
use entries::{show_drag_region, show_tab_region, show_vertical_tab_region};
pub(crate) use layout::HeaderLayout;
use layout::{
    AUTO_HIDE_PEEK_SIZE, auto_hide_panel_extent, horizontal_bar_visible,
    show_horizontal_edge_tab_list, vertical_panel_visible, vertical_tab_list_frame,
    vertical_tab_panel,
};
use outcome::apply_tab_outcome;
pub(crate) use tab_cell::{TabInteraction, render_tab_cell, render_tab_cell_sized};

#[derive(Default)]
pub(crate) struct TabStripOutcome {
    pub(crate) activated_tab: Option<usize>,
    pub(crate) activate_settings: bool,
    pub(crate) close_requested_tab: Option<usize>,
    pub(crate) close_settings: bool,
    pub(crate) promote_all_files_tab: Option<usize>,
    pub(crate) reordered_tabs: Option<(usize, usize)>,
    pub(crate) combined_tabs: Option<(usize, usize)>,
    pub(crate) consumed_scroll_request: bool,
}

pub(crate) fn show_header(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    show_horizontal_tab_list(ui, app, TabListPosition::Top, "header");
}

fn show_header_drag_region(
    ctx: &egui::Context,
    ui: &mut egui::Ui,
    layout: &HeaderLayout,
) -> TabStripOutcome {
    ui.allocate_ui_with_layout(
        egui::vec2(layout.tab_area_width, TAB_HEIGHT),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| show_drag_region(ctx, ui, layout.tab_area_width),
    );
    TabStripOutcome::default()
}

pub(crate) fn show_vertical_tab_list(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    match app.tab_list_position() {
        TabListPosition::Left => show_vertical_tab_panel(ui, app, TabListPosition::Left),
        TabListPosition::Right => show_vertical_tab_panel(ui, app, TabListPosition::Right),
        TabListPosition::Top | TabListPosition::Bottom => {}
    }
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

    finalize_vertical_tab_panel(ui, app, side, now, panel_visible, &panel_response.response);
}

fn vertical_tab_panel_size_range(panel_visible: bool) -> std::ops::RangeInclusive<f32> {
    if panel_visible {
        ScratchpadApp::VERTICAL_TAB_LIST_MIN_WIDTH..=ScratchpadApp::VERTICAL_TAB_LIST_MAX_WIDTH
    } else {
        AUTO_HIDE_PEEK_SIZE..=AUTO_HIDE_PEEK_SIZE
    }
}

fn finalize_vertical_tab_panel(
    ui: &egui::Ui,
    app: &mut ScratchpadApp,
    side: TabListPosition,
    now: Instant,
    panel_visible: bool,
    response: &egui::Response,
) {
    if !panel_visible {
        app.close_tab_list();
        return;
    }

    let _ = (ui, side, now);
    app.set_tab_list_width_from_layout(response.rect.width());
}

pub(crate) fn show_bottom_tab_list(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    show_horizontal_tab_list(ui, app, TabListPosition::Bottom, "bottom_tab_list");
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
    show_horizontal_edge_tab_list(
        ui,
        position,
        panel_id,
        true,
        bar_visible,
        |ui| {
            let outcome = show_horizontal_tab_bar(&ctx, ui, app, true);
            apply_tab_outcome(app, outcome);
        },
    );
}

fn show_horizontal_tab_bar(
    ctx: &egui::Context,
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    include_tabs: bool,
) -> TabStripOutcome {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        show_primary_actions(ui, app);

        ui.add_space(8.0);
        let layout = HeaderLayout::measure(app, ui.available_width(), 4.0, include_tabs);
        let outcome = if include_tabs {
            show_tab_region(ctx, ui, app, &layout)
        } else {
            show_header_drag_region(ctx, ui, &layout)
        };

        ui.add_space(8.0);
        show_caption_controls(ctx, ui, app, &layout);
        outcome
    })
    .inner
}

pub(crate) fn maybe_auto_scroll_tab_strip(
    ui: &mut egui::Ui,
    app: &ScratchpadApp,
    layout: &HeaderLayout,
    scroll_area_id: egui::Id,
    viewport_rect: egui::Rect,
) {
    if let Some(scroll_state) = egui::scroll_area::State::load(ui.ctx(), scroll_area_id) {
        crate::app::ui::tab_drag::auto_scroll_tab_strip(
            ui.ctx(),
            scroll_area_id,
            viewport_rect,
            app.estimated_tab_strip_width(layout.spacing),
            &scroll_state,
        );
    }
}

pub(crate) fn maybe_scroll_to_active_tab(
    ui: &mut egui::Ui,
    index: usize,
    active_tab_index: usize,
    pending_scroll_to_active: bool,
    rect: egui::Rect,
    outcome: &mut TabStripOutcome,
) {
    if index == active_tab_index && pending_scroll_to_active {
        ui.scroll_to_rect(rect, Some(egui::Align::Center));
        outcome.consumed_scroll_request = true;
    }
}

pub(crate) fn record_visible_tab(
    index: usize,
    rect: egui::Rect,
    viewport_rect: egui::Rect,
    visible_tab_indices: &mut HashSet<usize>,
) {
    if viewport_rect.intersects(rect) {
        visible_tab_indices.insert(index);
    }
}

pub(crate) fn apply_tab_interaction(outcome: &mut TabStripOutcome, interaction: TabInteraction) {
    match interaction {
        TabInteraction::None => {}
        TabInteraction::Activate(index) => outcome.activated_tab = Some(index),
        TabInteraction::RequestClose(index) => outcome.close_requested_tab = Some(index),
        TabInteraction::PromoteAllFiles(index) => outcome.promote_all_files_tab = Some(index),
    }
}


#[cfg(test)]
mod tests {
    use super::TabStripOutcome;
    use crate::app::ui::tab_strip::entries::apply_settings_tab_interaction;

    #[test]
    fn settings_tab_close_gesture_closes_settings_surface() {
        let mut outcome = TabStripOutcome::default();

        apply_settings_tab_interaction(&mut outcome, true, true, false);

        assert!(outcome.close_settings);
        assert!(!outcome.activate_settings);
        assert!(outcome.close_requested_tab.is_none());
    }

    #[test]
    fn clicking_settings_tab_activates_settings_surface() {
        let mut outcome = TabStripOutcome::default();

        apply_settings_tab_interaction(&mut outcome, false, false, true);

        assert!(outcome.activate_settings);
        assert!(!outcome.close_settings);
    }
}
