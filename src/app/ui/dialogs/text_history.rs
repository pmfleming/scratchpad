mod list;
mod model;
mod navigation;
mod persistence;
mod scrubber;
mod snapshots;
mod states;

use self::model::{HistoryAction, TextHistoryFileGroup, file_groups_from_entries};
use self::persistence::{read_active_tab, read_follow_focus, write_active_tab, write_follow_focus};
use crate::app::app_state::{ScratchpadApp, workspace::mutation as workspace_mutation};
use crate::app::theme::{action_bg, border, tab_selected_accent, tab_selected_bg};
use crate::app::ui::{callout, settings, widget_ids};
use eframe::egui;
use egui_phosphor::regular::{ARROW_U_UP_LEFT, DOTS_THREE, FILE_TEXT, FILES, FOLDER_OPEN};

pub(super) const TEXT_HISTORY_SIZE: egui::Vec2 = egui::vec2(
    crate::app::ui::search_replace::SEARCH_DIALOG_WIDTH - 20.0,
    520.0,
);
pub(super) const TEXT_HISTORY_LIST_MIN_HEIGHT: f32 = 330.0;
pub(super) const HISTORY_PILL_CORNER_RADIUS: u8 = 8;
pub(super) const HISTORY_PILL_INNER_MARGIN: i8 = 10;
pub(super) const HISTORY_PILL_SPACING: f32 = 6.0;
const UNDONE_OPACITY: f32 = 0.65;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HistoryTab {
    CurrentFile,
    AllFiles,
    File(u64),
}

fn scope_key() -> egui::Id {
    egui::Id::unique("history.scope.file")
}
fn confirm_key() -> egui::Id {
    egui::Id::unique("history.clear.confirm")
}

fn scope(ctx: &egui::Context, app: &ScratchpadApp) -> HistoryTab {
    let picked = ctx.data(|data| data.get_temp::<u64>(scope_key()));
    if let Some(id) = picked {
        if app
            .tab_manager
            .tabs
            .as_slice()
            .iter()
            .any(|tab| tab.buffer_by_id(id).is_some())
        {
            return HistoryTab::File(id);
        }
        ctx.data_mut(|data| data.remove::<u64>(scope_key()));
    }
    read_active_tab(ctx)
}

fn active_buffer(app: &ScratchpadApp) -> Option<u64> {
    app.tab_manager
        .active_tab()
        .map(|tab| tab.active_buffer().id)
}

fn scope_buffer(scope: HistoryTab, app: &ScratchpadApp) -> Option<u64> {
    match scope {
        HistoryTab::CurrentFile => active_buffer(app),
        HistoryTab::File(id) => Some(id),
        HistoryTab::AllFiles => None,
    }
}

/// Only the focused rail or scrubber owns navigation; opening a callout never
/// steals plain arrow keys from the editor.
pub(crate) fn handle_text_history_navigation(ctx: &egui::Context, app: &mut ScratchpadApp) {
    if app.state.dialogs.any_modal_open()
        || crate::app::app_state::workspace::accessors::pending_action(app).is_some()
    {
        return;
    }
    let rail =
        app.state.dialogs.text_history.is_open() && ctx.memory(|m| m.has_focus(list::focus_id()));
    let scrub = app.state.dialogs.history_scrubber.is_open()
        && ctx.memory(|m| m.has_focus(scrubber::focus_id()));
    if !rail && !scrub {
        return;
    }
    if rail
        && ctx.input_mut(|i| {
            i.consume_key(
                egui::Modifiers::CTRL | egui::Modifiers::SHIFT,
                egui::Key::Delete,
            )
        })
    {
        ctx.data_mut(|d| d.insert_temp(confirm_key(), true));
        return;
    }
    if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
        if scrub {
            app.state.dialogs.history_scrubber.close();
        } else {
            app.state.dialogs.text_history.close();
        }
        return;
    }
    let id = if scrub {
        active_buffer(app)
    } else {
        scope_buffer(scope(ctx, app), app)
    };
    let Some(id) = id else {
        return;
    };
    if snapshots::is_browsing(ctx, app, id) {
        return;
    }
    let entries = snapshots::state_entries(ctx, app);
    let states = states::States::new(&entries, id);
    let (older, newer) = if scrub {
        (egui::Key::ArrowLeft, egui::Key::ArrowRight)
    } else {
        (egui::Key::ArrowDown, egui::Key::ArrowUp)
    };
    let mut destination = None;
    for (key, position) in [
        (older, states.position.saturating_sub(1).max(states.first)),
        (newer, (states.position + 1).min(states.last)),
        (
            egui::Key::Home,
            if scrub { states.first } else { states.last },
        ),
        (
            egui::Key::End,
            if scrub { states.last } else { states.first },
        ),
    ] {
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, key)) {
            destination = Some(position);
        }
    }
    if let Some(position) = destination {
        snapshots::go_to_state(ctx, app, id, states.after(position));
    }
}

pub(crate) fn show_text_history_window(ctx: &egui::Context, app: &mut ScratchpadApp) {
    snapshots::cleanup(ctx, app);
    ctx.data_mut(|data| data.remove::<String>(snapshots::status_key()));
    if app.state.dialogs.text_history.is_open() {
        show_history(ctx, app);
    } else {
        ctx.data_mut(|data| data.remove::<bool>(confirm_key()));
    }
    scrubber::show(ctx, app);
    // The snapshot service belongs to both tools, not to either callout's lifetime.
    if !app.state.dialogs.text_history.is_open() && !app.state.dialogs.history_scrubber.is_open() {
        snapshots::cancel_pending(ctx);
        snapshots::cancel_coverage(ctx, app);
    }
}

fn show_history(ctx: &egui::Context, app: &mut ScratchpadApp) {
    let mut action = None;
    let mut close = false;
    let mut clear = false;
    let mut selected = scope(ctx, app);
    let screen = ctx.content_rect();
    callout::show_floating(
        ctx,
        "text_history_window",
        "History",
        egui::pos2(
            (screen.right() - TEXT_HISTORY_SIZE.x - 24.0).max(screen.left()),
            screen.top() + 72.0,
        ),
        TEXT_HISTORY_SIZE.x,
        |ui| {
            widget_ids::feature_scope(ui, "text_history_dialog", |ui| {
                settings::apply_dialog_typography(ui);
                callout::apply_spacing(ui);
                snapshots::poll(ui, app);
                snapshots::hydrate(ui, app);
                close = super::common::history_dialog_header(
                    ui,
                    "text_history.header",
                    "Close history",
                    "History",
                    18.0,
                );
                let entries = snapshots::state_entries(ctx, app);
                let groups = file_groups_from_entries(entries.iter());
                controls(ui, app, &groups, &mut selected);
                let target = scope_buffer(selected, app);
                let ids: Vec<_> = app
                    .tab_manager
                    .tabs
                    .as_slice()
                    .iter()
                    .flat_map(|tab| tab.buffers())
                    .filter(|buffer| target.is_none_or(|id| id == buffer.id))
                    .map(|b| b.id)
                    .collect();
                let catalog = snapshots::collect(ui, app, &ids);
                if let Some(id) = target {
                    file_caption(ui, app, id, &entries);
                }
                snapshot_returns(ui, app, &ids, &catalog, &mut action);
                let confirming =
                    ctx.data(|data| data.get_temp::<bool>(confirm_key()).unwrap_or(false));
                if confirming {
                    callout::section_frame(ui).show(ui, |ui| {
                        ui.label("Clear undo/redo history in all open files?");
                        ui.weak("This cannot be undone. Saved snapshots are kept.");
                        ui.horizontal(|ui| {
                            if ui.button("Cancel").clicked() {
                                ctx.data_mut(|d| d.remove::<bool>(confirm_key()));
                            }
                            if ui
                                .add(
                                    egui::Button::new(
                                        egui::RichText::new("Clear history")
                                            .color(egui::Color32::WHITE),
                                    )
                                    .fill(crate::app::theme::CLOSE_HOVER_BG),
                                )
                                .clicked()
                            {
                                clear = true;
                            }
                        });
                    });
                }
                list::render(ui, &entries, &catalog, target, &ids, &mut action);
                snapshots::footer(ui, &catalog);
            });
        },
    );
    if close {
        crate::app::commands::close_text_history(&mut app.state.dialogs);
    }
    if clear {
        ctx.data_mut(|d| d.remove::<bool>(confirm_key()));
        let saved = snapshots::clear_history(ctx, app);
        let live = workspace_mutation::clear_text_history(app);
        if saved && !live {
            app.tab_manager.mark_session_dirty();
        }
        action = None;
    }
    if let Some(action) = action {
        snapshots::execute(ctx, app, action);
    }
}

fn file_caption(
    ui: &mut egui::Ui,
    app: &ScratchpadApp,
    id: u64,
    entries: &[crate::app::text_history::TextHistoryEntryView],
) {
    let Some(buffer) = app
        .tab_manager
        .tabs
        .as_slice()
        .iter()
        .find_map(|tab| tab.buffer_by_id(id))
    else {
        return;
    };
    ui.horizontal(|ui| {
        truncated_label(
            ui,
            &buffer.name,
            ui.available_width() - 80.0,
            13.0,
            callout::muted_text(ui),
            egui::Sense::hover(),
        )
        .on_hover_text(
            buffer
                .path
                .as_ref()
                .map_or_else(|| buffer.name.clone(), |p| p.display().to_string()),
        );
        let states = states::States::new(entries, id);
        ui.weak(format!("{} / {}", states.position, states.entries.len()));
    });
}

fn snapshot_returns(
    ui: &mut egui::Ui,
    app: &ScratchpadApp,
    ids: &[u64],
    catalog: &snapshots::Catalog,
    action: &mut Option<HistoryAction>,
) {
    for &id in ids {
        if snapshots::is_browsing(ui.ctx(), app, id) {
            widget_ids::scope(ui, ("history.visit", id), |ui| {
                ui.horizontal(|ui| {
                    ui.weak(catalog.get(&id).map_or("Snapshot", |f| f.label.as_str()));
                    if control_icon_button(
                        ui,
                        "return",
                        ARROW_U_UP_LEFT,
                        "Return to latest",
                        false,
                        true,
                    )
                    .clicked()
                    {
                        *action = Some(HistoryAction::Latest { buffer_id: id });
                    }
                })
                .response
                .on_hover_text(
                    "Viewing a snapshot. Editing starts a new branch; the latest text is kept.",
                );
            });
        }
    }
}

fn controls(
    ui: &mut egui::Ui,
    app: &ScratchpadApp,
    groups: &[TextHistoryFileGroup],
    selected: &mut HistoryTab,
) {
    ui.horizontal(|ui| {
        for (id, icon, tip, tab) in [
            (
                "current",
                FILE_TEXT,
                "Current file",
                HistoryTab::CurrentFile,
            ),
            ("all", FILES, "All files", HistoryTab::AllFiles),
        ] {
            if control_icon_button(ui, id, icon, tip, *selected == tab, true).clicked() {
                *selected = tab;
                ui.ctx().data_mut(|data| data.remove::<u64>(scope_key()));
                write_active_tab(ui.ctx(), tab);
            }
        }
        let picker = control_icon_button(
            ui,
            "pick",
            FOLDER_OPEN,
            "Choose file",
            matches!(selected, HistoryTab::File(_)),
            true,
        );
        egui::Popup::menu(&picker).show(|ui| {
            for buffer in app
                .tab_manager
                .tabs
                .as_slice()
                .iter()
                .flat_map(|tab| tab.buffers())
            {
                let count = groups
                    .iter()
                    .find(|g| g.buffer_id == buffer.id)
                    .map_or(0, |g| g.rows.len());
                if ui
                    .selectable_label(
                        *selected == HistoryTab::File(buffer.id),
                        format!("{}  ·  {count}", buffer.name),
                    )
                    .on_hover_text(
                        buffer
                            .path
                            .as_ref()
                            .map_or_else(|| buffer.name.clone(), |p| p.display().to_string()),
                    )
                    .clicked()
                {
                    *selected = HistoryTab::File(buffer.id);
                    ui.ctx()
                        .data_mut(|data| data.insert_temp(scope_key(), buffer.id));
                    ui.close();
                }
            }
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let more = control_icon_button(ui, "more", DOTS_THREE, "History options", false, true);
            egui::Popup::menu(&more).show(|ui| {
                let mut follow = read_follow_focus(ui.ctx());
                if ui.checkbox(&mut follow, "Switch to changed file").changed() {
                    write_follow_focus(ui.ctx(), follow);
                }
                ui.separator();
                if ui.button("Clear history…").clicked() {
                    ui.ctx()
                        .data_mut(|data| data.insert_temp(confirm_key(), true));
                    ui.close();
                }
            });
        });
    });
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

fn control_icon_button(
    ui: &mut egui::Ui,
    id_source: &'static str,
    icon: &str,
    tooltip: &str,
    selected: bool,
    enabled: bool,
) -> egui::Response {
    let (fill, stroke_color, text_color) = if selected {
        (
            tab_selected_bg(ui),
            tab_selected_accent(ui),
            callout::text(ui),
        )
    } else {
        (action_bg(ui), border(ui), callout::muted_text(ui))
    };
    let button = egui::Button::new(
        egui::RichText::new(icon)
            .font(egui_phosphor::font_id(16.0))
            .color(text_color),
    )
    .min_size(egui::vec2(36.0, 30.0))
    .fill(fill)
    .stroke(egui::Stroke::new(1.0, stroke_color))
    .corner_radius(8);
    widget_ids::scope(ui, ("text_history.control", id_source), |ui| {
        ui.add_enabled(enabled, button)
    })
    .inner
    .accessible_name(tooltip)
    .on_hover_text(tooltip)
}

pub(super) fn dim_if(undone: bool, color: egui::Color32) -> egui::Color32 {
    if undone {
        color.gamma_multiply(UNDONE_OPACITY)
    } else {
        color
    }
}

pub(super) fn truncated_label(
    ui: &mut egui::Ui,
    text: &str,
    width: f32,
    size: f32,
    color: egui::Color32,
    sense: egui::Sense,
) -> egui::Response {
    ui.add_sized(
        egui::vec2(width.max(0.0), 0.0),
        egui::Label::new(egui::RichText::new(text).size(size).color(color))
            .truncate()
            .show_tooltip_when_elided(false)
            .sense(sense),
    )
}

#[cfg(test)]
mod tests;
