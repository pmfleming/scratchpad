//! Rail lifetime and opening markers. All navigation mutates the live buffer.
use super::{model::HistoryAction, states::States};
use crate::app::{
    app_state::{ScratchpadApp, workspace::accessors},
    domain::buffer::HistoryToken,
};
use eframe::egui;
use std::collections::HashMap;

#[derive(Clone)]
struct Marker {
    revision: HistoryToken,
    after: Option<u64>,
}
#[derive(Clone, Default)]
struct Rail {
    markers: HashMap<u64, Marker>,
}
fn key() -> egui::Id {
    egui::Id::unique("history.rail.session")
}

pub(super) fn hover_changed(ctx: &egui::Context, target: HistoryAction) -> bool {
    let key = egui::Id::unique("history.rail.hover_target");
    let moved = ctx.input(|i| {
        i.events
            .iter()
            .any(|e| matches!(e, egui::Event::PointerMoved(_)))
    });
    ctx.data_mut(|data| {
        let changed = data.get_temp::<HistoryAction>(key) != Some(target);
        data.insert_temp(key, target);
        changed || moved
    })
}
pub(super) fn rect_key() -> egui::Id {
    egui::Id::unique("history.rail.rect")
}

pub(super) fn begin(ctx: &egui::Context, app: &ScratchpadApp) -> bool {
    let first = ctx.data(|d| d.get_temp::<Rail>(key())).is_none();
    let mut changed = false;
    ctx.data_mut(|d| {
        let rail = d.get_temp_mut_or_default::<Rail>(key());
        for buffer in app
            .tab_manager
            .tabs
            .as_slice()
            .iter()
            .flat_map(|tab| tab.buffers())
        {
            let revision = buffer.document().history_token();
            if rail
                .markers
                .get(&buffer.id)
                .is_none_or(|m| m.revision != revision)
            {
                changed = true;
                let after = buffer
                    .document()
                    .history_entries()
                    .iter()
                    .rev()
                    .find(|e| !e.is_undone())
                    .map(|e| e.id);
                rail.markers.insert(buffer.id, Marker { revision, after });
            }
        }
    });
    if changed && !first {
        super::list::request_reveal(ctx);
    }
    first
}
pub(super) fn starting_position(ctx: &egui::Context, id: u64, states: &States<'_>) -> usize {
    ctx.data(|d| d.get_temp::<Rail>(key()))
        .and_then(|rail| rail.markers.get(&id).and_then(|m| states.index(m.after)))
        .unwrap_or(states.position)
}
pub(super) fn clear(ctx: &egui::Context) {
    ctx.data_mut(|d| d.remove::<Rail>(key()));
    super::snapshots::cancel_pending(ctx);
    super::list::request_reveal(ctx);
    ctx.request_repaint();
}
pub(super) fn close(ctx: &egui::Context, app: &mut ScratchpadApp) {
    clear(ctx);
    crate::app::commands::close_text_history(&mut app.state.dialogs);
    accessors::request_focus_for_active_view(app);
}
pub(super) fn prepare(ctx: &egui::Context, app: &mut ScratchpadApp) {
    if !app.state.dialogs.text_history.is_open() {
        ctx.data_mut(|d| d.remove::<Rail>(key()));
        return;
    }
    if app.state.dialogs.any_modal_open()
        || egui::Popup::is_any_open(ctx)
        || accessors::pending_action(app).is_some()
    {
        return;
    }
    let outside = ctx
        .data(|d| d.get_temp::<egui::Rect>(rect_key()))
        .is_some_and(|rect| {
            ctx.input(|i| {
                i.pointer.any_pressed()
                    && i.pointer.interact_pos().is_some_and(|p| !rect.contains(p))
            })
        });
    if outside {
        close(ctx, app);
    }
}
pub(super) fn select(
    ctx: &egui::Context,
    app: &mut ScratchpadApp,
    id: u64,
    after: Option<u64>,
) -> bool {
    begin(ctx, app);
    if !super::snapshots::go_to_state(ctx, app, id, after) {
        return false;
    }
    if let Some(buffer) = app
        .tab_manager
        .tabs
        .as_slice()
        .iter()
        .find_map(|tab| tab.buffer_by_id(id))
    {
        ctx.data_mut(|d| {
            if let Some(marker) = d
                .get_temp_mut_or_default::<Rail>(key())
                .markers
                .get_mut(&id)
            {
                marker.revision = buffer.document().history_token();
            }
        });
    }
    true
}
pub(super) fn execute(ctx: &egui::Context, app: &mut ScratchpadApp, action: HistoryAction) {
    match action {
        HistoryAction::State { buffer_id, after } => {
            select(ctx, app, buffer_id, after);
        }
        other => super::snapshots::execute(ctx, app, other),
    }
}
