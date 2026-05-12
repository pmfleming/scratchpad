use crate::app::app_state::ScratchpadApp;
use crate::app::chrome::{TabButtonOptions, tab_button_with_actions};
use crate::app::domain::TabAttentionState;
use crate::app::domain::tab::summary;
use crate::app::services::settings_store::TabListPosition;
use crate::app::theme::*;
use crate::app::ui::tab_drag;
use crate::app::ui::tab_strip::context_menu::attach_tab_list_context_menu;
use crate::app::ui::widget_ids;
use eframe::egui::{self, Stroke};
use std::collections::{HashMap, HashSet};
use std::time::Duration;

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
    row_width: f32,
    outcome: &'a mut OverflowMenuOutcome,
    overflow_popup_open: &'a mut bool,
}

struct OverflowRowState {
    selected: bool,
    display_name: String,
    can_promote_all_files: bool,
    attention_state: Option<TabAttentionState>,
}

#[derive(Clone, Copy)]
enum OverflowRowAction {
    None,
    Promote,
    Activate,
    Close,
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
const OVERFLOW_DRAG_HOVER_OPEN_DELAY_SECONDS: f64 = 0.4;

pub(crate) fn show_overflow_button(
    ctx: &egui::Context,
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    overflow_popup_open: &mut bool,
    visible_tab_indices: &HashSet<usize>,
    _duplicate_name_counts: &HashMap<String, usize>,
) -> OverflowMenuOutcome {
    let mut outcome = OverflowMenuOutcome::default();
    let overflow_popup_id = widget_ids::root_id("tab_overflow_popup");
    let overflow_button_response = overflow_button(ui);
    attach_tab_list_context_menu(&overflow_button_response, app);
    toggle_overflow_popup(overflow_popup_open, &overflow_button_response);
    maybe_open_overflow_popup_for_tab_drag(ctx, &overflow_button_response, overflow_popup_open);

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
    widget_ids::surface_response(
        ui,
        "tab_overflow.button",
        widget_ids::WidgetRole::IconButton,
        |ui| {
            ui.add_sized(
                [BUTTON_SIZE.x, BUTTON_SIZE.y],
                egui::Button::new(
                    egui::RichText::new(egui_phosphor::regular::CARET_DOWN).color(text_primary(ui)),
                )
                .fill(action_bg(ui))
                .stroke(Stroke::new(1.0, border(ui))),
            )
        },
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
    let row_width = TAB_BUTTON_WIDTH;
    let popup_width = row_width + TAB_LIST_SCROLLBAR_GUTTER;
    let popup_max_height = overflow_popup_max_height(ctx, request.anchor, request.pivot);
    let visible_row_count = overflow_row_count(request.app, request.visible_tab_indices);
    let popup_target_height = overflow_popup_target_height(visible_row_count, popup_max_height);
    let area_response = widget_ids::area(request.overflow_popup_id)
        .order(egui::Order::Foreground)
        .constrain(true)
        .fixed_pos(request.anchor)
        .pivot(request.pivot)
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.set_width(popup_width);
                ui.set_min_width(popup_width);

                let mut menu = OverflowMenuContext {
                    row_width,
                    outcome,
                    overflow_popup_open,
                };

                let scroll_output = ui
                    .scope(|ui| {
                        ui.spacing_mut().scroll = tab_list_scroll_style();
                        egui::ScrollArea::vertical()
                            .id_salt(overflow_popup_scroll_id(request.overflow_popup_id))
                            .auto_shrink([false, true])
                            .min_scrolled_height(popup_target_height)
                            .max_height(popup_target_height)
                            .show(ui, |ui| {
                                collect_overflow_row_rects(
                                    ui,
                                    request.app,
                                    &active_drag_sources,
                                    request.visible_tab_indices,
                                    &mut menu,
                                )
                            })
                    })
                    .inner;
                maybe_auto_scroll_overflow_popup(
                    ui,
                    scroll_output.id,
                    request.app,
                    request.visible_tab_indices,
                    scroll_output.inner_rect,
                    &scroll_output.state,
                );
                scroll_output.inner
            })
        });

    outcome.drop_zone = build_overflow_drop_zone(&area_response.inner.inner);
    Some(area_response.response)
}

fn maybe_open_overflow_popup_for_tab_drag(
    ctx: &egui::Context,
    button_response: &egui::Response,
    overflow_popup_open: &mut bool,
) {
    if !should_track_overflow_drag_hover(ctx, button_response, *overflow_popup_open) {
        clear_overflow_drag_hover_start(ctx);
        return;
    }

    let now = ctx.input(|input| input.time);
    let hover_started_at = overflow_drag_hover_started_at(ctx).unwrap_or_else(|| {
        store_overflow_drag_hover_start(ctx, now);
        now
    });

    if overflow_drag_hover_ready(hover_started_at, now) {
        *overflow_popup_open = true;
        clear_overflow_drag_hover_start(ctx);
    } else {
        ctx.request_repaint_after(overflow_drag_hover_remaining(hover_started_at, now));
    }
}

fn should_track_overflow_drag_hover(
    ctx: &egui::Context,
    button_response: &egui::Response,
    overflow_popup_open: bool,
) -> bool {
    !overflow_popup_open
        && tab_drag::is_drag_active_for_context(ctx)
        && pointer_over_response_rect(ctx, button_response)
}

fn pointer_over_response_rect(ctx: &egui::Context, response: &egui::Response) -> bool {
    ctx.input(|input| {
        input
            .pointer
            .latest_pos()
            .is_some_and(|pos| response.rect.contains(pos))
    })
}

fn maybe_auto_scroll_overflow_popup(
    ui: &mut egui::Ui,
    scroll_area_id: egui::Id,
    app: &ScratchpadApp,
    visible_tab_indices: &HashSet<usize>,
    viewport_rect: egui::Rect,
    scroll_state: &egui::scroll_area::State,
) {
    tab_drag::auto_scroll_tab_list(
        ui.ctx(),
        scroll_area_id,
        viewport_rect,
        estimated_overflow_popup_content_height(app, visible_tab_indices),
        scroll_state,
        tab_drag::TabDropAxis::Vertical,
    );
}

fn estimated_overflow_popup_content_height(
    app: &ScratchpadApp,
    visible_tab_indices: &HashSet<usize>,
) -> f32 {
    overflow_row_count(app, visible_tab_indices) as f32 * TAB_HEIGHT
}

fn overflow_popup_scroll_id(overflow_popup_id: egui::Id) -> egui::Id {
    overflow_popup_id.with("scroll")
}

fn overflow_drag_hover_ready(started_at: f64, now: f64) -> bool {
    now - started_at >= OVERFLOW_DRAG_HOVER_OPEN_DELAY_SECONDS
}

fn overflow_drag_hover_remaining(started_at: f64, now: f64) -> Duration {
    Duration::from_secs_f64((OVERFLOW_DRAG_HOVER_OPEN_DELAY_SECONDS - (now - started_at)).max(0.0))
}

fn overflow_drag_hover_started_at(ctx: &egui::Context) -> Option<f64> {
    ctx.data(|data| data.get_temp::<f64>(overflow_drag_hover_start_id()))
}

fn store_overflow_drag_hover_start(ctx: &egui::Context, started_at: f64) {
    ctx.data_mut(|data| {
        data.insert_temp(overflow_drag_hover_start_id(), started_at);
    });
}

fn clear_overflow_drag_hover_start(ctx: &egui::Context) {
    ctx.data_mut(|data| {
        data.remove::<f64>(overflow_drag_hover_start_id());
    });
}

fn overflow_drag_hover_start_id() -> egui::Id {
    widget_ids::ctx_key("tab_overflow_drag_hover_start")
}

fn overflow_popup_anchor(
    app: &ScratchpadApp,
    button_rect: egui::Rect,
) -> (egui::Pos2, egui::Align2) {
    match app.state.app_settings.tab_list_position() {
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
    (visible_row_count as f32 * TAB_HEIGHT)
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
    widget_ids::surface_scope(ui, ("tab_overflow.slot", slot_index), |ui| {
        if is_drag_source {
            return render_drag_source_placeholder(ui, menu.row_width);
        }

        let Some(row_state) = overflow_row_state(app, slot_index) else {
            return render_drag_source_placeholder(ui, menu.row_width);
        };

        let (response, promote_response, close_response, truncated) = tab_button_with_actions(
            ui,
            ("tab_overflow.slot", slot_index),
            &row_state.display_name,
            row_state.selected,
            row_state.selected,
            TabButtonOptions::with_actions(
                menu.row_width,
                row_state.can_promote_all_files,
                row_state.attention_state.map(attention_color),
            ),
        );
        let response = maybe_attach_overflow_row_tooltip(response, &row_state, truncated);
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

fn maybe_attach_overflow_row_tooltip(
    response: egui::Response,
    row_state: &OverflowRowState,
    truncated: bool,
) -> egui::Response {
    if truncated || !row_state.display_name.is_empty() {
        response.on_hover_text(row_state.display_name.clone())
    } else {
        response
    }
}

fn overflow_row_state(app: &ScratchpadApp, slot_index: usize) -> Option<OverflowRowState> {
    Some(OverflowRowState {
        selected: app.tab_slot_selected(slot_index) || app.active_tab_slot_index() == slot_index,
        display_name: app.display_tab_name_at_slot(slot_index)?,
        can_promote_all_files: app
            .workspace_index_for_slot(slot_index)
            .and_then(|index| app.tab_manager.tabs.as_slice().get(index))
            .is_some_and(summary::can_promote_all_files),
        attention_state: app
            .workspace_index_for_slot(slot_index)
            .and_then(|index| app.tab_manager.tabs.as_slice().get(index))
            .and_then(summary::attention_state),
    })
}

fn attention_color(state: TabAttentionState) -> egui::Color32 {
    match state {
        TabAttentionState::AutoEdit => egui::Color32::from_rgb(230, 132, 46),
        TabAttentionState::Dirty => egui::Color32::from_rgb(70, 176, 96),
        TabAttentionState::DiskProblem => egui::Color32::from_rgb(220, 64, 64),
    }
}

fn apply_overflow_row_actions(
    app: &ScratchpadApp,
    slot_index: usize,
    response: &egui::Response,
    promote_response: Option<&egui::Response>,
    close_response: &egui::Response,
    menu: &mut OverflowMenuContext<'_>,
) {
    match overflow_row_action(app, slot_index, response, promote_response, close_response) {
        OverflowRowAction::None => {}
        OverflowRowAction::Promote => {
            menu.outcome.promote_all_files_tab = Some(slot_index);
            *menu.overflow_popup_open = false;
        }
        OverflowRowAction::Activate => handle_overflow_slot_action(app, slot_index, menu, false),
        OverflowRowAction::Close => handle_overflow_slot_action(app, slot_index, menu, true),
    }
}

fn overflow_row_action(
    app: &ScratchpadApp,
    slot_index: usize,
    response: &egui::Response,
    promote_response: Option<&egui::Response>,
    close_response: &egui::Response,
) -> OverflowRowAction {
    if promote_response.is_some_and(|promote| promote.clicked())
        && app.workspace_index_for_slot(slot_index).is_some()
    {
        return OverflowRowAction::Promote;
    }
    if close_response.clicked() {
        return OverflowRowAction::Close;
    }
    if response.clicked() {
        return OverflowRowAction::Activate;
    }
    OverflowRowAction::None
}

fn handle_overflow_slot_action(
    app: &ScratchpadApp,
    slot_index: usize,
    menu: &mut OverflowMenuContext<'_>,
    is_close: bool,
) {
    match (app.tab_slot_is_settings(slot_index), is_close) {
        (true, true) => menu.outcome.close_settings = true,
        (true, false) => menu.outcome.activate_settings = true,
        (false, true) => menu.outcome.close_requested_tab = Some(slot_index),
        (false, false) => menu.outcome.activated_tab = Some(slot_index),
    }
    *menu.overflow_popup_open = false;
}

fn render_drag_source_placeholder(ui: &mut egui::Ui, width: f32) -> egui::Rect {
    let (_, rect) = ui.allocate_space(egui::vec2(width, TAB_HEIGHT));
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

#[cfg(test)]
mod tests {
    use super::{
        overflow_drag_hover_ready, overflow_drag_hover_remaining, overflow_popup_target_height,
    };
    use crate::app::theme::TAB_HEIGHT;

    #[test]
    fn overflow_popup_height_tracks_visible_rows() {
        assert_eq!(
            overflow_popup_target_height(4, TAB_HEIGHT * 20.0),
            TAB_HEIGHT * 4.0
        );
    }

    #[test]
    fn overflow_popup_height_caps_at_viewport_max() {
        assert_eq!(
            overflow_popup_target_height(20, TAB_HEIGHT * 4.5),
            TAB_HEIGHT * 4.5
        );
    }

    #[test]
    fn drag_hover_open_waits_for_dwell_delay() {
        assert!(!overflow_drag_hover_ready(10.0, 10.39));
        assert!(overflow_drag_hover_ready(10.0, 10.4));
    }

    #[test]
    fn drag_hover_repaint_uses_remaining_delay() {
        assert_eq!(
            overflow_drag_hover_remaining(10.0, 10.25),
            std::time::Duration::from_millis(150)
        );
    }
}
