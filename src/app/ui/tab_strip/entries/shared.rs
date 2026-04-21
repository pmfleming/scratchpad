use crate::app::app_state::ScratchpadApp;
use crate::app::chrome::tab_button_sized;
use crate::app::domain::WorkspaceTab;
use crate::app::ui::tab_drag::{self, TabRectEntry};
use crate::app::ui::tab_strip::context_menu::attach_tab_context_menu;
use crate::app::ui::tab_strip::tab_cell::{TabCellOutcome, TabCellProps};
use crate::app::ui::tab_strip::{
    TabStripOutcome, apply_tab_interaction, maybe_scroll_to_active_tab, render_tab_cell_sized,
};
use eframe::egui;
use std::collections::HashMap;

pub(super) struct SlotCellContext<'a> {
    duplicate_name_counts: &'a HashMap<String, usize>,
    active_slot_index: usize,
    pending_scroll_to_active: bool,
    showing_settings: bool,
    width: f32,
}

pub(super) fn slot_cell_context<'a>(
    app: &ScratchpadApp,
    duplicate_name_counts: &'a HashMap<String, usize>,
    width: f32,
) -> SlotCellContext<'a> {
    SlotCellContext {
        active_slot_index: app.active_tab_slot_index(),
        duplicate_name_counts,
        pending_scroll_to_active: app.tab_manager().pending_scroll_to_active,
        showing_settings: app.showing_settings(),
        width,
    }
}

pub(super) fn collect_slot_entries(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    context: &SlotCellContext<'_>,
    outcome: &mut TabStripOutcome,
    mut on_rect: impl FnMut(usize, egui::Rect),
) -> Vec<TabRectEntry> {
    let total_slots = app.total_tab_slots();
    let mut entries = Vec::with_capacity(total_slots);

    for slot_index in 0..total_slots {
        let cell_outcome = render_tab_slot_cell(ui, app, slot_index, context, outcome);
        on_rect(slot_index, cell_outcome.rect);
        entries.push(tab_rect_entry(
            slot_index,
            cell_outcome.rect,
            !app.tab_slot_is_settings(slot_index),
        ));
    }

    entries
}

fn render_tab_slot_cell(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    slot_index: usize,
    context: &SlotCellContext<'_>,
    outcome: &mut TabStripOutcome,
) -> TabCellOutcome {
    if let Some(tab) = workspace_tab_for_slot(app, slot_index) {
        let has_duplicate = context
            .duplicate_name_counts
            .get(&tab.buffer.name)
            .copied()
            .unwrap_or(0)
            > 1;
        let display_name = tab.full_display_name(has_duplicate);
        let tooltip = display_name.clone();
        let can_promote_all_files = tab.can_promote_all_files();
        let is_active = !context.showing_settings && context.active_slot_index == slot_index;
        let is_selected = app.tab_slot_selected(slot_index);
        let cell_outcome = render_tab_cell_sized(
            ui,
            app,
            slot_index,
            TabCellProps {
                display_name: &display_name,
                tooltip: Some(tooltip),
                can_promote_all_files,
                is_active,
                is_selected,
                pending_scroll_to_active: context.pending_scroll_to_active,
                width: context.width,
            },
        );
        apply_tab_interaction(outcome, cell_outcome.interaction);
        return finish_tab_slot_cell(ui, slot_index, context, cell_outcome, outcome);
    }

    let is_active = context.showing_settings && context.active_slot_index == slot_index;
    let is_selected = app.tab_slot_selected(slot_index);
    let (tab_response, close_response, _) =
        tab_button_sized(ui, "Settings", is_active, is_selected, context.width);
    let dragged_slots = app.dragged_tab_slots(slot_index);
    tab_drag::begin_tab_drag_if_needed(
        ui,
        slot_index,
        &dragged_slots,
        &tab_response,
        &close_response,
    );
    let tab_clicked = tab_response.clicked()
        && handle_settings_tab_click(app, slot_index, ui.input(|input| input.modifiers));
    apply_settings_tab_interaction(
        outcome,
        app.showing_settings(),
        close_response.clicked(),
        tab_clicked,
    );
    attach_tab_context_menu(&tab_response, app, slot_index);
    maybe_scroll_to_active_tab(
        ui,
        slot_index,
        context.active_slot_index,
        context.pending_scroll_to_active,
        tab_response.rect,
        outcome,
    );
    TabCellOutcome {
        rect: tab_response.rect,
        interaction: crate::app::ui::tab_strip::TabInteraction::None,
    }
}

pub(super) fn handle_settings_tab_click(
    app: &mut ScratchpadApp,
    slot_index: usize,
    modifiers: egui::Modifiers,
) -> bool {
    if modifiers.shift {
        app.select_tab_slot_range(slot_index);
        true
    } else if modifiers.command || modifiers.ctrl {
        app.toggle_tab_slot_selection(slot_index);
        false
    } else {
        app.select_only_tab_slot(slot_index);
        true
    }
}

fn finish_tab_slot_cell(
    ui: &mut egui::Ui,
    slot_index: usize,
    context: &SlotCellContext<'_>,
    cell_outcome: TabCellOutcome,
    outcome: &mut TabStripOutcome,
) -> TabCellOutcome {
    maybe_scroll_to_active_tab(
        ui,
        slot_index,
        context.active_slot_index,
        context.pending_scroll_to_active,
        cell_outcome.rect,
        outcome,
    );
    cell_outcome
}

fn workspace_tab_for_slot(app: &ScratchpadApp, slot_index: usize) -> Option<&WorkspaceTab> {
    let workspace_index = app.workspace_index_for_slot(slot_index)?;
    app.tabs().get(workspace_index)
}

fn tab_rect_entry(index: usize, rect: egui::Rect, combine_enabled: bool) -> TabRectEntry {
    TabRectEntry {
        index,
        rect,
        combine_enabled,
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
