use crate::app::app_state::ScratchpadApp;
use crate::app::chrome::tab_button_sized_with_actions;
use crate::app::domain::WorkspaceTab;
use crate::app::services::settings_store::TabListPosition;
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
    pub(crate) activate_settings: bool,
    pub(crate) promote_all_files_tab: Option<usize>,
    pub(crate) close_requested_tab: Option<usize>,
    pub(crate) close_settings: bool,
    pub(crate) drop_zone: Option<tab_drag::TabDropZone>,
}

struct OverflowMenuContext<'a> {
    popup_width: f32,
    outcome: &'a mut OverflowMenuOutcome,
    overflow_popup_open: &'a mut bool,
}

struct OverflowRowState {
    selected: bool,
    display_name: String,
    can_promote_all_files: bool,
}

struct OverflowPopupRequest<'a> {
    app: &'a ScratchpadApp,
    visible_tab_indices: &'a HashSet<usize>,
    overflow_popup_id: egui::Id,
    anchor: egui::Pos2,
    pivot: egui::Align2,
}

const BOTTOM_OVERFLOW_GAP: f32 = 6.0;
const OVERFLOW_POPUP_VIEWPORT_MARGIN: f32 = 8.0;

pub(crate) fn show_overflow_button(
    ctx: &egui::Context,
    ui: &mut egui::Ui,
    app: &ScratchpadApp,
    overflow_popup_open: &mut bool,
    visible_tab_indices: &HashSet<usize>,
    _duplicate_name_counts: &HashMap<String, usize>,
) -> OverflowMenuOutcome {
    let mut outcome = OverflowMenuOutcome::default();
    let overflow_popup_id = ui.id().with("tab_overflow_popup");
    let overflow_button_response = overflow_button(ui);
    toggle_overflow_popup(overflow_popup_open, &overflow_button_response);

    let (anchor, pivot) = overflow_popup_anchor(app, overflow_button_response.rect);
    let popup_request = OverflowPopupRequest {
        app,
        visible_tab_indices,
        overflow_popup_id,
        anchor,
        pivot,
    };

    if let Some(popup_response) =
        show_overflow_popup(ctx, popup_request, overflow_popup_open, &mut outcome)
        && should_close_overflow_popup(
            ctx,
            &overflow_button_response,
            &popup_response,
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
            egui::RichText::new(egui_phosphor::regular::CARET_DOWN).color(text_primary(ui)),
        )
        .fill(action_bg(ui))
        .stroke(Stroke::new(1.0, border(ui))),
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
) -> Option<egui::Response> {
    if !*overflow_popup_open {
        return None;
    }

    let active_drag_sources = tab_drag::active_drag_sources_for_context(ctx);
    let popup_width = TAB_BUTTON_WIDTH;
    let popup_max_height = overflow_popup_max_height(ctx, request.anchor, request.pivot);
    let visible_row_count = overflow_row_count(request.app, request.visible_tab_indices) - 1;
    let area_response = egui::Area::new(request.overflow_popup_id)
        .order(egui::Order::Foreground)
        .constrain(true)
        .fixed_pos(request.anchor)
        .pivot(request.pivot)
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.set_width(popup_width);
                ui.set_min_width(popup_width);

                let mut menu = OverflowMenuContext {
                    popup_width,
                    outcome,
                    overflow_popup_open,
                };

                egui::ScrollArea::vertical()
                    .id_salt(request.overflow_popup_id.with("scroll"))
                    .auto_shrink([false, false])
                    .min_scrolled_height(overflow_popup_target_height(
                        visible_row_count,
                        popup_max_height,
                    ))
                    .max_height(popup_max_height)
                    .show(ui, |ui| {
                        collect_overflow_row_rects(
                            ui,
                            request.app,
                            &active_drag_sources,
                            request.visible_tab_indices,
                            &mut menu,
                        )
                    })
                    .inner
            })
        });

    outcome.drop_zone = build_overflow_drop_zone(&area_response.inner.inner);
    Some(area_response.response)
}

fn overflow_popup_anchor(
    app: &ScratchpadApp,
    button_rect: egui::Rect,
) -> (egui::Pos2, egui::Align2) {
    match app.tab_list_position() {
        TabListPosition::Bottom => (
            egui::pos2(button_rect.right(), button_rect.top() - BOTTOM_OVERFLOW_GAP),
            egui::Align2::RIGHT_BOTTOM,
        ),
        TabListPosition::Top | TabListPosition::Left | TabListPosition::Right => {
            (button_rect.right_bottom(), egui::Align2::RIGHT_TOP)
        }
    }
}

fn overflow_popup_max_height(ctx: &egui::Context, anchor: egui::Pos2, pivot: egui::Align2) -> f32 {
    let viewport = ctx.content_rect();
    let available_height = match pivot.y() {
        egui::Align::TOP => viewport.bottom() - anchor.y,
        egui::Align::BOTTOM => anchor.y - viewport.top(),
        egui::Align::Center => viewport.height(),
    };

    (available_height - OVERFLOW_POPUP_VIEWPORT_MARGIN).max(TAB_HEIGHT)
}

fn overflow_popup_target_height(visible_row_count: usize, popup_max_height: f32) -> f32 {
    ((visible_row_count.saturating_sub(1)) as f32 * TAB_HEIGHT)
        .min(popup_max_height)
        .max(TAB_HEIGHT)
}

fn overflow_row_count(app: &ScratchpadApp, visible_tab_indices: &HashSet<usize>) -> usize {
    (0..app.total_tab_slots())
        .filter(|slot_index| should_show_overflow_row(*slot_index, &[], visible_tab_indices))
        .count()
}

fn collect_overflow_row_rects(
    ui: &mut egui::Ui,
    app: &ScratchpadApp,
    active_drag_sources: &[usize],
    visible_tab_indices: &HashSet<usize>,
    menu: &mut OverflowMenuContext<'_>,
) -> Vec<tab_drag::TabRectEntry> {
    let mut row_rects = Vec::with_capacity(app.total_tab_slots());

    for slot_index in 0..app.total_tab_slots() {
        if !should_show_overflow_row(slot_index, active_drag_sources, visible_tab_indices) {
            continue;
        }

        let row_rect = show_overflow_row(
            ui,
            app,
            slot_index,
            active_drag_sources.contains(&slot_index),
            menu,
        );
        row_rects.push(tab_drag::TabRectEntry {
            index: slot_index,
            rect: row_rect,
            combine_enabled: !app.tab_slot_is_settings(slot_index),
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
    slot_index: usize,
    _active_drag_sources: &[usize],
    visible_tab_indices: &HashSet<usize>,
) -> bool {
    match overflow_list_mode() {
        OverflowListMode::AllTabs => true,
        OverflowListMode::HiddenTabsOnly => !visible_tab_indices.contains(&slot_index),
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
    app: &ScratchpadApp,
    slot_index: usize,
    is_drag_source: bool,
    menu: &mut OverflowMenuContext<'_>,
) -> egui::Rect {
    ui.push_id(("tab_overflow", slot_index), |ui| {
        if is_drag_source {
            return render_drag_source_placeholder(ui, menu.popup_width);
        }

        let Some(row_state) = overflow_row_state(app, slot_index) else {
            return render_drag_source_placeholder(ui, menu.popup_width);
        };

        let (response, promote_response, close_response, _truncated) =
            tab_button_sized_with_actions(
                ui,
                &row_state.display_name,
                row_state.selected,
                row_state.selected,
                row_state.can_promote_all_files,
                menu.popup_width,
            );
        tab_drag::begin_tab_drag_if_needed(
            ui,
            slot_index,
            &app.dragged_tab_slots(slot_index),
            &response,
            &close_response,
        );
        apply_overflow_row_actions(
            app,
            slot_index,
            &response,
            promote_response.as_ref(),
            &close_response,
            menu,
        );

        response.rect
    })
    .inner
}

fn overflow_row_state(app: &ScratchpadApp, slot_index: usize) -> Option<OverflowRowState> {
    Some(OverflowRowState {
        selected: app.tab_slot_selected(slot_index) || app.active_tab_slot_index() == slot_index,
        display_name: app.display_tab_name_at_slot(slot_index)?,
        can_promote_all_files: app
            .workspace_index_for_slot(slot_index)
            .and_then(|index| app.tabs().get(index))
            .is_some_and(WorkspaceTab::can_promote_all_files),
    })
}

fn apply_overflow_row_actions(
    app: &ScratchpadApp,
    slot_index: usize,
    response: &egui::Response,
    promote_response: Option<&egui::Response>,
    close_response: &egui::Response,
    menu: &mut OverflowMenuContext<'_>,
) {
    if promote_response.is_some_and(|promote| promote.clicked())
        && app.workspace_index_for_slot(slot_index).is_some()
    {
        menu.outcome.promote_all_files_tab = Some(slot_index);
        *menu.overflow_popup_open = false;
        return;
    }

    if response.clicked() {
        handle_overflow_slot_action(app, slot_index, menu, false);
    }

    if close_response.clicked() {
        handle_overflow_slot_action(app, slot_index, menu, true);
    }
}

fn handle_overflow_slot_action(
    app: &ScratchpadApp,
    slot_index: usize,
    menu: &mut OverflowMenuContext<'_>,
    is_close: bool,
) {
    if app.tab_slot_is_settings(slot_index) {
        if is_close {
            menu.outcome.close_settings = true;
        } else {
            menu.outcome.activate_settings = true;
        }
    } else if is_close {
        menu.outcome.close_requested_tab = Some(slot_index);
    } else {
        menu.outcome.activated_tab = Some(slot_index);
    }
    *menu.overflow_popup_open = false;
}

fn render_drag_source_placeholder(ui: &mut egui::Ui, width: f32) -> egui::Rect {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, TAB_HEIGHT), egui::Sense::hover());
    ui.painter()
        .rect_filled(rect, 4.0, tab_active_bg(ui).gamma_multiply(0.25));
    ui.painter().rect_stroke(
        rect,
        4.0,
        Stroke::new(1.0, border(ui).gamma_multiply(0.75)),
        egui::StrokeKind::Outside,
    );
    rect
}
