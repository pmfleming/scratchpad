use crate::app::chrome::tab_button_sized;
use crate::app::domain::WorkspaceTab;
use crate::app::theme::*;
use crate::app::ui::tab_drag;
use eframe::egui::{self, Stroke};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum OverflowListMode {
    AllTabs,
    HiddenTabsOnly,
}

pub(crate) const OVERFLOW_LIST_MODE_TOKEN: &str = "all-tabs";

#[derive(Default)]
pub(crate) struct OverflowMenuOutcome {
    pub(crate) activated_tab: Option<usize>,
    pub(crate) close_requested_tab: Option<usize>,
    pub(crate) drop_zone: Option<tab_drag::TabDropZone>,
}

struct OverflowMenuContext<'a> {
    popup_width: f32,
    duplicate_name_counts: &'a HashMap<String, usize>,
    outcome: &'a mut OverflowMenuOutcome,
    overflow_popup_open: &'a mut bool,
}

pub(crate) fn show_overflow_button(
    ctx: &egui::Context,
    ui: &mut egui::Ui,
    tabs: &[WorkspaceTab],
    active_tab_index: usize,
    overflow_popup_open: &mut bool,
    visible_tab_indices: &HashSet<usize>,
    duplicate_name_counts: &HashMap<String, usize>,
) -> OverflowMenuOutcome {
    let mut outcome = OverflowMenuOutcome::default();
    let overflow_popup_id = ui.id().with("tab_overflow_popup");
    let overflow_button_response = ui.add_sized(
        [BUTTON_SIZE.x, BUTTON_SIZE.y],
        egui::Button::new(
            egui::RichText::new(egui_phosphor::regular::CARET_DOWN).color(TEXT_PRIMARY),
        )
        .fill(ACTION_BG)
        .stroke(Stroke::new(1.0, BORDER)),
    );

    if overflow_button_response.clicked() {
        *overflow_popup_open = !*overflow_popup_open;
    }

    if *overflow_popup_open {
        let active_drag_source = tab_drag::active_drag_source_for_context(ctx);
        let popup_width = TAB_BUTTON_WIDTH;
        let area_response = egui::Area::new(overflow_popup_id)
            .order(egui::Order::Foreground)
            .constrain(true)
            .fixed_pos(overflow_button_response.rect.right_bottom())
            .pivot(egui::Align2::RIGHT_TOP)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_width(popup_width);
                    ui.set_min_width(popup_width);

                    let mut menu = OverflowMenuContext {
                        popup_width,
                        duplicate_name_counts,
                        outcome: &mut outcome,
                        overflow_popup_open,
                    };

                    let mut row_rects = Vec::with_capacity(tabs.len());

                    for (index, tab) in tabs.iter().enumerate() {
                        if !should_show_overflow_row(index, active_drag_source, visible_tab_indices)
                        {
                            continue;
                        }

                        let row_rect =
                            show_overflow_row(ui, index, tab, active_tab_index == index, &mut menu);
                        row_rects.push(tab_drag::TabRectEntry {
                            index,
                            rect: row_rect,
                        });
                    }

                    row_rects
                })
            });

        let row_rects = area_response.inner.inner;
        if !row_rects.is_empty() {
            outcome.drop_zone = Some(tab_drag::TabDropZone {
                axis: tab_drag::TabDropAxis::Vertical,
                entries: row_rects,
            });
        }

        if (!tab_drag::is_drag_active_for_context(ctx)
            && ctx.input(|input| input.key_pressed(egui::Key::Escape)))
            || (overflow_button_response.clicked_elsewhere()
                && !tab_drag::is_drag_active_for_context(ctx)
                && !area_response.response.hovered()
                && outcome.close_requested_tab.is_none())
        {
            *overflow_popup_open = false;
        }
    }

    outcome
}

fn should_show_overflow_row(
    index: usize,
    active_drag_source: Option<usize>,
    visible_tab_indices: &HashSet<usize>,
) -> bool {
    if active_drag_source == Some(index) {
        return false;
    }

    match overflow_list_mode() {
        OverflowListMode::AllTabs => true,
        OverflowListMode::HiddenTabsOnly => !visible_tab_indices.contains(&index),
    }
}

fn overflow_list_mode() -> OverflowListMode {
    match OVERFLOW_LIST_MODE_TOKEN {
        "all-tabs" => OverflowListMode::AllTabs,
        "overflow-only" => OverflowListMode::HiddenTabsOnly,
        _ => OverflowListMode::AllTabs,
    }
}

fn show_overflow_row(
    ui: &mut egui::Ui,
    index: usize,
    tab: &WorkspaceTab,
    selected: bool,
    menu: &mut OverflowMenuContext<'_>,
) -> egui::Rect {
    ui.push_id(("tab_overflow", index), |ui| {
        let has_duplicate = menu
            .duplicate_name_counts
            .get(&tab.buffer.name)
            .copied()
            .unwrap_or(0)
            > 1;

        let display_name = tab.full_display_name(has_duplicate);

        let (response, close_response, _truncated) =
            tab_button_sized(ui, &display_name, selected, menu.popup_width);
        tab_drag::begin_tab_drag_if_needed(ui, index, &response, &close_response);

        if response.clicked() {
            menu.outcome.activated_tab = Some(index);
            *menu.overflow_popup_open = false;
        }

        if close_response.clicked() {
            menu.outcome.close_requested_tab = Some(index);
        }

        response.rect
    })
    .inner
}
