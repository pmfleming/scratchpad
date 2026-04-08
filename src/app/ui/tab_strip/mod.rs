pub mod actions;
pub mod layout;
pub mod tab_cell;

use crate::app::app_state::ScratchpadApp;
use crate::app::commands::AppCommand;
use crate::app::domain::WorkspaceTab;
use crate::app::theme::*;
use crate::app::ui::tab_drag::{self, TabDropZone};
use crate::app::ui::tab_overflow;
use eframe::egui::{self, Sense, Stroke};
use std::collections::{HashMap, HashSet};

pub(crate) use actions::{show_caption_controls, show_primary_actions};
pub(crate) use layout::HeaderLayout;
pub(crate) use tab_cell::{TabInteraction, render_tab_cell};

#[derive(Default)]
struct TabStripOutcome {
    activated_tab: Option<usize>,
    close_requested_tab: Option<usize>,
    reordered_tabs: Option<(usize, usize)>,
    consumed_scroll_request: bool,
}

pub(crate) fn show_header(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    let ctx = ui.ctx().clone();
    egui::Panel::top("header")
        .exact_size(HEADER_HEIGHT)
        .frame(
            egui::Frame::NONE
                .fill(HEADER_BG)
                .stroke(Stroke::new(1.0, BORDER))
                .inner_margin(egui::Margin {
                    left: HEADER_LEFT_PADDING as i8,
                    right: HEADER_RIGHT_PADDING as i8,
                    top: HEADER_VERTICAL_PADDING as i8,
                    bottom: HEADER_VERTICAL_PADDING as i8,
                }),
        )
        .show_inside(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                show_primary_actions(ui, app);

                ui.add_space(8.0);
                let layout = HeaderLayout::measure(app, ui.available_width(), 4.0);
                let outcome = show_tab_region(&ctx, ui, app, &layout);

                ui.add_space(8.0);
                show_caption_controls(&ctx, ui, app, &layout);
                apply_tab_outcome(app, outcome);
            });
        });
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
    let mut drop_zones = Vec::new();

    ui.allocate_ui_with_layout(
        egui::vec2(layout.tab_area_width, TAB_HEIGHT),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            if layout.visible_strip_width > 0.0
                && let Some(tab_bar_zone) = show_scrolling_tab_strip(
                    ui,
                    app,
                    layout,
                    &duplicate_name_counts,
                    &mut visible_tab_indices,
                    &mut outcome,
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
                    &visible_tab_indices,
                    &duplicate_name_counts,
                    &mut outcome,
                )
            {
                drop_zones.push(overflow_zone);
            }

            if let Some((from_index, to_index)) =
                tab_drag::update_tab_drag(ui, &drop_zones, app.tabs().len())
            {
                outcome.reordered_tabs = Some((from_index, to_index));
            }
            tab_drag::paint_dragged_tab_ghost(ui.ctx(), app.tabs());

            ui.add_space(layout.spacing);
            if crate::app::chrome::phosphor_button(
                ui,
                egui_phosphor::regular::PLUS,
                BUTTON_SIZE,
                ACTION_BG,
                ACTION_HOVER_BG,
                "New Tab",
            )
            .clicked()
            {
                app.handle_command(AppCommand::NewTab);
            }

            show_drag_region(ctx, ui, layout.drag_width);
        },
    );

    outcome
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
    let entries = ui
        .allocate_ui_with_layout(
            egui::vec2(layout.visible_strip_width, TAB_HEIGHT),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.set_width(layout.visible_strip_width);
                ui.set_min_width(layout.visible_strip_width);
                ui.set_max_width(layout.visible_strip_width);

                let viewport_rect = ui.max_rect();
                if let Some(scroll_state) = egui::scroll_area::State::load(ui.ctx(), scroll_area_id)
                {
                    crate::app::ui::tab_drag::auto_scroll_tab_strip(
                        ui.ctx(),
                        scroll_area_id,
                        viewport_rect,
                        app.estimated_tab_strip_width(layout.spacing),
                        &scroll_state,
                    );
                }

                egui::ScrollArea::horizontal()
                    .id_salt(scroll_area_id)
                    .auto_shrink([false, false])
                    .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.x = layout.spacing;
                        ui.horizontal(|ui| {
                            let mut row_rects = Vec::with_capacity(app.tabs().len());
                            for (index, tab) in app.tabs().iter().enumerate() {
                                let cell_outcome = render_tab_cell(
                                    ui,
                                    index,
                                    tab,
                                    app.active_tab_index() == index,
                                    app.tab_manager().pending_scroll_to_active,
                                    duplicate_name_counts,
                                );
                                apply_tab_interaction(outcome, cell_outcome.interaction);
                                if app.active_tab_index() == index
                                    && app.tab_manager().pending_scroll_to_active
                                {
                                    ui.scroll_to_rect(cell_outcome.rect, Some(egui::Align::Center));
                                    outcome.consumed_scroll_request = true;
                                }
                                if viewport_rect.intersects(cell_outcome.rect) {
                                    visible_tab_indices.insert(index);
                                }
                                row_rects.push(crate::app::ui::tab_drag::TabRectEntry {
                                    index,
                                    rect: cell_outcome.rect,
                                });
                            }
                            row_rects
                        })
                        .inner
                    })
                    .inner
            },
        )
        .inner;

    if entries.is_empty() {
        None
    } else {
        Some(TabDropZone {
            axis: tab_drag::TabDropAxis::Horizontal,
            entries,
        })
    }
}

fn apply_tab_interaction(outcome: &mut TabStripOutcome, interaction: TabInteraction) {
    match interaction {
        TabInteraction::None => {}
        TabInteraction::Activate(index) => outcome.activated_tab = Some(index),
        TabInteraction::RequestClose(index) => outcome.close_requested_tab = Some(index),
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
    let tab_manager = &app.tab_manager;
    let overflow_popup_open = &mut app.overflow_popup_open;
    let overflow_outcome = tab_overflow::show_overflow_button(
        ctx,
        ui,
        &tab_manager.tabs,
        tab_manager.active_tab_index,
        overflow_popup_open,
        visible_tab_indices,
        duplicate_name_counts,
    );

    outcome.activated_tab = outcome.activated_tab.or(overflow_outcome.activated_tab);
    outcome.close_requested_tab = outcome
        .close_requested_tab
        .or(overflow_outcome.close_requested_tab);
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
    ui.painter().rect_filled(rect, 0.0, HEADER_BG);
}

fn apply_tab_outcome(app: &mut ScratchpadApp, outcome: TabStripOutcome) {
    if let Some(index) = outcome.activated_tab {
        app.handle_command(AppCommand::ActivateTab { index });
    }

    if let Some(index) = outcome.close_requested_tab {
        app.handle_command(AppCommand::RequestCloseTab { index });
    }

    if let Some((from_index, to_index)) = outcome.reordered_tabs {
        app.handle_command(AppCommand::ReorderTab {
            from_index,
            to_index,
        });
    }

    if outcome.consumed_scroll_request {
        app.tab_manager_mut().pending_scroll_to_active = false;
    }
}

fn duplicate_name_counts(tabs: &[WorkspaceTab]) -> HashMap<String, usize> {
    let mut counts = HashMap::with_capacity(tabs.len());
    for tab in tabs {
        *counts.entry(tab.buffer.name.clone()).or_insert(0) += 1;
    }
    counts
}
