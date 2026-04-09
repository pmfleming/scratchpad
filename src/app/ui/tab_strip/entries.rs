use super::{
    HeaderLayout, TabStripOutcome, apply_tab_interaction, maybe_scroll_to_active_tab,
    record_visible_tab,
};
use crate::app::app_state::ScratchpadApp;
use crate::app::chrome::tab_button_sized;
use crate::app::domain::WorkspaceTab;
use crate::app::theme::{TAB_BUTTON_WIDTH, TAB_HEIGHT};
use crate::app::ui::tab_drag::{self, TabRectEntry};
use crate::app::ui::tab_strip::render_tab_cell;
use eframe::egui;
use std::collections::{HashMap, HashSet};

struct WorkspaceTabCellContext<'a> {
    showing_settings: bool,
    duplicate_name_counts: &'a HashMap<String, usize>,
    active_tab_index: usize,
    pending_scroll_to_active: bool,
}

pub(crate) fn allocate_tab_strip_entries(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    layout: &HeaderLayout,
    scroll_area_id: egui::Id,
    duplicate_name_counts: &HashMap<String, usize>,
    visible_tab_indices: &mut HashSet<usize>,
    outcome: &mut TabStripOutcome,
) -> Vec<TabRectEntry> {
    ui.allocate_ui_with_layout(
        egui::vec2(layout.visible_strip_width, TAB_HEIGHT),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            configure_tab_strip_viewport(ui, layout.visible_strip_width);
            let viewport_rect = ui.max_rect();
            super::maybe_auto_scroll_tab_strip(ui, app, layout, scroll_area_id, viewport_rect);
            render_tab_strip_entries(
                ui,
                app,
                layout,
                scroll_area_id,
                viewport_rect,
                duplicate_name_counts,
                visible_tab_indices,
                outcome,
            )
        },
    )
    .inner
}

fn configure_tab_strip_viewport(ui: &mut egui::Ui, visible_strip_width: f32) {
    ui.set_width(visible_strip_width);
    ui.set_min_width(visible_strip_width);
    ui.set_max_width(visible_strip_width);
}

#[allow(clippy::too_many_arguments)]
fn render_tab_strip_entries(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    layout: &HeaderLayout,
    scroll_area_id: egui::Id,
    viewport_rect: egui::Rect,
    duplicate_name_counts: &HashMap<String, usize>,
    visible_tab_indices: &mut HashSet<usize>,
    outcome: &mut TabStripOutcome,
) -> Vec<TabRectEntry> {
    egui::ScrollArea::horizontal()
        .id_salt(scroll_area_id)
        .auto_shrink([false, false])
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.x = layout.spacing;
            ui.horizontal(|ui| {
                collect_tab_strip_entries(
                    ui,
                    app,
                    duplicate_name_counts,
                    viewport_rect,
                    visible_tab_indices,
                    outcome,
                )
            })
            .inner
        })
        .inner
}

fn collect_tab_strip_entries(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    duplicate_name_counts: &HashMap<String, usize>,
    viewport_rect: egui::Rect,
    visible_tab_indices: &mut HashSet<usize>,
    outcome: &mut TabStripOutcome,
) -> Vec<TabRectEntry> {
    let active_slot_index = app.active_tab_slot_index();
    let pending_scroll_to_active = app.tab_manager().pending_scroll_to_active;
    let total_slots = app.total_tab_slots();
    let mut row_rects = Vec::with_capacity(total_slots);

    collect_tab_entries(
        ui,
        app,
        duplicate_name_counts,
        viewport_rect,
        visible_tab_indices,
        outcome,
        active_slot_index,
        pending_scroll_to_active,
        &mut row_rects,
    );

    row_rects
}

#[allow(clippy::too_many_arguments)]
fn collect_tab_entries(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    duplicate_name_counts: &HashMap<String, usize>,
    viewport_rect: egui::Rect,
    visible_tab_indices: &mut HashSet<usize>,
    outcome: &mut TabStripOutcome,
    active_slot_index: usize,
    pending_scroll_to_active: bool,
    row_rects: &mut Vec<TabRectEntry>,
) {
    let cell_context = WorkspaceTabCellContext {
        showing_settings: app.showing_settings(),
        duplicate_name_counts,
        active_tab_index: active_slot_index,
        pending_scroll_to_active,
    };

    for slot_index in 0..app.total_tab_slots() {
        let cell_outcome = if app.tab_slot_is_settings(slot_index) {
            render_settings_tab_cell(ui, app, slot_index, active_slot_index, outcome)
        } else {
            let workspace_index = app
                .workspace_index_for_slot(slot_index)
                .unwrap_or(slot_index);
            let tab = &app.tabs()[workspace_index];
            render_workspace_tab_cell(ui, slot_index, tab, &cell_context, outcome)
        };

        record_visible_tab(
            slot_index,
            cell_outcome.rect,
            viewport_rect,
            visible_tab_indices,
        );
        row_rects.push(TabRectEntry {
            index: slot_index,
            rect: cell_outcome.rect,
            combine_enabled: !app.tab_slot_is_settings(slot_index),
        });
    }
}

fn render_workspace_tab_cell(
    ui: &mut egui::Ui,
    slot_index: usize,
    tab: &WorkspaceTab,
    context: &WorkspaceTabCellContext<'_>,
    outcome: &mut TabStripOutcome,
) -> super::tab_cell::TabCellOutcome {
    let cell_outcome = render_tab_cell(
        ui,
        slot_index,
        tab,
        !context.showing_settings && context.active_tab_index == slot_index,
        context.pending_scroll_to_active,
        context.duplicate_name_counts,
    );
    apply_tab_interaction(outcome, cell_outcome.interaction);
    maybe_scroll_to_active_tab(
        ui,
        slot_index,
        context.active_tab_index,
        context.pending_scroll_to_active,
        cell_outcome.rect,
        outcome,
    );
    cell_outcome
}

fn render_settings_tab_cell(
    ui: &mut egui::Ui,
    app: &ScratchpadApp,
    slot_index: usize,
    active_slot_index: usize,
    outcome: &mut TabStripOutcome,
) -> super::tab_cell::TabCellOutcome {
    let (tab_response, close_response, _) =
        tab_button_sized(ui, "Settings", app.showing_settings(), TAB_BUTTON_WIDTH);
    tab_drag::begin_tab_drag_if_needed(ui, slot_index, &tab_response, &close_response);
    apply_settings_tab_interaction(
        outcome,
        app.showing_settings(),
        close_response.clicked(),
        tab_response.clicked(),
    );
    maybe_scroll_to_active_tab(
        ui,
        slot_index,
        active_slot_index,
        app.tab_manager().pending_scroll_to_active,
        tab_response.rect,
        outcome,
    );
    super::tab_cell::TabCellOutcome {
        rect: tab_response.rect,
        interaction: super::TabInteraction::None,
    }
}

pub(crate) fn apply_settings_tab_interaction(
    outcome: &mut TabStripOutcome,
    showing_settings: bool,
    close_clicked: bool,
    tab_clicked: bool,
) {
    if close_clicked && showing_settings {
        outcome.close_settings = true;
    } else if tab_clicked {
        outcome.activate_settings = true;
    }
}
