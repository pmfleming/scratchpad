use crate::app::theme::text_primary;
use crate::app::ui::widget_ids;
use eframe::egui;

const SCROLL_BLOCK_HOVER_SECONDS: f64 = 0.25;

pub(crate) fn apply_spacing(ui: &mut egui::Ui) {
    ui.spacing_mut().item_spacing = egui::vec2(8.0, 8.0);
}

pub(crate) fn mark_scroll_blocker_if_hovered(ctx: &egui::Context, response: &egui::Response) {
    if response.hovered() || response.contains_pointer() {
        let now = ctx.input(|input| input.time);
        ctx.data_mut(|data| data.insert_temp(scroll_blocker_id(), now));
    }
}

pub(crate) fn scroll_blocker_hovered(ctx: &egui::Context) -> bool {
    let now = ctx.input(|input| input.time);
    ctx.data(|data| {
        data.get_temp::<f64>(scroll_blocker_id())
            .is_some_and(|last_hovered| now - last_hovered <= SCROLL_BLOCK_HOVER_SECONDS)
    })
}

pub(crate) fn set_modal_scroll_blocker_active(ctx: &egui::Context, active: bool) {
    ctx.data_mut(|data| {
        if active {
            data.insert_temp(modal_scroll_blocker_id(), true);
        } else {
            data.remove::<bool>(modal_scroll_blocker_id());
        }
    });
}

pub(crate) fn scroll_blocker_active(ctx: &egui::Context) -> bool {
    scroll_blocker_hovered(ctx)
        || ctx.data(|data| data.get_temp::<bool>(modal_scroll_blocker_id()) == Some(true))
}

fn scroll_blocker_id() -> egui::Id {
    widget_ids::ctx_key("callout_scroll_blocker_hover")
}

fn modal_scroll_blocker_id() -> egui::Id {
    widget_ids::ctx_key("callout_scroll_blocker_modal")
}

pub(crate) fn text(ui: &egui::Ui) -> egui::Color32 {
    text_primary(ui)
}
