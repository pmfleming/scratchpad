use eframe::egui::{self, Id, Rect, Response, Sense};
use std::hash::Hash;

const ID_NAMESPACE: &str = "scratchpad.widget";

pub(crate) fn configure_debug_options(ctx: &egui::Context) {
    ctx.options_mut(|options| options.warn_on_id_clash = cfg!(debug_assertions));
}

pub(crate) fn global(key: impl Hash) -> Id {
    Id::new((ID_NAMESPACE, key))
}

pub(crate) fn local(ui: &egui::Ui, key: impl Hash) -> Id {
    ui.make_persistent_id((ID_NAMESPACE, key))
}

pub(crate) fn child(id: Id, key: impl Hash) -> Id {
    id.with((ID_NAMESPACE, key))
}

pub(crate) fn scope<R>(
    ui: &mut egui::Ui,
    key: impl Hash,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    ui.push_id((ID_NAMESPACE, key), add_contents)
}

pub(crate) fn interact(
    ui: &egui::Ui,
    rect: Rect,
    id: Id,
    sense: Sense,
    kind: &'static str,
) -> Response {
    let response = ui.interact(rect, id, sense);
    track(ui.ctx(), id, response.rect, kind);
    response
}

pub(crate) fn track(ctx: &egui::Context, id: Id, rect: Rect, kind: &'static str) {
    #[cfg(debug_assertions)]
    ctx.check_for_id_clash(id, rect, kind);
}
