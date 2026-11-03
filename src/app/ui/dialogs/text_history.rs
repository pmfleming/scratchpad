//! A file's History and scrubber share the document's explicit Now and selected route.
mod panel;
mod recent;
mod scrubber;
#[cfg(test)]
mod tests;
mod tree_list;

use crate::app::{
    app_state::{
        ScratchpadApp,
        workspace::{accessors, mutation},
    },
    domain::buffer::TextHistoryEvent,
    platform::{PlatformProfile, resolved_profile},
};
use eframe::egui;
pub(crate) use scrubber::history_scrubbing;

#[derive(Clone, Default)]
struct HistoryUi {
    opened: bool,
    all_files: bool,
    selected: Option<u64>,
    search: String,
    forget: Option<u64>,
}
fn state_key() -> egui::Id {
    egui::Id::unique("history.tree.ui")
}
fn focus_id() -> egui::Id {
    egui::Id::unique("history.tree.focus")
}
fn rect_key() -> egui::Id {
    egui::Id::unique("history.tree.rect")
}
fn active_buffer(app: &ScratchpadApp) -> Option<u64> {
    app.tab_manager
        .active_tab()
        .map(|tab| tab.active_buffer().id)
}
fn dismissible_history(app: &ScratchpadApp) -> bool {
    matches!(
        resolved_profile(app.state.app_settings.platform_profile()),
        PlatformProfile::LinuxGeneric | PlatformProfile::Hyprland
    )
}
fn lock_navigation_focus(ctx: &egui::Context, id: egui::Id) {
    ctx.memory_mut(|m| {
        m.set_focus_lock_filter(
            id,
            egui::EventFilter {
                horizontal_arrows: true,
                vertical_arrows: true,
                tab: false,
                escape: true,
            },
        );
    });
}
fn close(ctx: &egui::Context, app: &mut ScratchpadApp) {
    app.state.dialogs.text_history.close();
    ctx.data_mut(|d| d.get_temp_mut_or_default::<HistoryUi>(state_key()).opened = false);
    accessors::request_focus_for_active_view(app);
}

fn navigate(ctx: &egui::Context, app: &mut ScratchpadApp, buffer: u64, node: u64) -> bool {
    let focused = ctx.memory(|m| m.has_focus(focus_id()) || m.has_focus(scrubber::focus_id()));
    let result = mutation::navigate_text_history(app, buffer, node, true);
    if focused
        && let Some(view) = app
            .tab_manager
            .active_tab()
            .and_then(|t| t.layout.active_view())
            .map(|v| v.id)
    {
        accessors::consume_focus_request(app, view);
    }
    if result && let Some(tab) = app.tab_manager.active_tab() {
        let buffer = tab.active_buffer();
        let offset = tab
            .layout
            .active_view()
            .and_then(|view| view.cursor_range)
            .map_or(0, |selection| selection.primary.index);
        crate::app::ui::editor_content::native_editor::history_feedback::mark(ctx, buffer, offset);
    }
    ctx.request_repaint();
    result
}

pub(crate) fn handle_text_history_navigation(ctx: &egui::Context, app: &mut ScratchpadApp) {
    if app.state.dialogs.any_modal_open()
        || egui::Popup::is_any_open(ctx)
        || accessors::pending_action(app).is_some()
    {
        return;
    }
    let rail = app.state.dialogs.text_history.is_open() && ctx.memory(|m| m.has_focus(focus_id()));
    let scrub = app.state.dialogs.history_scrubber.is_open()
        && ctx.memory(|m| m.has_focus(scrubber::focus_id()));
    if !rail && !scrub {
        return;
    }
    if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
        if rail {
            close(ctx, app);
        } else {
            scrubber::close(ctx, app);
        }
        return;
    }
    if resume_editor_input(ctx, app) {
        return;
    }
    let selected = ctx
        .data(|d| d.get_temp::<HistoryUi>(state_key()))
        .and_then(|s| s.selected);
    let id = if rail {
        selected.or_else(|| active_buffer(app))
    } else {
        active_buffer(app)
    };
    let Some(id) = id else {
        return;
    };
    let Some(buffer) = app
        .tab_manager
        .tabs
        .as_slice()
        .iter()
        .find_map(|tab| tab.buffer_by_id(id))
    else {
        return;
    };
    let document = buffer.document();
    let mut position = document.operation_undo_depth();
    let route = document.history_route();
    let mut changed = false;
    ctx.input_mut(|i| {
        for (key, target) in [
            (egui::Key::ArrowLeft, position.saturating_sub(1)),
            (egui::Key::ArrowDown, position.saturating_sub(1)),
            (egui::Key::ArrowRight, (position + 1).min(route.len())),
            (egui::Key::ArrowUp, (position + 1).min(route.len())),
            (egui::Key::Home, 0),
            (egui::Key::End, route.len()),
        ] {
            if i.consume_key(egui::Modifiers::NONE, key) {
                position = target;
                changed = true;
            }
        }
    });
    let target = position.checked_sub(1).map_or(0, |i| route[i]);
    if changed {
        navigate(ctx, app, id, target);
    }
}

pub(crate) fn dismiss_text_history(ctx: &egui::Context, app: &mut ScratchpadApp) {
    if app.state.dialogs.any_modal_open()
        || egui::Popup::is_any_open(ctx)
        || accessors::pending_action(app).is_some()
    {
        return;
    }
    if (app.state.dialogs.text_history.is_open() || app.state.dialogs.history_scrubber.is_open())
        && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
    {
        if app.state.dialogs.text_history.is_open() {
            close(ctx, app);
        } else {
            scrubber::close(ctx, app);
        }
    }
}

pub(crate) fn show_text_history_window(ctx: &egui::Context, app: &mut ScratchpadApp) {
    let active = active_buffer(app);
    for buffer in app
        .tab_manager
        .tabs
        .as_mut_slice()
        .iter_mut()
        .flat_map(|tab| tab.buffers_mut())
    {
        let event = buffer.take_text_history_event();
        if Some(buffer.id) != active {
            continue;
        }
        match event {
            Some(TextHistoryEvent::Replay) if !app.state.dialogs.text_history.is_open() => {
                app.state.dialogs.history_scrubber.open();
            }
            Some(TextHistoryEvent::Edit) => app.state.dialogs.history_scrubber.close(),
            _ => {}
        }
    }
    if app.state.dialogs.text_history.is_open() {
        show_history(ctx, app);
    } else {
        ctx.data_mut(|d| d.get_temp_mut_or_default::<HistoryUi>(state_key()).opened = false);
    }
    if !app.state.dialogs.text_history.is_open() {
        scrubber::show(ctx, app);
    }
}

fn show_history(ctx: &egui::Context, app: &mut ScratchpadApp) {
    let mut state = ctx.data_mut(|d| d.get_temp_mut_or_default::<HistoryUi>(state_key()).clone());
    let old_selected = state.selected;
    let first = !state.opened;
    state.opened = true;
    if first
        || !app.tab_manager.tabs.as_slice().iter().any(|tab| {
            state
                .selected
                .is_some_and(|id| tab.buffer_by_id(id).is_some())
        })
    {
        state.selected = active_buffer(app);
    }
    let mut action = None;
    let mut forgotten = None;
    let mut reopen = None;
    let mut focus = false;
    let should_close = panel::show(ctx, app, |ui, app| {
        ui.horizontal(|ui| {
            ui.selectable_value(&mut state.all_files, false, "Selected file");
            ui.selectable_value(&mut state.all_files, true, "All files");
        });
        ui.add(
            egui::TextEdit::singleline(&mut state.search)
                .hint_text("Find recently edited files")
                .id_salt("history.file_search"),
        );
        reopen = recent::show(ui, app, &mut state);
        ui.separator();
        let mut files: Vec<_> = app
            .tab_manager
            .tabs
            .as_slice()
            .iter()
            .flat_map(|t| t.buffers())
            .filter(|b| state.all_files || state.selected == Some(b.id))
            .collect();
        files.sort_by_key(|b| std::cmp::Reverse(b.document().last_edit()));
        for buffer in files {
            if state.all_files && !recent::matches(buffer, &state.search) {
                continue;
            }
            let label = crate::app::text_history::buffer_label(buffer);
            egui::CollapsingHeader::new(label).id_salt(("history.file", buffer.id))
                .default_open(state.selected == Some(buffer.id)).show(ui, |ui| {
                    if let Some(node) = tree_list::show(ui, buffer) {
                        state.selected = Some(buffer.id); action = Some((buffer.id, node)); focus = true;
                    }
                    if ui.small_button("Forget history…").clicked() { state.forget = Some(buffer.id); }
                    if state.forget == Some(buffer.id) {
                        ui.label("Forget every branch for this file? Current text is kept. This cannot be undone.");
                        ui.horizontal(|ui| {
                            if ui.button("Cancel").clicked() { state.forget = None; }
                            if ui.button("Forget this file's history").clicked() { forgotten = Some(buffer.id); state.forget = None; }
                        });
                    }
                });
        }
        history_focus(ui, ctx, app, focus, first);
    });
    if action.is_none() && old_selected != state.selected && !first {
        action = state.selected.and_then(|id| {
            app.tab_manager
                .tabs
                .as_slice()
                .iter()
                .find_map(|t| t.buffer_by_id(id))
                .map(|b| (id, b.document().history_now()))
        });
    }
    ctx.data_mut(|d| d.insert_temp(state_key(), state));
    if let Some((buffer, node)) = action {
        navigate(ctx, app, buffer, node);
    }
    if let Some(id) = forgotten {
        mutation::clear_text_history(app, &[id]);
    }
    if let Some(token) = reopen {
        crate::app::app_state::workspace::lifecycle::reopen_archived_tab(app, &token);
        ctx.data_mut(|d| {
            d.get_temp_mut_or_default::<HistoryUi>(state_key()).selected = active_buffer(app);
        });
    }
    if should_close {
        close(ctx, app);
    } else if ctx.memory(|m| m.has_focus(focus_id()))
        && let Some(view) = app
            .tab_manager
            .active_tab()
            .and_then(|t| t.layout.active_view())
            .map(|v| v.id)
    {
        accessors::consume_focus_request(app, view);
    }
}

fn history_focus(
    ui: &egui::Ui,
    ctx: &egui::Context,
    app: &ScratchpadApp,
    focus: bool,
    first: bool,
) {
    let response = crate::app::ui::widget_ids::interact(
        ui,
        ui.min_rect(),
        focus_id(),
        egui::Sense::focusable_noninteractive(),
        "history-focus",
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::ListBox, true, "File history"));
    if focus || (first && !app.state.search_state.open() && !app.state.dialogs.any_modal_open()) {
        response.request_focus();
    }
    if response.has_focus() {
        lock_navigation_focus(ctx, focus_id());
    }
}

fn resume_editor_input(ctx: &egui::Context, app: &mut ScratchpadApp) -> bool {
    let resume = ctx.input(|i| {
        i.events.iter().any(|event| match event {
            egui::Event::Text(text)
            | egui::Event::Paste(text)
            | egui::Event::Ime(
                egui::ImeEvent::Preedit { text, .. } | egui::ImeEvent::Commit(text),
            ) => !text.is_empty(),
            egui::Event::Cut => true,
            egui::Event::Key {
                key,
                modifiers,
                pressed: true,
                ..
            } => {
                matches!(
                    key,
                    egui::Key::Backspace | egui::Key::Delete | egui::Key::Enter
                ) || (matches!(key, egui::Key::Z | egui::Key::Y) && modifiers.command)
            }
            _ => false,
        })
    });
    if resume {
        ctx.memory_mut(|m| {
            m.surrender_focus(focus_id());
            m.surrender_focus(scrubber::focus_id());
        });
        accessors::request_focus_for_active_view(app);
    }
    resume
}
