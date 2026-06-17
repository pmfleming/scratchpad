use crate::app::app_state::{ScratchpadApp, workspace::display_tabs};
use crate::app::chrome::{TabButtonOptions, tab_button_with_actions, tab_label_font_id};
use crate::app::domain::{TabAttentionState, WorkspaceTab};
use crate::app::theme::TAB_HEIGHT;
use crate::app::ui::tab_drag::{TabDropAxis, TabRectEntry};
use crate::app::ui::tab_strip::context_menu::{
    attach_tab_context_menu, attach_tab_list_context_menu,
};
use crate::app::ui::tab_strip::tab_cell::{TabCellOutcome, TabCellProps};
use crate::app::ui::tab_strip::{
    TabStripOutcome, apply_tab_interaction, maybe_scroll_to_active_tab, render_tab_cell_sized,
};
use crate::app::ui::widget_ids;
use eframe::egui;
use std::collections::HashMap;

pub(super) struct SlotCellContext<'a> {
    duplicate_name_counts: &'a HashMap<String, usize>,
    active_slot_index: usize,
    pending_scroll_to_active: bool,
    showing_settings: bool,
    width: f32,
    spacing: f32,
    axis: TabDropAxis,
    label_font_id: egui::FontId,
}

pub(super) fn slot_cell_context<'a>(
    app: &ScratchpadApp,
    duplicate_name_counts: &'a HashMap<String, usize>,
    width: f32,
    spacing: f32,
    axis: TabDropAxis,
) -> SlotCellContext<'a> {
    SlotCellContext {
        active_slot_index: crate::app::app_state::workspace::display_tabs::active_tab_slot_index(
            app,
        ),
        duplicate_name_counts,
        pending_scroll_to_active: app.tab_manager.pending_scroll_to_active,
        showing_settings: crate::app::app_state::settings_state::showing_settings(app),
        width,
        spacing,
        axis,
        label_font_id: tab_label_font_id(app.state.app_settings.font_size()),
    }
}

pub(super) fn collect_slot_entries(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    context: &SlotCellContext<'_>,
    outcome: &mut TabStripOutcome,
    mut on_rect: impl FnMut(usize, egui::Rect),
) -> Vec<TabRectEntry> {
    let total_slots = crate::app::app_state::workspace::display_tabs::total_tab_slots(app);
    let ranges = visible_slot_ranges(ui, total_slots, context);
    let visible_slots = ranges
        .iter()
        .map(|range| range.end.saturating_sub(range.start))
        .sum::<usize>();
    let mut entries = Vec::with_capacity(visible_slots);
    let mut next_slot = 0usize;

    for range in ranges {
        add_virtual_slot_space(ui, range.start.saturating_sub(next_slot), context);
        for slot_index in range.clone() {
            let cell_outcome = render_tab_slot_cell(ui, app, slot_index, context, outcome);
            on_rect(slot_index, cell_outcome.rect);
            entries.push(tab_rect_entry(
                slot_index,
                cell_outcome.rect,
                !crate::app::app_state::workspace::display_tabs::tab_slot_is_settings(
                    app, slot_index,
                ),
            ));
        }
        next_slot = range.end;
    }
    add_virtual_slot_space(ui, total_slots.saturating_sub(next_slot), context);

    entries
}

fn visible_slot_ranges(
    ui: &egui::Ui,
    total_slots: usize,
    context: &SlotCellContext<'_>,
) -> Vec<std::ops::Range<usize>> {
    if total_slots == 0 {
        return Vec::new();
    }
    let slot_extent = slot_advance(context);
    let content_origin = match context.axis {
        TabDropAxis::Horizontal => ui.cursor().min.x,
        TabDropAxis::Vertical => ui.cursor().min.y,
    };
    let clip = ui.clip_rect();
    let (clip_min, clip_max) = match context.axis {
        TabDropAxis::Horizontal => (clip.min.x, clip.max.x),
        TabDropAxis::Vertical => (clip.min.y, clip.max.y),
    };
    let first = ((clip_min - content_origin) / slot_extent).floor().max(0.0) as usize;
    let last = ((clip_max - content_origin) / slot_extent).ceil().max(0.0) as usize;
    let mut ranges = Vec::with_capacity(2);
    ranges.push(first.saturating_sub(2)..last.saturating_add(2).min(total_slots));
    if context.pending_scroll_to_active && context.active_slot_index < total_slots {
        ranges.push(context.active_slot_index..context.active_slot_index + 1);
    }
    merge_slot_ranges(ranges)
}

fn merge_slot_ranges(mut ranges: Vec<std::ops::Range<usize>>) -> Vec<std::ops::Range<usize>> {
    ranges.retain(|range| range.start < range.end);
    ranges.sort_by_key(|range| range.start);
    let mut merged: Vec<std::ops::Range<usize>> = Vec::new();
    for range in ranges {
        if let Some(last) = merged.last_mut()
            && range.start <= last.end
        {
            last.end = last.end.max(range.end);
            continue;
        }
        merged.push(range);
    }
    merged
}

fn add_virtual_slot_space(ui: &mut egui::Ui, slot_count: usize, context: &SlotCellContext<'_>) {
    if slot_count == 0 {
        return;
    }
    let extent = slot_count as f32 * slot_advance(context);
    ui.add_space(extent.max(0.0));
}

fn slot_advance(context: &SlotCellContext<'_>) -> f32 {
    match context.axis {
        TabDropAxis::Horizontal => context.width + context.spacing,
        TabDropAxis::Vertical => TAB_HEIGHT + context.spacing,
    }
    .max(1.0)
}

pub(super) fn attach_tab_list_background_context_menu(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    app: &mut ScratchpadApp,
    entries: &[TabRectEntry],
    id: &'static str,
) {
    if pointer_over_tab_entry(ui, entries) {
        return;
    }
    let response = widget_ids::interact(
        ui,
        rect,
        widget_ids::local(ui, id),
        egui::Sense::click(),
        id,
    );
    attach_tab_list_context_menu(&response, app);
}

fn pointer_over_tab_entry(ui: &egui::Ui, entries: &[TabRectEntry]) -> bool {
    ui.input(|input| input.pointer.interact_pos())
        .is_some_and(|pos| entries.iter().any(|entry| entry.rect.contains(pos)))
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
            .get(&tab.buffers.buffer.name)
            .copied()
            .unwrap_or(0)
            > 1;
        let display_name = crate::app::domain::tab::summary::full_display_name(tab, has_duplicate);
        let attention_state = crate::app::domain::tab::summary::attention_state(tab);
        let can_promote_all_files = crate::app::domain::tab::summary::can_promote_all_files(tab);
        let is_active = !context.showing_settings && context.active_slot_index == slot_index;
        let is_selected =
            crate::app::app_state::workspace::display_tabs::tab_slot_selected(app, slot_index);
        let cell_outcome = render_tab_cell_sized(
            ui,
            app,
            slot_index,
            TabCellProps {
                display_name: &display_name,
                tooltip: Some(workspace_tab_tooltip(&display_name, attention_state)),
                can_promote_all_files,
                attention_state,
                is_active,
                is_selected,
                pending_scroll_to_active: context.pending_scroll_to_active,
                width: context.width,
                label_font_id: context.label_font_id.clone(),
            },
        );
        apply_tab_interaction(outcome, cell_outcome.interaction);
        return finish_tab_slot_cell(ui, slot_index, context, cell_outcome, outcome);
    }

    let is_active = context.showing_settings && context.active_slot_index == slot_index;
    let is_selected =
        crate::app::app_state::workspace::display_tabs::tab_slot_selected(app, slot_index);
    let (tab_response, _, close_response, _) = tab_button_with_actions(
        ui,
        ("tab_strip.slot", slot_index),
        "Settings",
        is_active,
        is_selected,
        TabButtonOptions::new(context.width).with_label_font_id(context.label_font_id.clone()),
    );
    let tab_clicked = tab_response.clicked()
        && handle_settings_tab_click(app, slot_index, ui.input(|input| input.modifiers));
    let tab_context_click = attach_tab_context_menu(&tab_response, app, slot_index);
    apply_settings_tab_interaction(
        outcome,
        crate::app::app_state::settings_state::showing_settings(app),
        close_response.clicked(),
        tab_clicked || tab_context_click.secondary_clicked(),
    );
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
        display_tabs::select_tab_slot_range(app, slot_index);
        true
    } else if modifiers.command || modifiers.ctrl {
        display_tabs::toggle_tab_slot_selection(app, slot_index);
        false
    } else {
        display_tabs::select_only_tab_slot(app, slot_index);
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
    let workspace_index =
        crate::app::app_state::workspace::display_tabs::workspace_index_for_slot(app, slot_index)?;
    app.tab_manager.tabs.as_slice().get(workspace_index)
}

fn workspace_tab_tooltip(
    display_name: &str,
    _attention_state: Option<TabAttentionState>,
) -> String {
    display_name.to_owned()
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
    _showing_settings: bool,
    close_clicked: bool,
    tab_clicked: bool,
) {
    if close_clicked {
        outcome.close_settings = true;
    } else if tab_clicked {
        outcome.activate_settings = true;
    }
}

#[cfg(test)]
mod tests {
    use super::{SlotCellContext, slot_advance, workspace_tab_tooltip};
    use crate::app::domain::TabAttentionState;
    use crate::app::theme::TAB_HEIGHT;
    use crate::app::ui::tab_drag::TabDropAxis;
    use eframe::egui;
    use std::collections::HashMap;

    #[test]
    fn tab_attention_dot_does_not_change_the_tab_tooltip() {
        let display_name = "notes.txt (C:\\notes)";

        assert_eq!(
            workspace_tab_tooltip(display_name, Some(TabAttentionState::DiskProblem)),
            display_name
        );
    }

    #[test]
    fn vertical_virtual_tabs_advance_by_height_not_panel_width() {
        let duplicate_name_counts = HashMap::new();
        let context = SlotCellContext {
            duplicate_name_counts: &duplicate_name_counts,
            active_slot_index: 0,
            pending_scroll_to_active: false,
            showing_settings: false,
            width: 320.0,
            spacing: 4.0,
            axis: TabDropAxis::Vertical,
            label_font_id: egui::FontId::proportional(14.0),
        };

        assert_eq!(slot_advance(&context), TAB_HEIGHT + 4.0);
    }

    #[test]
    fn horizontal_virtual_tabs_advance_by_width() {
        let duplicate_name_counts = HashMap::new();
        let context = SlotCellContext {
            duplicate_name_counts: &duplicate_name_counts,
            active_slot_index: 0,
            pending_scroll_to_active: false,
            showing_settings: false,
            width: 160.0,
            spacing: 4.0,
            axis: TabDropAxis::Horizontal,
            label_font_id: egui::FontId::proportional(14.0),
        };

        assert_eq!(slot_advance(&context), 164.0);
    }
}
