use super::common::truncated_label;
mod file_picker;
mod list;
mod model;
mod navigation;
pub(crate) use navigation::{dismiss_text_history, handle_text_history_navigation};
mod panel;
mod persistence;
mod rail;
mod scrubber;
mod snapshots;
mod states;
pub(crate) use scrubber::history_scrubbing;

use self::model::HistoryAction;
use self::persistence::{read_active_tab, read_follow_focus, write_active_tab, write_follow_focus};
use crate::app::app_state::{ScratchpadApp, workspace::mutation as workspace_mutation};
use crate::app::platform::{PlatformProfile, resolved_profile};
use crate::app::theme::{action_bg, border, tab_selected_accent, tab_selected_bg};
use crate::app::ui::{callout, widget_ids};
use eframe::egui;
use egui_phosphor::regular::{ARROW_U_UP_LEFT, FILE_TEXT, FILES, FOLDER_OPEN, TRASH};

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

#[derive(Clone)]
struct ClearRequest {
    buffer_ids: Vec<u64>,
    description: String,
}

fn request_clear(ctx: &egui::Context, app: &ScratchpadApp, selected: HistoryTab) {
    let target = scope_buffer(selected, app);
    if selected != HistoryTab::AllFiles && target.is_none() {
        return;
    }
    let buffers: Vec<_> = app
        .tab_manager
        .tabs
        .as_slice()
        .iter()
        .flat_map(|tab| tab.buffers())
        .filter(|buffer| target.is_none_or(|id| id == buffer.id))
        .collect();
    if buffers.is_empty() {
        return;
    }
    let description = if selected == HistoryTab::AllFiles {
        "Clear undo/redo history in all open files?".into()
    } else {
        format!("Clear undo/redo history in {}?", buffers[0].name)
    };
    let request = ClearRequest {
        buffer_ids: buffers.iter().map(|buffer| buffer.id).collect(),
        description,
    };
    ctx.data_mut(|data| data.insert_temp(confirm_key(), request));
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

fn dismissible_history(app: &ScratchpadApp) -> bool {
    matches!(
        resolved_profile(app.state.app_settings.platform_profile()),
        PlatformProfile::LinuxGeneric | PlatformProfile::Hyprland
    )
}

fn scope_buffer(scope: HistoryTab, app: &ScratchpadApp) -> Option<u64> {
    match scope {
        HistoryTab::CurrentFile => active_buffer(app),
        HistoryTab::File(id) => Some(id),
        HistoryTab::AllFiles => None,
    }
}

pub(crate) fn show_text_history_window(ctx: &egui::Context, app: &mut ScratchpadApp) {
    snapshots::cleanup(ctx, app);
    ctx.data_mut(|data| data.remove::<String>(snapshots::status_key()));
    if app.state.dialogs.text_history.is_open() {
        show_history(ctx, app);
    } else {
        ctx.data_mut(|data| data.remove::<ClearRequest>(confirm_key()));
    }
    scrubber::show(ctx, app);
    // The snapshot service belongs to both tools, not to either callout's lifetime.
    if !app.state.dialogs.text_history.is_open() && !app.state.dialogs.history_scrubber.is_open() {
        snapshots::cancel_pending(ctx);
        snapshots::cancel_coverage(ctx, app);
    }
}

fn show_history(ctx: &egui::Context, app: &mut ScratchpadApp) {
    let first = rail::begin(ctx, app);
    if first && let Some(id) = active_buffer(app) {
        list::select_buffer(ctx, id);
    }
    // Navigate immediately on opening, while preserving an existing dialog or
    // text-entry workflow's input and Escape handling.
    let focus_on_open = first
        && !app.state.search_state.open()
        && app.state.dialogs.tab_rename.is_none()
        && !app.state.dialogs.any_modal_open()
        && !crate::app::app_state::settings_state::showing_settings(app)
        && !egui::Popup::is_any_open(ctx);
    let mut action = None;
    let mut clear = None;
    let mut selected = scope(ctx, app);
    let close = panel::show(ctx, app, |ui, app| {
        snapshots::poll(ui, app);
        snapshots::hydrate(ui, app);
        let entries = snapshots::state_entries(ctx, app);
        if let Some(id) = controls(ui, app, &entries, &mut selected)
            && navigation::activate_file(app, id)
        {
            ctx.request_repaint();
        }
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
        let confirming = ctx.data(|data| data.get_temp::<ClearRequest>(confirm_key()));
        if let Some(request) = confirming {
            callout::section_frame(ui).corner_radius(0).show(ui, |ui| {
                ui.label(&request.description);
                ui.weak("This cannot be undone. Saved snapshots are kept.");
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        ctx.data_mut(|d| d.remove::<ClearRequest>(confirm_key()));
                    }
                    if ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new("Clear history").color(egui::Color32::WHITE),
                            )
                            .fill(crate::app::theme::CLOSE_HOVER_BG),
                        )
                        .clicked()
                    {
                        clear = Some(request.buffer_ids.clone());
                    }
                });
            });
        }
        list::render(ui, &entries, &catalog, target, &ids, &mut action, first);
        if focus_on_open {
            ctx.memory_mut(|memory| memory.request_focus(list::focus_id()));
            lock_navigation_focus(ctx, list::focus_id());
            navigation::retain_tool_focus(ctx, app);
        }
        snapshots::footer(ui, &catalog);
    });
    if close {
        rail::close(ctx, app);
        return;
    }
    if let Some(ids) = clear {
        rail::clear(ctx);
        ctx.data_mut(|d| d.remove::<ClearRequest>(confirm_key()));
        let saved = match snapshots::clear_history(ctx, app, &ids) {
            Ok(saved) => saved,
            Err(error) => {
                app.state.status.set_error_status_in_domain(
                    crate::app::app_state::StatusDomain::History,
                    format!("Could not clear history: {error}"),
                );
                return;
            }
        };
        let live = workspace_mutation::clear_text_history(app, &ids);
        if saved && !live {
            app.tab_manager.mark_session_dirty();
        }
        action = None;
    }
    if let Some(action) = action {
        rail::execute(ctx, app, action);
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
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let states = states::States::new(entries, id);
            // Allocate the actual count width before giving the filename the
            // remainder. A fixed allowance makes large counts grow the window.
            ui.weak(if snapshots::is_browsing(ui.ctx(), app, id) {
                "Snapshot".to_owned()
            } else {
                format!("{} / {}", states.position, states.entries.len())
            });
            truncated_label(
                ui,
                &buffer.name,
                ui.available_width(),
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
        });
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
    entries: &[crate::app::text_history::TextHistoryEntryView],
    selected: &mut HistoryTab,
) -> Option<u64> {
    let mut picked = None;
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
        picked = file_picker::show(ui, &picker, app, entries, selected);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if control_icon_button(ui, "clear", TRASH, "Clear undo/redo history", false, true)
                .clicked()
            {
                request_clear(ui.ctx(), app, *selected);
            }
        });
    });
    let mut follow = read_follow_focus(ui.ctx());
    if ui.checkbox(&mut follow, "Switch to changed file").changed() {
        write_follow_focus(ui.ctx(), follow);
    }
    picked
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
    .corner_radius(0);
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

#[cfg(test)]
mod tests;
