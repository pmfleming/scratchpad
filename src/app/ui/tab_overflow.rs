use crate::app::app_state::{ScratchpadApp, workspace::display_tabs};
use crate::app::chrome::{TabButtonOptions, tab_button_with_actions, tab_label_font_id};
use crate::app::services::settings_store::TabListPosition;
use crate::app::theme::{
    BUTTON_SIZE, TAB_HEIGHT, TAB_LIST_SCROLLBAR_GUTTER, action_bg, border, tab_active_bg,
    tab_list_scroll_style, text_primary,
};
use crate::app::ui::tab_drag;
use crate::app::ui::tab_strip::context_menu::attach_tab_list_context_menu;
use crate::app::ui::tab_strip::tab_cell::{TabPresentation, attention_marker, tab_presentation};
use crate::app::ui::widget_ids;
use eframe::egui::{self, Stroke};
use std::collections::HashMap;
use std::time::Duration;

#[cfg(test)]
mod tests;

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
    row_height: f32,
    label_font_id: egui::FontId,
    outcome: &'a mut OverflowMenuOutcome,
    overflow_popup_open: &'a mut bool,
}

struct OverflowRowState {
    selected: bool,
    presentation: TabPresentation,
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
    duplicate_name_counts: &'a HashMap<String, usize>,
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
    duplicate_name_counts: &HashMap<String, usize>,
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
        duplicate_name_counts,
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
                    egui::RichText::new(egui_phosphor::regular::CARET_DOWN)
                        .font(egui_phosphor::font_id(14.0))
                        .color(text_primary(ui)),
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
    let (mut filter, opening) = begin_filter(ctx, *overflow_popup_open)?;
    let matching = matching_slots(request.app, request.duplicate_name_counts, &filter.text);
    filter_keys(ctx, &mut filter, &matching);

    let active_drag_sources = tab_drag::active_drag_sources_for_context(ctx);
    let row_width =
        crate::app::chrome::tab_button_width(request.app.state.app_settings.font_size())
            .min((ctx.content_rect().width() - TAB_LIST_SCROLLBAR_GUTTER - 16.0).max(0.0));
    let popup_width = row_width + TAB_LIST_SCROLLBAR_GUTTER;
    let popup_max_height = overflow_popup_max_height(ctx, request.anchor, request.pivot);
    let visible_row_count = matching.len();
    let row_height =
        crate::app::chrome::tab_row_height(ctx, request.app.state.app_settings.font_size());
    let popup_target_height =
        overflow_popup_target_height(visible_row_count, popup_max_height, row_height);
    let area_response = widget_ids::area(request.overflow_popup_id)
        .order(egui::Order::Foreground)
        .constrain(true)
        .fixed_pos(request.anchor)
        .pivot(request.pivot)
        .show(ctx, |ui| {
            crate::app::ui::panel_style::window_frame(
                ctx,
                request.app.state.app_settings.editor_background_color(),
            )
            .inner_margin(egui::Margin::symmetric(0, 6))
            .show(ui, |ui| {
                ui.set_width(popup_width);
                ui.set_min_width(popup_width);
                crate::app::ui::panel_style::apply(ui);
                filter_field(ui, &mut filter, opening, matching.len());

                let mut menu = OverflowMenuContext {
                    row_width,
                    row_height,
                    label_font_id: tab_label_font_id(request.app.state.app_settings.font_size()),
                    outcome,
                    overflow_popup_open,
                };

                let scroll_output = ui
                    .scope(|ui| {
                        ui.spacing_mut().scroll = tab_list_scroll_style();
                        widget_ids::scroll_area(
                            ui,
                            egui::ScrollArea::vertical(),
                            overflow_popup_scroll_id(request.overflow_popup_id),
                        )
                        .auto_shrink([false, true])
                        .min_scrolled_height(popup_target_height)
                        .max_height(popup_target_height)
                        .show(ui, |ui| {
                            collect_overflow_row_rects(
                                ui,
                                request.app,
                                request.duplicate_name_counts,
                                &active_drag_sources,
                                &matching,
                                &mut menu,
                            )
                        })
                    })
                    .inner;
                maybe_auto_scroll_overflow_popup(
                    ui,
                    scroll_output.id,
                    request.app,
                    scroll_output.inner_rect,
                    &scroll_output.state,
                );
                filter_footer(ui, &filter, &scroll_output.inner);
                scroll_output.inner
            })
        });

    if std::mem::take(&mut filter.activate)
        && let Some(slot) = filter.highlight.or_else(|| matching.first().copied())
    {
        if display_tabs::tab_slot_is_settings(request.app, slot) {
            outcome.activate_settings = true;
        } else {
            outcome.activated_tab = Some(slot);
        }
        *overflow_popup_open = false;
    }
    ctx.data_mut(|d| d.insert_temp(filter_state_id(), filter));
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
    viewport_rect: egui::Rect,
    scroll_state: &egui::scroll_area::State,
) {
    tab_drag::auto_scroll_tab_list(
        ui.ctx(),
        scroll_area_id,
        viewport_rect,
        estimated_overflow_popup_content_height(ui, app),
        scroll_state,
        tab_drag::TabDropAxis::Vertical,
    );
}

fn estimated_overflow_popup_content_height(ui: &egui::Ui, app: &ScratchpadApp) -> f32 {
    display_tabs::total_tab_slots(app) as f32
        * crate::app::chrome::tab_row_height(ui.ctx(), app.state.app_settings.font_size())
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

fn overflow_popup_target_height(
    visible_row_count: usize,
    popup_max_height: f32,
    row_height: f32,
) -> f32 {
    (visible_row_count as f32 * row_height)
        .max(row_height)
        .min(popup_max_height)
}

fn collect_overflow_row_rects(
    ui: &mut egui::Ui,
    app: &ScratchpadApp,
    duplicate_name_counts: &HashMap<String, usize>,
    active_drag_sources: &[usize],
    matching: &[usize],
    menu: &mut OverflowMenuContext<'_>,
) -> Vec<tab_drag::TabRectEntry> {
    let mut row_rects = Vec::with_capacity(matching.len());
    for &slot_index in matching {
        let row_rect = show_overflow_row(
            ui,
            app,
            slot_index,
            duplicate_name_counts,
            active_drag_sources.contains(&slot_index),
            menu,
        );
        row_rects.push(tab_drag::TabRectEntry {
            index: slot_index,
            rect: row_rect,
            combine_enabled: !display_tabs::tab_slot_is_settings(app, slot_index),
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

fn show_overflow_row(
    ui: &mut egui::Ui,
    app: &ScratchpadApp,
    slot_index: usize,
    duplicate_name_counts: &HashMap<String, usize>,
    is_drag_source: bool,
    menu: &mut OverflowMenuContext<'_>,
) -> egui::Rect {
    widget_ids::surface_scope(ui, ("tab_overflow.slot", slot_index), |ui| {
        let row_height = menu.row_height;
        if is_drag_source {
            return render_drag_source_placeholder(ui, menu.row_width, row_height);
        }

        let Some(row_state) = overflow_row_state(app, slot_index, duplicate_name_counts) else {
            return render_drag_source_placeholder(ui, menu.row_width, row_height);
        };

        let (response, promote_response, close_response, truncated) = tab_button_with_actions(
            ui,
            ("tab_overflow.slot", slot_index),
            &row_state.presentation.display_name,
            row_state.selected,
            row_state.selected,
            TabButtonOptions::with_actions(
                menu.row_width,
                row_state.presentation.can_promote_all_files,
                row_state.presentation.attention_state.map(attention_marker),
            )
            .with_label_font_id(menu.label_font_id.clone())
            .with_row_height(row_height),
        );
        let response = maybe_attach_overflow_row_tooltip(response, &row_state, truncated);
        tab_drag::begin_tab_drag_if_needed(
            ui,
            slot_index,
            &display_tabs::dragged_tab_slots(app, slot_index),
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
    if truncated || !row_state.presentation.tooltip.is_empty() {
        response.on_hover_text(row_state.presentation.tooltip.clone())
    } else {
        response
    }
}

fn overflow_row_state(
    app: &ScratchpadApp,
    slot_index: usize,
    duplicate_name_counts: &HashMap<String, usize>,
) -> Option<OverflowRowState> {
    Some(OverflowRowState {
        selected: display_tabs::tab_slot_selected(app, slot_index)
            || display_tabs::active_tab_slot_index(app) == slot_index,
        presentation: tab_presentation(app, slot_index, duplicate_name_counts)?,
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
        && display_tabs::workspace_index_for_slot(app, slot_index).is_some()
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
    match (
        display_tabs::tab_slot_is_settings(app, slot_index),
        is_close,
    ) {
        (true, true) => menu.outcome.close_settings = true,
        (true, false) => menu.outcome.activate_settings = true,
        (false, true) => menu.outcome.close_requested_tab = Some(slot_index),
        (false, false) => menu.outcome.activated_tab = Some(slot_index),
    }
    *menu.overflow_popup_open = false;
}

fn render_drag_source_placeholder(ui: &mut egui::Ui, width: f32, row_height: f32) -> egui::Rect {
    let (_, rect) = ui.allocate_space(egui::vec2(width, row_height));
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

const OVERFLOW_KEY_HINTS: &str = "↑ ↓ Move   Enter Open   Esc Close";

/// Type-to-filter and keyboard state for one opening of the list.
#[derive(Clone, Default)]
struct OverflowFilter {
    text: String,
    /// The row ↑ ↓ moved to, by slot index.
    highlight: Option<usize>,
    /// Enter was pressed; open the highlighted or first row.
    activate: bool,
    opened: bool,
}

fn filter_state_id() -> egui::Id {
    widget_ids::ctx_key("tab_overflow_filter")
}

fn filter_field_id() -> egui::Id {
    widget_ids::ctx_key("tab_overflow_filter_field")
}

/// Rows the list shows: every listed tab whose name contains the filter text.
fn matching_slots(
    app: &ScratchpadApp,
    duplicate_name_counts: &HashMap<String, usize>,
    text: &str,
) -> Vec<usize> {
    let needle = text.trim().to_lowercase();
    (0..display_tabs::total_tab_slots(app))
        .filter(|&slot| {
            needle.is_empty()
                || tab_presentation(app, slot, duplicate_name_counts)
                    .is_some_and(|p| p.display_name.to_lowercase().contains(&needle))
        })
        .collect()
}

/// ↑ ↓ move through the filtered rows and Enter opens one, while the filter has focus.
fn filter_keys(ctx: &egui::Context, filter: &mut OverflowFilter, matching: &[usize]) {
    if !ctx.memory(|m| m.has_focus(filter_field_id())) {
        return;
    }
    let position = filter
        .highlight
        .and_then(|slot| matching.iter().position(|&s| s == slot));
    let consume = |key| ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, key));
    let last = matching.len().saturating_sub(1);
    let target = if consume(egui::Key::ArrowDown) {
        Some(position.map_or(0, |i| (i + 1).min(last)))
    } else if consume(egui::Key::ArrowUp) {
        Some(position.map_or(0, |i| i.saturating_sub(1)))
    } else {
        None
    };
    if let Some(index) = target {
        filter.highlight = matching.get(index).copied();
    }
    if filter
        .highlight
        .is_some_and(|slot| !matching.contains(&slot))
    {
        filter.highlight = None;
    }
    filter.activate |= consume(egui::Key::Enter) && !matching.is_empty();
}

fn filter_field(ui: &mut egui::Ui, filter: &mut OverflowFilter, opening: bool, matches: usize) {
    ui.horizontal(|ui| {
        ui.add_space(10.0);
        let response = ui.add(
            egui::TextEdit::singleline(&mut filter.text)
                .id(filter_field_id())
                .hint_text("Filter")
                .frame(egui::Frame::NONE)
                .desired_width(ui.available_width() - 10.0),
        );
        response.widget_info(|| egui::WidgetInfo::text_edit(true, "", &filter.text, "Filter tabs"));
        if response.changed() {
            filter.highlight = None;
        }
        if opening {
            response.request_focus();
        }
        let line = response.rect.expand2(egui::vec2(0.0, 2.0));
        ui.painter().line_segment(
            [line.left_bottom(), line.right_bottom()],
            egui::Stroke::new(
                1.0,
                if response.has_focus() {
                    crate::app::theme::tab_selected_accent(ui)
                } else {
                    border(ui)
                },
            ),
        );
        if matches == 0 {
            ui.label(egui::RichText::new("No tabs").size(11.0).weak());
        }
    });
    ui.add_space(4.0);
}

/// The filter for this opening of the list; `None` while the list is closed.
/// The flag is true on the frame the list opens.
fn begin_filter(ctx: &egui::Context, open: bool) -> Option<(OverflowFilter, bool)> {
    let filter = ctx
        .data(|d| d.get_temp::<OverflowFilter>(filter_state_id()))
        .unwrap_or_default();
    if !open {
        if filter.opened {
            ctx.data_mut(|d| d.remove::<OverflowFilter>(filter_state_id()));
        }
        return None;
    }
    if filter.opened {
        return Some((filter, false));
    }
    Some((
        OverflowFilter {
            opened: true,
            ..OverflowFilter::default()
        },
        true,
    ))
}

/// Marks the keyboard row and shows the key hints under the list.
fn filter_footer(ui: &mut egui::Ui, filter: &OverflowFilter, rows: &[tab_drag::TabRectEntry]) {
    if let Some(row) = filter
        .highlight
        .and_then(|slot| rows.iter().find(|row| row.index == slot))
    {
        ui.painter().rect_filled(
            egui::Rect::from_min_size(row.rect.min, egui::vec2(2.0, row.rect.height())),
            0,
            crate::app::theme::tab_selected_accent(ui),
        );
    }
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.add_space(10.0);
        ui.label(egui::RichText::new(OVERFLOW_KEY_HINTS).size(11.0).weak());
    });
}
