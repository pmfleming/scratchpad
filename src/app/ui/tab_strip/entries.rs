use super::{
    HeaderLayout, TabStripOutcome, apply_tab_interaction, maybe_scroll_to_active_tab,
    record_visible_tab,
};
use crate::app::app_state::ScratchpadApp;
use crate::app::chrome::tab_button_sized;
use crate::app::commands::AppCommand;
use crate::app::domain::WorkspaceTab;
use crate::app::theme::{
    BUTTON_SIZE, TAB_BUTTON_WIDTH, TAB_HEIGHT, action_bg, action_hover_bg, border,
};
use crate::app::ui::tab_drag::{self, TabDropAxis, TabDropZone, TabRectEntry};
use crate::app::ui::tab_overflow;
use crate::app::ui::tab_strip::render_tab_cell_sized;
use crate::app::ui::tab_strip::tab_cell::TabCellProps;
use eframe::egui::{self, Sense, Stroke};
use std::collections::{HashMap, HashSet};

struct TabStripEntriesContext<'a> {
    app: &'a mut ScratchpadApp,
    duplicate_name_counts: &'a HashMap<String, usize>,
    viewport_rect: egui::Rect,
    visible_tab_indices: &'a mut HashSet<usize>,
    outcome: &'a mut TabStripOutcome,
}

struct SlotCellContext<'a> {
    duplicate_name_counts: &'a HashMap<String, usize>,
    active_slot_index: usize,
    pending_scroll_to_active: bool,
    showing_settings: bool,
    width: f32,
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

pub(crate) fn duplicate_name_counts(tabs: &[WorkspaceTab]) -> HashMap<String, usize> {
    let mut counts = HashMap::with_capacity(tabs.len());
    for tab in tabs {
        *counts.entry(tab.buffer.name.clone()).or_insert(0) += 1;
    }
    counts
}

pub(crate) fn show_vertical_tab_region(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
) -> TabStripOutcome {
    tab_drag::sync_drag_state(ui);
    let duplicate_name_counts = duplicate_name_counts(app.tabs());
    let mut outcome = TabStripOutcome::default();

    super::actions::show_vertical_primary_actions(ui, app);
    ui.add_space(8.0);
    let drop_zones =
        show_vertical_tab_entries_above_new_tab(ui, app, &duplicate_name_counts, &mut outcome);
    apply_tab_drag_feedback(ui, app, &drop_zones, &mut outcome);
    outcome
}

pub(crate) fn show_tab_region(
    ctx: &egui::Context,
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    layout: &HeaderLayout,
) -> TabStripOutcome {
    let duplicate_name_counts = duplicate_name_counts(app.tabs());
    let mut visible_tab_indices = HashSet::new();
    let mut outcome = TabStripOutcome::default();

    ui.allocate_ui_with_layout(
        egui::vec2(layout.tab_area_width, TAB_HEIGHT),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            tab_drag::sync_drag_state(ui);
            ui.spacing_mut().item_spacing.x = 0.0;
            let drop_zones = collect_tab_drop_zones(
                ctx,
                ui,
                app,
                layout,
                &duplicate_name_counts,
                &mut visible_tab_indices,
                &mut outcome,
            );
            apply_tab_drag_feedback(ui, app, &drop_zones, &mut outcome);
            render_new_tab_action(ui, app, layout.spacing);
            show_drag_region(ctx, ui, layout.drag_width);
        },
    );

    outcome
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
    let cell_context =
        slot_cell_context(context.app, context.duplicate_name_counts, TAB_BUTTON_WIDTH);
    collect_slot_entries(
        ui,
        context.app,
        &cell_context,
        context.outcome,
        |slot_index, rect| {
            record_visible_tab(
                slot_index,
                rect,
                context.viewport_rect,
                context.visible_tab_indices,
            );
        },
    )
}

fn collect_tab_drop_zones(
    ctx: &egui::Context,
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    layout: &HeaderLayout,
    duplicate_name_counts: &HashMap<String, usize>,
    visible_tab_indices: &mut HashSet<usize>,
    outcome: &mut TabStripOutcome,
) -> Vec<TabDropZone> {
    let mut drop_zones = Vec::new();

    if layout.visible_strip_width > 0.0
        && let Some(tab_bar_zone) = show_scrolling_tab_strip(
            ui,
            app,
            layout,
            duplicate_name_counts,
            visible_tab_indices,
            outcome,
        )
    {
        drop_zones.push(tab_bar_zone);
    }

    if (layout.has_overflow || app.overflow_popup_open)
        && let Some(overflow_zone) = show_overflow_controls(
            ctx,
            ui,
            app,
            layout,
            visible_tab_indices,
            duplicate_name_counts,
            outcome,
        )
    {
        drop_zones.push(overflow_zone);
    }

    drop_zones
}

fn render_tab_slot_cell(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    slot_index: usize,
    context: &SlotCellContext<'_>,
    outcome: &mut TabStripOutcome,
) -> super::tab_cell::TabCellOutcome {
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

    let (tab_response, close_response, _) =
        tab_button_sized(ui, "Settings", app.showing_settings(), false, context.width);
    tab_drag::begin_tab_drag_if_needed(ui, slot_index, &[slot_index], &tab_response, &close_response);
    apply_settings_tab_interaction(
        outcome,
        app.showing_settings(),
        close_response.clicked(),
        tab_response.clicked(),
    );
    maybe_scroll_to_active_tab(
        ui,
        slot_index,
        context.active_slot_index,
        context.pending_scroll_to_active,
        tab_response.rect,
        outcome,
    );
    super::tab_cell::TabCellOutcome {
        rect: tab_response.rect,
        interaction: super::TabInteraction::None,
    }
}

fn finish_tab_slot_cell(
    ui: &mut egui::Ui,
    slot_index: usize,
    context: &SlotCellContext<'_>,
    cell_outcome: super::tab_cell::TabCellOutcome,
    outcome: &mut TabStripOutcome,
) -> super::tab_cell::TabCellOutcome {
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

fn show_scrolling_tab_strip(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    layout: &HeaderLayout,
    duplicate_name_counts: &HashMap<String, usize>,
    visible_tab_indices: &mut HashSet<usize>,
    outcome: &mut TabStripOutcome,
) -> Option<TabDropZone> {
    let scroll_area_id = ui.id().with("tab_strip");
    let entries = allocate_tab_strip_entries(
        ui,
        app,
        layout,
        scroll_area_id,
        duplicate_name_counts,
        visible_tab_indices,
        outcome,
    );

    (!entries.is_empty()).then_some(TabDropZone {
        axis: TabDropAxis::Horizontal,
        entries,
    })
}

fn show_vertical_tab_entries_above_new_tab(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    duplicate_name_counts: &HashMap<String, usize>,
    outcome: &mut TabStripOutcome,
) -> Vec<TabDropZone> {
    let scroll_height = (ui.available_height() - BUTTON_SIZE.y - 8.0).max(0.0);
    let drop_zones = ui
        .allocate_ui_with_layout(
            egui::vec2(ui.available_width(), scroll_height),
            egui::Layout::top_down(egui::Align::Min),
            |ui| show_scrolling_vertical_tab_list(ui, app, duplicate_name_counts, outcome),
        )
        .inner
        .into_iter()
        .collect::<Vec<_>>();

    ui.add_space(8.0);
    show_vertical_new_tab_action(ui, app);
    drop_zones
}

fn show_vertical_new_tab_action(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    let width = ui.available_width().max(BUTTON_SIZE.x);
    if ui
        .add_sized(
            egui::vec2(width, BUTTON_SIZE.y),
            egui::Button::new(format!("{} New tab", egui_phosphor::regular::PLUS))
                .fill(action_bg(ui))
                .stroke(Stroke::new(1.0, border(ui))),
        )
        .on_hover_text("New Tab")
        .clicked()
    {
        app.handle_command(AppCommand::NewTab);
    }
}

fn show_scrolling_vertical_tab_list(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    duplicate_name_counts: &HashMap<String, usize>,
    outcome: &mut TabStripOutcome,
) -> Option<TabDropZone> {
    let scroll_area_id = ui.id().with("vertical_tab_list");
    let entries = egui::ScrollArea::vertical()
        .id_salt(scroll_area_id)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 4.0;
            let viewport_rect = ui.max_rect();
            maybe_auto_scroll_vertical_tab_list(ui, app, scroll_area_id, viewport_rect);
            collect_vertical_tab_entries(ui, app, duplicate_name_counts, outcome)
        })
        .inner;

    (!entries.is_empty()).then_some(TabDropZone {
        axis: TabDropAxis::Vertical,
        entries,
    })
}

fn collect_vertical_tab_entries(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    duplicate_name_counts: &HashMap<String, usize>,
    outcome: &mut TabStripOutcome,
) -> Vec<TabRectEntry> {
    let context = slot_cell_context(
        app,
        duplicate_name_counts,
        ui.available_width().max(TAB_BUTTON_WIDTH),
    );
    collect_slot_entries(ui, app, &context, outcome, |_, _| {})
}

fn slot_cell_context<'a>(
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

fn collect_slot_entries(
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

fn maybe_auto_scroll_vertical_tab_list(
    ui: &mut egui::Ui,
    app: &ScratchpadApp,
    scroll_area_id: egui::Id,
    viewport_rect: egui::Rect,
) {
    if let Some(scroll_state) = egui::scroll_area::State::load(ui.ctx(), scroll_area_id) {
        crate::app::ui::tab_drag::auto_scroll_tab_list(
            ui.ctx(),
            scroll_area_id,
            viewport_rect,
            estimated_vertical_tab_list_height(app, 4.0),
            &scroll_state,
            crate::app::ui::tab_drag::TabDropAxis::Vertical,
        );
    }
}

fn estimated_vertical_tab_list_height(app: &ScratchpadApp, spacing: f32) -> f32 {
    let tab_count = app.total_tab_slots();
    if tab_count > 0 {
        (tab_count as f32 * TAB_HEIGHT) + ((tab_count.saturating_sub(1)) as f32 * spacing)
    } else {
        0.0
    }
}

fn apply_tab_drag_feedback(
    ui: &mut egui::Ui,
    app: &ScratchpadApp,
    drop_zones: &[TabDropZone],
    outcome: &mut TabStripOutcome,
) {
    update_reordered_tabs(ui, app.total_tab_slots(), drop_zones, outcome);
    tab_drag::paint_dragged_tab_ghost(ui.ctx(), app);
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

fn render_new_tab_action(ui: &mut egui::Ui, app: &mut ScratchpadApp, spacing: f32) {
    ui.add_space(spacing);
    if crate::app::chrome::phosphor_button(
        ui,
        egui_phosphor::regular::PLUS,
        BUTTON_SIZE,
        action_bg(ui),
        action_hover_bg(ui),
        "New Tab",
    )
    .clicked()
    {
        app.handle_command(AppCommand::NewTab);
    }
}

fn update_reordered_tabs(
    ui: &mut egui::Ui,
    tab_count: usize,
    drop_zones: &[TabDropZone],
    outcome: &mut TabStripOutcome,
) {
    if let Some(commit) = tab_drag::update_tab_drag(ui, drop_zones, tab_count) {
        match commit {
            tab_drag::TabDragCommit::Reorder {
                from_index,
                to_index,
            } => outcome.reordered_tabs = Some((from_index, to_index)),
            tab_drag::TabDragCommit::ReorderGroup {
                from_indices,
                to_index,
            } => outcome.reordered_tab_group = Some((from_indices, to_index)),
            tab_drag::TabDragCommit::Combine {
                source_index,
                target_index,
            } => outcome.combined_tabs = Some((source_index, target_index)),
            tab_drag::TabDragCommit::CombineGroup {
                source_indices,
                target_index,
            } => outcome.combined_tab_group = Some((source_indices, target_index)),
        }
    }
}

fn show_overflow_controls(
    ctx: &egui::Context,
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    layout: &HeaderLayout,
    visible_tab_indices: &HashSet<usize>,
    duplicate_name_counts: &HashMap<String, usize>,
    outcome: &mut TabStripOutcome,
) -> Option<TabDropZone> {
    ui.add_space(layout.spacing);
    let mut overflow_popup_open = app.overflow_popup_open;
    let overflow_outcome = tab_overflow::show_overflow_button(
        ctx,
        ui,
        app,
        &mut overflow_popup_open,
        visible_tab_indices,
        duplicate_name_counts,
    );
    app.overflow_popup_open = overflow_popup_open;

    outcome.activated_tab = outcome.activated_tab.or(overflow_outcome.activated_tab);
    outcome.activate_settings = outcome.activate_settings || overflow_outcome.activate_settings;
    outcome.promote_all_files_tab = outcome
        .promote_all_files_tab
        .or(overflow_outcome.promote_all_files_tab);
    outcome.close_requested_tab = outcome
        .close_requested_tab
        .or(overflow_outcome.close_requested_tab);
    outcome.close_settings = outcome.close_settings || overflow_outcome.close_settings;
    overflow_outcome.drop_zone
}

pub(crate) fn show_drag_region(ctx: &egui::Context, ui: &mut egui::Ui, drag_width: f32) {
    if drag_width <= 0.0 {
        return;
    }

    let (rect, drag_response) =
        ui.allocate_exact_size(egui::vec2(drag_width, TAB_HEIGHT), Sense::click_and_drag());
    if drag_response.drag_started() {
        ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
    }
    if drag_response.double_clicked() {
        let maximized = ctx.input(|input| input.viewport().maximized.unwrap_or(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
    }
    ui.painter()
        .rect_filled(rect, 0.0, crate::app::theme::header_bg(ui));
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
