//! Execute an inclusive native replay target after the shared state model validates it.
use super::{
    active_buffer, list, model::TextHistoryAction, rail, request_clear, scope, scope_buffer,
    scrubber, snapshots, states::States,
};
use crate::app::{
    app_state::{ScratchpadApp, workspace::mutation},
    ui::editor_content::native_editor::history_feedback,
};
use eframe::egui;

#[derive(Clone, Copy)]
enum Surface {
    Rail,
    Scrubber,
}

impl Surface {
    fn focused(ctx: &egui::Context, app: &ScratchpadApp) -> Option<Self> {
        [
            (
                Self::Rail,
                app.state.dialogs.text_history.is_open(),
                list::focus_id(),
            ),
            (
                Self::Scrubber,
                app.state.dialogs.history_scrubber.is_open(),
                scrubber::focus_id(),
            ),
        ]
        .into_iter()
        .find_map(|(surface, open, id)| {
            (open && ctx.memory(|m| m.has_focus(id))).then_some(surface)
        })
    }

    fn close(self, ctx: &egui::Context, app: &mut ScratchpadApp) {
        match self {
            Self::Rail => rail::close(ctx, app),
            Self::Scrubber => scrubber::close(ctx, app),
        }
    }

    fn navigate(self, ctx: &egui::Context, app: &mut ScratchpadApp) {
        let id = match self {
            Self::Rail => scope_buffer(scope(ctx, app), app).or_else(|| list::selected_buffer(ctx)),
            Self::Scrubber => {
                active_buffer(app).filter(|&id| !snapshots::is_browsing(ctx, app, id))
            }
        };
        let Some(id) = id else {
            return;
        };
        let entries = snapshots::state_entries(ctx, app);
        let states = States::new(&entries, id);
        let position = match self {
            Self::Rail => states.position,
            Self::Scrubber => states.position,
        };
        let Some(position) = self.destination(ctx, &states, position) else {
            return;
        };
        match self {
            Self::Rail => {
                rail::select(ctx, app, id, states.after(position));
                list::request_reveal(ctx);
            }
            Self::Scrubber => {
                snapshots::go_to_state(ctx, app, id, states.after(position));
            }
        }
    }

    fn destination(
        self,
        ctx: &egui::Context,
        states: &States<'_>,
        position: usize,
    ) -> Option<usize> {
        use egui::Key::{ArrowDown, ArrowLeft, ArrowRight, ArrowUp, End, Home};
        let (older, newer, home, end) = match self {
            Self::Rail => (ArrowDown, ArrowUp, states.last, states.first),
            Self::Scrubber => (ArrowLeft, ArrowRight, states.first, states.last),
        };
        // Consume every matching key; later keys retain the existing priority.
        [
            (older, position.saturating_sub(1).max(states.first)),
            (newer, (position + 1).min(states.last)),
            (Home, home),
            (End, end),
        ]
        .into_iter()
        .fold(None, |previous, (key, position)| {
            if consume(ctx, egui::Modifiers::NONE, key) {
                Some(position)
            } else {
                previous
            }
        })
    }
}

fn consume(ctx: &egui::Context, modifiers: egui::Modifiers, key: egui::Key) -> bool {
    ctx.input_mut(|input| input.consume_key(modifiers, key))
}

fn blocked(app: &ScratchpadApp) -> bool {
    app.state.dialogs.any_modal_open()
        || crate::app::app_state::workspace::accessors::pending_action(app).is_some()
}

fn escape_popup_key() -> egui::Id {
    egui::Id::unique("history.escape.popup")
}

/// Only the focused rail or scrubber owns navigation. History focuses its rail
/// on opening; the automatic scrubber leaves focus with the editor.
pub(crate) fn handle_text_history_navigation(ctx: &egui::Context, app: &mut ScratchpadApp) {
    rail::prepare(ctx, app);
    let popup_open = egui::Popup::is_any_open(ctx);
    ctx.data_mut(|d| d.insert_temp(escape_popup_key(), popup_open));
    if blocked(app) || popup_open {
        return;
    }
    let Some(surface) = Surface::focused(ctx, app) else {
        return;
    };
    if consume(ctx, egui::Modifiers::NONE, egui::Key::Escape) {
        surface.close(ctx, app);
        return;
    }
    match surface {
        Surface::Scrubber if scrubber::resume_editor_input(ctx, app) => return,
        Surface::Rail
            if consume(
                ctx,
                egui::Modifiers::CTRL | egui::Modifiers::SHIFT,
                egui::Key::Delete,
            ) =>
        {
            request_clear(ctx, app, scope(ctx, app));
            return;
        }
        _ => {}
    }
    surface.navigate(ctx, app);
}

/// Unfocused surfaces dismiss only after widgets and shortcuts can consume Escape.
pub(crate) fn dismiss_text_history(ctx: &egui::Context, app: &mut ScratchpadApp) {
    let surface = if app.state.dialogs.history_scrubber.is_open() {
        Surface::Scrubber
    } else if app.state.dialogs.text_history.is_open() {
        Surface::Rail
    } else {
        return;
    };
    if !blocked(app)
        && !egui::Popup::is_any_open(ctx)
        && !ctx.data(|d| d.get_temp::<bool>(escape_popup_key()).unwrap_or(false))
        && consume(ctx, egui::Modifiers::NONE, egui::Key::Escape)
    {
        surface.close(ctx, app);
    }
}

pub(super) fn activate_file(app: &mut ScratchpadApp, buffer_id: u64) -> bool {
    let target = app
        .tab_manager
        .tabs
        .as_slice()
        .iter()
        .enumerate()
        .find_map(|(index, tab)| {
            tab.layout
                .active_view()
                .filter(|view| view.buffer_id == buffer_id)
                .or_else(|| {
                    tab.layout
                        .views
                        .iter()
                        .find(|view| view.buffer_id == buffer_id)
                })
                .map(|view| (index, view.id))
        });
    let Some((index, view_id)) = target else {
        return false;
    };
    crate::app::app_state::settings_controller::activate_workspace_surface(app);
    crate::app::commands::activate_pending_view_command(app, index, view_id)
}

pub(super) fn apply(
    ctx: &egui::Context,
    app: &mut ScratchpadApp,
    action: TextHistoryAction,
) -> bool {
    let offset = app
        .tab_manager
        .tabs
        .as_slice()
        .iter()
        .find_map(|tab| tab.buffer_by_id(action.buffer_id))
        .and_then(|buffer| {
            buffer
                .document()
                .history_entries()
                .iter()
                .find(|entry| entry.id == action.entry_id)
        })
        .and_then(|entry| {
            entry
                .edits
                .iter()
                .map(|edit| edit.start_char() as usize)
                .min()
        });
    if !mutation::apply_text_history_to_entry(
        app,
        action.buffer_id,
        action.entry_id,
        super::read_follow_focus(ctx),
    ) {
        return false;
    }
    if let Some(buffer) = app
        .tab_manager
        .tabs
        .as_slice()
        .iter()
        .find_map(|tab| tab.buffer_by_id(action.buffer_id))
        && let Some(offset) = offset
    {
        history_feedback::mark(ctx, buffer, offset);
    }
    retain_tool_focus(ctx, app);
    ctx.request_repaint();
    true
}

pub(super) fn retain_tool_focus(ctx: &egui::Context, app: &mut ScratchpadApp) {
    if ctx.memory(|m| {
        m.has_focus(super::list::focus_id()) || m.has_focus(super::scrubber::focus_id())
    }) && let Some(view) = app
        .tab_manager
        .active_tab()
        .and_then(|tab| tab.layout.active_view())
        .map(|view| view.id)
    {
        crate::app::app_state::workspace::accessors::consume_focus_request(app, view);
    }
}

#[cfg(test)]
mod tests;
