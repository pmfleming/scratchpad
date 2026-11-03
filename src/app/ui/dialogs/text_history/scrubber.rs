use super::{active_buffer, navigate};
use crate::app::{
    app_state::{ScratchpadApp, workspace::accessors},
    domain::buffer::HistoryToken,
    ui::callout,
};
use eframe::egui;

#[derive(Clone)]
struct Drag {
    buffer: u64,
    token: HistoryToken,
    cancelled: bool,
}
fn drag_key() -> egui::Id {
    egui::Id::unique("history.tree.drag")
}
pub(super) fn focus_id() -> egui::Id {
    egui::Id::unique("history.tree.scrubber")
}
pub(super) fn close(ctx: &egui::Context, app: &mut ScratchpadApp) {
    app.state.dialogs.history_scrubber.close();
    ctx.data_mut(|d| d.remove::<Drag>(drag_key()));
    accessors::request_focus_for_active_view(app);
}
pub(crate) fn history_scrubbing(ctx: &egui::Context, app: &ScratchpadApp) -> bool {
    app.state.dialogs.history_scrubber.is_open()
        && ctx.input(|i| i.pointer.any_down())
        && ctx
            .data(|d| d.get_temp::<Drag>(drag_key()))
            .is_some_and(|drag| {
                !drag.cancelled
                    && app.tab_manager.active_tab().is_some_and(|t| {
                        t.active_buffer().id == drag.buffer
                            && t.active_buffer().document().history_token() == drag.token
                    })
            })
}
pub(super) fn show(ctx: &egui::Context, app: &mut ScratchpadApp) {
    if !app.state.dialogs.history_scrubber.is_open() {
        ctx.data_mut(|d| d.remove::<Drag>(drag_key()));
        return;
    }
    let Some(id) = active_buffer(app) else {
        return;
    };
    let screen = ctx.content_rect();
    let mut target = None;
    callout::show_floating(
        ctx,
        "history_scrubber_window",
        "Scrubber",
        egui::pos2(
            (screen.right() - 510.0).max(0.0),
            (screen.bottom() - 150.0).max(0.0),
        ),
        480.0,
        |ui| {
            let buffer = app.tab_manager.active_tab().unwrap().active_buffer();
            let document = buffer.document();
            let route = document.history_route();
            ui.label(&buffer.name);
            ui.label(format!(
                "Now · {} of {} changes",
                document.operation_undo_depth(),
                route.len()
            ));
            let mut position = document.operation_undo_depth();
            let token = document.history_token();
            let mut drag = ctx.data(|d| d.get_temp::<Drag>(drag_key()));
            if let Some(drag) = &mut drag {
                drag.cancelled |= drag.buffer != id || drag.token != token;
            }
            let enabled = drag.as_ref().is_none_or(|d| !d.cancelled);
            let response = ui.add_enabled(
                enabled,
                egui::Slider::new(&mut position, 0..=route.len())
                    .text("Now")
                    .show_value(false),
            );
            let focus = ui.interact(
                response.rect,
                focus_id(),
                egui::Sense::focusable_noninteractive(),
            );
            if response.drag_started() || response.clicked() {
                focus.request_focus();
            }
            if focus.has_focus() {
                super::lock_navigation_focus(ctx, focus_id());
            }
            if response.changed() && enabled {
                target = Some(position.checked_sub(1).map_or(0, |i| route[i]));
                if ctx.input(|i| i.pointer.any_down()) {
                    drag = Some(Drag {
                        buffer: id,
                        token,
                        cancelled: false,
                    });
                }
            }
            let pointer_down = ctx.input(|i| i.pointer.any_down());
            ctx.data_mut(|d| {
                if pointer_down {
                    if let Some(drag) = drag {
                        d.insert_temp(drag_key(), drag);
                    }
                } else {
                    d.remove::<Drag>(drag_key());
                }
            });
            let label = document
                .history_entries()
                .iter()
                .find(|e| e.id == document.history_now())
                .map_or("Empty document", |e| e.summary.as_str());
            ui.label(label);
        },
    );
    if let Some(node) = target {
        let applied = navigate(ctx, app, id, node);
        if let Some(buffer) = app.tab_manager.active_tab().map(|t| t.active_buffer()) {
            ctx.data_mut(|d| {
                if let Some(mut drag) = d.get_temp::<Drag>(drag_key()) {
                    if applied && drag.buffer == buffer.id {
                        drag.token = buffer.document().history_token();
                    } else {
                        drag.cancelled = true;
                    }
                    d.insert_temp(drag_key(), drag);
                }
            });
        }
    }
}
