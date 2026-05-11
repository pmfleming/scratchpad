use super::*;

const OVERFLOW_DRAG_HOVER_OPEN_DELAY_SECONDS: f64 = 0.4;

pub(super) fn maybe_open_overflow_popup_for_tab_drag(
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

pub(super) fn maybe_auto_scroll_overflow_popup(
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

pub(super) fn overflow_popup_scroll_id(overflow_popup_id: egui::Id) -> egui::Id {
    overflow_popup_id.with("scroll")
}

pub(super) fn overflow_drag_hover_ready(started_at: f64, now: f64) -> bool {
    now - started_at >= OVERFLOW_DRAG_HOVER_OPEN_DELAY_SECONDS
}

pub(super) fn overflow_drag_hover_remaining(started_at: f64, now: f64) -> Duration {
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
