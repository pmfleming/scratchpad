use crate::app::chrome::tab_button_sized_with_actions;
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
    pub(crate) promote_all_files_tab: Option<usize>,
    pub(crate) close_requested_tab: Option<usize>,
    pub(crate) drop_zone: Option<tab_drag::TabDropZone>,
}

struct OverflowMenuContext<'a> {
    popup_width: f32,
    duplicate_name_counts: &'a HashMap<String, usize>,
    outcome: &'a mut OverflowMenuOutcome,
    overflow_popup_open: &'a mut bool,
}

struct OverflowPopupRequest<'a> {
    tabs: &'a [WorkspaceTab],
    active_tab_index: usize,
    visible_tab_indices: &'a HashSet<usize>,
    duplicate_name_counts: &'a HashMap<String, usize>,
    overflow_popup_id: egui::Id,
    anchor: egui::Pos2,
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
    let overflow_button_response = overflow_button(ui);
    toggle_overflow_popup(overflow_popup_open, &overflow_button_response);

    let popup_request = OverflowPopupRequest {
        tabs,
        active_tab_index,
        visible_tab_indices,
        duplicate_name_counts,
        overflow_popup_id,
        anchor: overflow_button_response.rect.right_bottom(),
    };

    if let Some(area_response) =
        show_overflow_popup(ctx, popup_request, overflow_popup_open, &mut outcome)
        && should_close_overflow_popup(
            ctx,
            &overflow_button_response,
            &area_response.response,
            outcome.close_requested_tab,
        )
    {
        *overflow_popup_open = false;
    }

    outcome
}

fn overflow_button(ui: &mut egui::Ui) -> egui::Response {
    ui.add_sized(
        [BUTTON_SIZE.x, BUTTON_SIZE.y],
        egui::Button::new(
            egui::RichText::new(egui_phosphor::regular::CARET_DOWN).color(TEXT_PRIMARY),
        )
        .fill(ACTION_BG)
        .stroke(Stroke::new(1.0, BORDER)),
    )
}

fn toggle_overflow_popup(overflow_popup_open: &mut bool, response: &egui::Response) {
    if response.clicked() {
        *overflow_popup_open = !*overflow_popup_open;
    }
}

fn show_overflow_popup(
    ctx: &egui::Context,
    request: OverflowPopupRequest<'_>,
    overflow_popup_open: &mut bool,
    outcome: &mut OverflowMenuOutcome,
) -> Option<egui::InnerResponse<egui::InnerResponse<Vec<tab_drag::TabRectEntry>>>> {
    if !*overflow_popup_open {
        return None;
    }

    let active_drag_source = tab_drag::active_drag_source_for_context(ctx);
    let popup_width = TAB_BUTTON_WIDTH;
    let area_response = egui::Area::new(request.overflow_popup_id)
        .order(egui::Order::Foreground)
        .constrain(true)
        .fixed_pos(request.anchor)
        .pivot(egui::Align2::RIGHT_TOP)
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.set_width(popup_width);
                ui.set_min_width(popup_width);

                let mut menu = OverflowMenuContext {
                    popup_width,
                    duplicate_name_counts: request.duplicate_name_counts,
                    outcome,
                    overflow_popup_open,
                };

                collect_overflow_row_rects(
                    ui,
                    request.tabs,
                    request.active_tab_index,
                    active_drag_source,
                    request.visible_tab_indices,
                    &mut menu,
                )
            })
        });

    outcome.drop_zone = build_overflow_drop_zone(&area_response.inner.inner);
    Some(area_response)
}

fn collect_overflow_row_rects(
    ui: &mut egui::Ui,
    tabs: &[WorkspaceTab],
    active_tab_index: usize,
    active_drag_source: Option<usize>,
    visible_tab_indices: &HashSet<usize>,
    menu: &mut OverflowMenuContext<'_>,
) -> Vec<tab_drag::TabRectEntry> {
    let mut row_rects = Vec::with_capacity(tabs.len());

    for (index, tab) in tabs.iter().enumerate() {
        if !should_show_overflow_row(index, active_drag_source, visible_tab_indices) {
            continue;
        }

        let row_rect = show_overflow_row(
            ui,
            index,
            tab,
            active_tab_index == index,
            active_drag_source == Some(index),
            menu,
        );
        row_rects.push(tab_drag::TabRectEntry {
            index,
            rect: row_rect,
        });
    }

    row_rects
}

fn build_overflow_drop_zone(row_rects: &[tab_drag::TabRectEntry]) -> Option<tab_drag::TabDropZone> {
    if row_rects.is_empty() {
        None
    } else {
        Some(tab_drag::TabDropZone {
            axis: tab_drag::TabDropAxis::Vertical,
            entries: row_rects.to_vec(),
        })
    }
}

fn should_close_overflow_popup(
    ctx: &egui::Context,
    button_response: &egui::Response,
    popup_response: &egui::Response,
    close_requested_tab: Option<usize>,
) -> bool {
    if tab_drag::is_drag_active_for_context(ctx) {
        return false;
    }

    ctx.input(|input| input.key_pressed(egui::Key::Escape))
        || (button_response.clicked_elsewhere()
            && !popup_response.hovered()
            && close_requested_tab.is_none())
}

fn should_show_overflow_row(
    index: usize,
    _active_drag_source: Option<usize>,
    visible_tab_indices: &HashSet<usize>,
) -> bool {
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
    is_drag_source: bool,
    menu: &mut OverflowMenuContext<'_>,
) -> egui::Rect {
    ui.push_id(("tab_overflow", index), |ui| {
        if is_drag_source {
            return render_drag_source_placeholder(ui, menu.popup_width);
        }

        let has_duplicate = menu
            .duplicate_name_counts
            .get(&tab.buffer.name)
            .copied()
            .unwrap_or(0)
            > 1;

        let display_name = tab.full_display_name(has_duplicate);
        let can_promote_all_files = tab.can_promote_all_files();

        let (response, promote_response, close_response, _truncated) =
            tab_button_sized_with_actions(
                ui,
                &display_name,
                selected,
                can_promote_all_files,
                menu.popup_width,
            );
        tab_drag::begin_tab_drag_if_needed(ui, index, &response, &close_response);

        if promote_response.is_some_and(|promote| promote.clicked()) {
            menu.outcome.promote_all_files_tab = Some(index);
            *menu.overflow_popup_open = false;
        } else if response.clicked() {
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

fn render_drag_source_placeholder(ui: &mut egui::Ui, width: f32) -> egui::Rect {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, TAB_HEIGHT), egui::Sense::hover());
    ui.painter()
        .rect_filled(rect, 4.0, TAB_ACTIVE_BG.gamma_multiply(0.25));
    ui.painter().rect_stroke(
        rect,
        4.0,
        Stroke::new(1.0, BORDER.gamma_multiply(0.75)),
        egui::StrokeKind::Outside,
    );
    rect
}
