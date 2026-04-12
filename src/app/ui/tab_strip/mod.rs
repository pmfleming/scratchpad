pub mod actions;
mod entries;
pub mod layout;
mod outcome;
pub mod tab_cell;

use crate::app::app_state::ScratchpadApp;
use crate::app::commands::AppCommand;
use crate::app::domain::WorkspaceTab;
use crate::app::services::settings_store::{DEFAULT_TAB_LIST_WIDTH, TabListPosition};
use crate::app::theme::*;
use crate::app::ui::tab_drag::{self, TabDragCommit, TabDropZone};
use crate::app::ui::tab_overflow;
use eframe::egui::{self, Sense, Stroke};
use std::collections::{HashMap, HashSet};

pub(crate) use actions::{show_caption_controls, show_primary_actions};
use entries::allocate_tab_strip_entries;
pub(crate) use layout::HeaderLayout;
use outcome::apply_tab_outcome;
pub(crate) use tab_cell::{TabInteraction, render_tab_cell, render_tab_cell_sized};

const VERTICAL_TAB_LIST_PADDING: f32 = 8.0;
const VERTICAL_TAB_LIST_MIN_WIDTH: f32 = 96.0;
const VERTICAL_TAB_LIST_MAX_WIDTH: f32 = 360.0;
const AUTO_HIDE_PEEK_SIZE: f32 = 6.0;
const AUTO_HIDE_REVEAL_MARGIN: f32 = 12.0;

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
    if app.tab_list_position() != TabListPosition::Top {
        return;
    }

    let ctx = ui.ctx().clone();
    let header_visible = !app.auto_hide_tab_list() || top_bar_visible(ui);
    let header_height = if header_visible {
        HEADER_HEIGHT
    } else {
        AUTO_HIDE_PEEK_SIZE
    };
    egui::Panel::top("header")
        .exact_size(header_height)
        .frame(
            egui::Frame::NONE
                .fill(header_bg(ui))
                .stroke(Stroke::new(1.0, border(ui)))
                .inner_margin(egui::Margin {
                    left: HEADER_LEFT_PADDING as i8,
                    right: HEADER_RIGHT_PADDING as i8,
                    top: HEADER_VERTICAL_PADDING as i8,
                    bottom: HEADER_VERTICAL_PADDING as i8,
                }),
        )
        .show_inside(ui, |ui| {
            if !header_visible {
                return;
            }
            let outcome = show_horizontal_tab_bar(&ctx, ui, app, show_tabs_in_header(app));
            apply_tab_outcome(app, outcome);
        });
}

fn show_tabs_in_header(app: &ScratchpadApp) -> bool {
    app.tab_list_position() == TabListPosition::Top
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
    let panel_visible = !app.auto_hide_tab_list() || vertical_bar_visible(ui, app, side);
    let panel_width = if panel_visible {
        app.tab_list_width().max(DEFAULT_TAB_LIST_WIDTH)
    } else {
        AUTO_HIDE_PEEK_SIZE
    };
    let panel = match side {
        TabListPosition::Left => egui::Panel::left("vertical_tab_list_left"),
        TabListPosition::Right => egui::Panel::right("vertical_tab_list_right"),
        TabListPosition::Top | TabListPosition::Bottom => return,
    };

    let panel_response = panel
        .default_size(panel_width)
        .size_range(if panel_visible {
            VERTICAL_TAB_LIST_MIN_WIDTH..=VERTICAL_TAB_LIST_MAX_WIDTH
        } else {
            AUTO_HIDE_PEEK_SIZE..=AUTO_HIDE_PEEK_SIZE
        })
        .resizable(panel_visible)
        .frame(
            egui::Frame::NONE
                .fill(header_bg(ui))
                .stroke(Stroke::new(1.0, border(ui)))
                .inner_margin(egui::Margin {
                    left: VERTICAL_TAB_LIST_PADDING as i8,
                    right: VERTICAL_TAB_LIST_PADDING as i8,
                    top: VERTICAL_TAB_LIST_PADDING as i8,
                    bottom: VERTICAL_TAB_LIST_PADDING as i8,
                }),
        )
        .show_inside(ui, |ui| {
            if !panel_visible {
                return;
            }
            let outcome = show_vertical_tab_region(ui, app);
            apply_tab_outcome(app, outcome);
        });
    if panel_visible {
        app.set_tab_list_width_from_layout(panel_response.response.rect.width());
    }
}

fn show_vertical_tab_region(ui: &mut egui::Ui, app: &mut ScratchpadApp) -> TabStripOutcome {
    tab_drag::sync_drag_state(ui);
    let duplicate_name_counts = duplicate_name_counts(app.tabs());
    let mut outcome = TabStripOutcome::default();

    actions::show_vertical_primary_actions(ui, app);
    ui.add_space(8.0);
    let drop_zones =
        show_vertical_tab_entries_above_new_tab(ui, app, &duplicate_name_counts, &mut outcome);
    apply_tab_drag_feedback(ui, app, &drop_zones, &mut outcome);
    outcome
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
        axis: tab_drag::TabDropAxis::Vertical,
        entries,
    })
}

fn collect_vertical_tab_entries(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    duplicate_name_counts: &HashMap<String, usize>,
    outcome: &mut TabStripOutcome,
) -> Vec<tab_drag::TabRectEntry> {
    let active_slot_index = app.active_tab_slot_index();
    let pending_scroll_to_active = app.tab_manager().pending_scroll_to_active;
    let showing_settings = app.showing_settings();
    let width = ui.available_width().max(TAB_BUTTON_WIDTH);
    let total_slots = app.total_tab_slots();
    let mut entries = Vec::with_capacity(total_slots);

    for slot_index in 0..total_slots {
        let cell_outcome = if app.tab_slot_is_settings(slot_index) {
            render_vertical_settings_tab_cell(
                ui,
                app,
                slot_index,
                active_slot_index,
                width,
                outcome,
            )
        } else {
            let workspace_index = app
                .workspace_index_for_slot(slot_index)
                .unwrap_or(slot_index);
            let tab = &app.tabs()[workspace_index];
            let cell_outcome = render_tab_cell_sized(
                ui,
                slot_index,
                tab,
                !showing_settings && active_slot_index == slot_index,
                pending_scroll_to_active,
                duplicate_name_counts,
                width,
            );
            apply_tab_interaction(outcome, cell_outcome.interaction);
            maybe_scroll_to_active_tab(
                ui,
                slot_index,
                active_slot_index,
                pending_scroll_to_active,
                cell_outcome.rect,
                outcome,
            );
            cell_outcome
        };

        entries.push(tab_drag::TabRectEntry {
            index: slot_index,
            rect: cell_outcome.rect,
            combine_enabled: !app.tab_slot_is_settings(slot_index),
        });
    }

    entries
}

fn render_vertical_settings_tab_cell(
    ui: &mut egui::Ui,
    app: &ScratchpadApp,
    slot_index: usize,
    active_slot_index: usize,
    width: f32,
    outcome: &mut TabStripOutcome,
) -> tab_cell::TabCellOutcome {
    let (tab_response, close_response, _) =
        crate::app::chrome::tab_button_sized(ui, "Settings", app.showing_settings(), width);
    tab_drag::begin_tab_drag_if_needed(ui, slot_index, &tab_response, &close_response);
    entries::apply_settings_tab_interaction(
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
    tab_cell::TabCellOutcome {
        rect: tab_response.rect,
        interaction: TabInteraction::None,
    }
}

fn maybe_auto_scroll_vertical_tab_list(
    ui: &mut egui::Ui,
    app: &ScratchpadApp,
    scroll_area_id: egui::Id,
    viewport_rect: egui::Rect,
) {
    if let Some(scroll_state) = egui::scroll_area::State::load(ui.ctx(), scroll_area_id) {
        let content_height = estimated_vertical_tab_list_height(app, 4.0);
        crate::app::ui::tab_drag::auto_scroll_vertical_tab_list(
            ui.ctx(),
            scroll_area_id,
            viewport_rect,
            content_height,
            &scroll_state,
        );
    }
}

fn estimated_vertical_tab_list_height(app: &ScratchpadApp, spacing: f32) -> f32 {
    let tab_count = app.total_tab_slots();
    if tab_count == 0 {
        return 0.0;
    }

    (tab_count as f32 * TAB_HEIGHT) + ((tab_count.saturating_sub(1)) as f32 * spacing)
}

pub(crate) fn show_bottom_tab_list(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    if app.tab_list_position() != TabListPosition::Bottom {
        return;
    }

    let ctx = ui.ctx().clone();
    let bottom_bar_visible = !app.auto_hide_tab_list() || bottom_bar_visible(ui);
    let bottom_bar_height = if bottom_bar_visible {
        HEADER_HEIGHT
    } else {
        AUTO_HIDE_PEEK_SIZE
    };
    egui::Panel::bottom("bottom_tab_list")
        .exact_size(bottom_bar_height)
        .frame(
            egui::Frame::NONE
                .fill(header_bg(ui))
                .stroke(Stroke::new(1.0, border(ui)))
                .inner_margin(egui::Margin {
                    left: HEADER_LEFT_PADDING as i8,
                    right: HEADER_RIGHT_PADDING as i8,
                    top: HEADER_VERTICAL_PADDING as i8,
                    bottom: HEADER_VERTICAL_PADDING as i8,
                }),
        )
        .show_inside(ui, |ui| {
            if !bottom_bar_visible {
                return;
            }
            let outcome = show_horizontal_tab_bar(&ctx, ui, app, true);
            apply_tab_outcome(app, outcome);
        });
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

fn top_bar_visible(ui: &egui::Ui) -> bool {
    ui.input(|input| {
        input.pointer.hover_pos().is_some_and(|pos| {
            pos.y <= ui.max_rect().top() + HEADER_HEIGHT + AUTO_HIDE_REVEAL_MARGIN
        })
    })
}

fn bottom_bar_visible(ui: &egui::Ui) -> bool {
    ui.input(|input| {
        input.pointer.hover_pos().is_some_and(|pos| {
            pos.y >= ui.max_rect().bottom() - HEADER_HEIGHT - AUTO_HIDE_REVEAL_MARGIN
        })
    })
}

fn vertical_bar_visible(ui: &egui::Ui, app: &ScratchpadApp, side: TabListPosition) -> bool {
    let expanded_width = app.tab_list_width().max(DEFAULT_TAB_LIST_WIDTH);
    ui.input(|input| {
        input.pointer.hover_pos().is_some_and(|pos| match side {
            TabListPosition::Left => {
                pos.x <= ui.max_rect().left() + expanded_width + AUTO_HIDE_REVEAL_MARGIN
            }
            TabListPosition::Right => {
                pos.x >= ui.max_rect().right() - expanded_width - AUTO_HIDE_REVEAL_MARGIN
            }
            TabListPosition::Top | TabListPosition::Bottom => false,
        })
    })
}

fn show_tab_region(
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

    if let Some(tab_bar_zone) = maybe_show_scrolling_tab_strip(
        ui,
        app,
        layout,
        duplicate_name_counts,
        visible_tab_indices,
        outcome,
    ) {
        drop_zones.push(tab_bar_zone);
    }

    if let Some(overflow_zone) = maybe_show_overflow_controls(
        ctx,
        ui,
        app,
        layout,
        visible_tab_indices,
        duplicate_name_counts,
        outcome,
    ) {
        drop_zones.push(overflow_zone);
    }

    drop_zones
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

fn maybe_show_scrolling_tab_strip(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    layout: &HeaderLayout,
    duplicate_name_counts: &HashMap<String, usize>,
    visible_tab_indices: &mut HashSet<usize>,
    outcome: &mut TabStripOutcome,
) -> Option<TabDropZone> {
    if layout.visible_strip_width <= 0.0 {
        None
    } else {
        show_scrolling_tab_strip(
            ui,
            app,
            layout,
            duplicate_name_counts,
            visible_tab_indices,
            outcome,
        )
    }
}

fn maybe_show_overflow_controls(
    ctx: &egui::Context,
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    layout: &HeaderLayout,
    visible_tab_indices: &HashSet<usize>,
    duplicate_name_counts: &HashMap<String, usize>,
    outcome: &mut TabStripOutcome,
) -> Option<TabDropZone> {
    if layout.has_overflow || app.overflow_popup_open {
        show_overflow_controls(
            ctx,
            ui,
            app,
            layout,
            visible_tab_indices,
            duplicate_name_counts,
            outcome,
        )
    } else {
        None
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
            TabDragCommit::Reorder {
                from_index,
                to_index,
            } => outcome.reordered_tabs = Some((from_index, to_index)),
            TabDragCommit::Combine {
                source_index,
                target_index,
            } => outcome.combined_tabs = Some((source_index, target_index)),
        }
    }
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

    drop_zone_from_entries(entries)
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

fn drop_zone_from_entries(
    entries: Vec<crate::app::ui::tab_drag::TabRectEntry>,
) -> Option<TabDropZone> {
    (!entries.is_empty()).then_some(TabDropZone {
        axis: tab_drag::TabDropAxis::Horizontal,
        entries,
    })
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

fn show_drag_region(ctx: &egui::Context, ui: &mut egui::Ui, drag_width: f32) {
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
    ui.painter().rect_filled(rect, 0.0, header_bg(ui));
}

fn duplicate_name_counts(tabs: &[WorkspaceTab]) -> HashMap<String, usize> {
    let mut counts = HashMap::with_capacity(tabs.len());
    for tab in tabs {
        *counts.entry(tab.buffer.name.clone()).or_insert(0) += 1;
    }
    counts
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
