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

struct TabStripEntriesContext<'a> {
    app: &'a mut ScratchpadApp,
    duplicate_name_counts: &'a HashMap<String, usize>,
    viewport_rect: egui::Rect,
    visible_tab_indices: &'a mut HashSet<usize>,
    outcome: &'a mut TabStripOutcome,
    active_slot_index: usize,
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
            let mut context = TabStripEntriesContext {
                active_slot_index: app.active_tab_slot_index(),
                pending_scroll_to_active: app.tab_manager().pending_scroll_to_active,
                app,
                duplicate_name_counts,
                viewport_rect,
                visible_tab_indices,
                outcome,
            };
            render_tab_strip_entries(ui, layout, scroll_area_id, &mut context)
        },
    )
    .inner
}

fn configure_tab_strip_viewport(ui: &mut egui::Ui, visible_strip_width: f32) {
    ui.set_width(visible_strip_width);
    ui.set_min_width(visible_strip_width);
    ui.set_max_width(visible_strip_width);
}

fn render_tab_strip_entries(
    ui: &mut egui::Ui,
    layout: &HeaderLayout,
    scroll_area_id: egui::Id,
    context: &mut TabStripEntriesContext<'_>,
) -> Vec<TabRectEntry> {
    egui::ScrollArea::horizontal()
        .id_salt(scroll_area_id)
        .auto_shrink([false, false])
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.x = layout.spacing;
            ui.horizontal(|ui| collect_tab_entries(ui, context)).inner
        })
        .inner
}

fn collect_tab_entries(
    ui: &mut egui::Ui,
    context: &mut TabStripEntriesContext<'_>,
) -> Vec<TabRectEntry> {
    let cell_context = WorkspaceTabCellContext {
        showing_settings: context.app.showing_settings(),
        duplicate_name_counts: context.duplicate_name_counts,
        active_tab_index: context.active_slot_index,
        pending_scroll_to_active: context.pending_scroll_to_active,
    };

    let total_slots = context.app.total_tab_slots();
    let mut row_rects = Vec::with_capacity(total_slots);

    for slot_index in 0..total_slots {
        let cell_outcome = if context.app.tab_slot_is_settings(slot_index) {
            render_settings_tab_cell(
                ui,
                context.app,
                slot_index,
                context.active_slot_index,
                context.outcome,
            )
        } else {
            let workspace_index = context
                .app
                .workspace_index_for_slot(slot_index)
                .unwrap_or(slot_index);
            let tab = &context.app.tabs()[workspace_index];
            render_workspace_tab_cell(ui, slot_index, tab, &cell_context, context.outcome)
        };

        record_visible_tab(
            slot_index,
            cell_outcome.rect,
            context.viewport_rect,
            context.visible_tab_indices,
        );
        row_rects.push(TabRectEntry {
            index: slot_index,
            rect: cell_outcome.rect,
            combine_enabled: !context.app.tab_slot_is_settings(slot_index),
        });
    }

    row_rects
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
