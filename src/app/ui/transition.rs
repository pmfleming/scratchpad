use crate::app::ui::tab_drag;
use crate::app::ui::widget_ids;
use eframe::egui;

fn chrome_transition_id() -> egui::Id {
    widget_ids::global("chrome_transition_active")
}

pub(crate) fn set_chrome_transition_active(ctx: &egui::Context, active: bool) {
    ctx.data_mut(|data| data.insert_temp(chrome_transition_id(), active));
}

pub(crate) fn chrome_transition_active(ctx: &egui::Context) -> bool {
    ctx.data(|data| {
        data.get_temp::<bool>(chrome_transition_id())
            .unwrap_or(false)
    })
}

pub(crate) fn suppress_interactive_chrome(ctx: &egui::Context) -> bool {
    tab_drag::is_drag_active_for_context(ctx) || chrome_transition_active(ctx)
}
