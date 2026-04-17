pub mod actions;
mod entries;
pub mod layout;
mod outcome;
mod panels;
pub mod tab_cell;
mod top_drag;

use crate::app::app_state::ScratchpadApp;
use eframe::egui;
use std::collections::HashSet;

pub(crate) use layout::HeaderLayout;
pub(crate) use tab_cell::{TabInteraction, render_tab_cell_sized};

#[derive(Default)]
pub(crate) struct TabStripOutcome {
    pub(crate) activated_tab: Option<usize>,
    pub(crate) rename_requested_tab: Option<usize>,
    pub(crate) activate_settings: bool,
    pub(crate) close_requested_tab: Option<usize>,
    pub(crate) close_settings: bool,
    pub(crate) promote_all_files_tab: Option<usize>,
    pub(crate) reordered_tabs: Option<(usize, usize)>,
    pub(crate) reordered_tab_group: Option<(Vec<usize>, usize)>,
    pub(crate) combined_tabs: Option<(usize, usize)>,
    pub(crate) combined_tab_group: Option<(Vec<usize>, usize)>,
    pub(crate) consumed_scroll_request: bool,
}

pub(crate) fn show_header(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    panels::show_header(ui, app);
}

pub(crate) fn show_top_drag_bar(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    top_drag::show_top_drag_bar(ui, app);
}

pub(crate) fn show_vertical_tab_list(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    panels::show_vertical_tab_list(ui, app);
}

pub(crate) fn show_bottom_tab_list(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    panels::show_bottom_tab_list(ui, app);
}

pub(crate) fn maybe_auto_scroll_tab_strip(
    ui: &mut egui::Ui,
    app: &ScratchpadApp,
    layout: &HeaderLayout,
    scroll_area_id: egui::Id,
    viewport_rect: egui::Rect,
) {
    if let Some(scroll_state) = egui::scroll_area::State::load(ui.ctx(), scroll_area_id) {
        crate::app::ui::tab_drag::auto_scroll_tab_list(
            ui.ctx(),
            scroll_area_id,
            viewport_rect,
            app.estimated_tab_strip_width(layout.spacing),
            &scroll_state,
            crate::app::ui::tab_drag::TabDropAxis::Horizontal,
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
        TabInteraction::BeginRename(index) => {
            outcome.activated_tab = Some(index);
            outcome.rename_requested_tab = Some(index);
        }
        TabInteraction::RequestClose(index) => outcome.close_requested_tab = Some(index),
        TabInteraction::PromoteAllFiles(index) => outcome.promote_all_files_tab = Some(index),
    }
}

#[cfg(test)]
mod tests {
    use super::TabStripOutcome;

    #[test]
    fn outcome_defaults_to_empty_actions() {
        let outcome = TabStripOutcome::default();

        assert!(outcome.activated_tab.is_none());
        assert!(outcome.rename_requested_tab.is_none());
        assert!(!outcome.activate_settings);
        assert!(outcome.close_requested_tab.is_none());
        assert!(!outcome.close_settings);
    }
}
