mod card;
mod track;
use super::{active_buffer, navigate, style};
use crate::app::{
    app_state::{ScratchpadApp, workspace::accessors},
    domain::{BufferState, buffer::HistoryToken},
    ui::{callout, widget_ids},
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
    let width = (screen.width() - 24.0).clamp(160.0, 480.0);
    let mut target = None;
    let mut open_history = false;
    widget_ids::area("history_scrubber_window")
        .accessible_name("Scrubber")
        .role(egui::Role::Dialog)
        .order(egui::Order::Foreground)
        .constrain(true)
        .movable(true)
        .default_size(egui::vec2(width, 104.0))
        .default_pos(egui::pos2(
            (screen.right() - width - 24.0).max(screen.left()),
            (screen.bottom() - 150.0).max(screen.top()),
        ))
        .show(ctx, |ui| {
            style::apply(ui);
            let frame = callout::frame(ui).corner_radius(egui::CornerRadius::ZERO);
            let inner = frame.show(ui, |ui| {
                ui.set_width((width - 24.0).max(1.0));
                ui.set_min_width((width - 24.0).max(1.0));
                ui.set_max_width((width - 24.0).max(1.0));
                let Some(tab) = app.tab_manager.active_tab() else {
                    return;
                };
                let buffer = tab.active_buffer();
                open_history = card::header(ui, buffer);
                target = transport(ui, buffer);
                card::caption(ui, buffer);
            });
            callout::mark_scroll_blocker_if_hovered(ctx, &inner.response);
        });
    if let Some(node) = target {
        apply_target(ctx, app, id, node);
    }
    if open_history {
        close(ctx, app);
        app.state.dialogs.text_history.open();
    }
}

fn transport(ui: &mut egui::Ui, buffer: &BufferState) -> Option<u64> {
    let ctx = ui.ctx().clone();
    let document = buffer.document();
    let route = document.history_route();
    let token = document.history_token();
    let mut drag = ctx.data(|d| d.get_temp::<Drag>(drag_key()));
    if let Some(drag) = &mut drag {
        drag.cancelled |= drag.buffer != buffer.id || drag.token != token;
    }
    let enabled = drag.as_ref().is_none_or(|d| !d.cancelled);
    let mut position = document.operation_undo_depth();
    let (response, stepped) = ui
        .add_enabled_ui(enabled, |ui| card::transport(ui, buffer, &mut position))
        .inner;
    if response.has_focus() {
        super::lock_navigation_focus(&ctx, focus_id());
    }
    if stepped {
        ctx.memory_mut(|m| m.request_focus(focus_id()));
    }
    let changed = (response.changed() || stepped) && enabled;
    let pointer_down = ctx.input(|i| i.pointer.any_down());
    if enabled
        && pointer_down
        && (response.is_pointer_button_down_on() || changed)
        && drag.is_none()
    {
        drag = Some(Drag {
            buffer: buffer.id,
            token,
            cancelled: false,
        });
    }
    ctx.data_mut(|d| {
        if pointer_down {
            if let Some(drag) = drag {
                d.insert_temp(drag_key(), drag);
            }
        } else {
            d.remove::<Drag>(drag_key());
        }
    });
    changed.then(|| position.checked_sub(1).map_or(0, |i| route[i]))
}

fn apply_target(ctx: &egui::Context, app: &mut ScratchpadApp, id: u64, node: u64) {
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
