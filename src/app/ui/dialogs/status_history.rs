//! Status messages in History's panel style: a severity switch, flat rows and the
//! selected message in full below.
use crate::app::app_state::{ScratchpadApp, StatusDomain, StatusFilter, StatusSeverity, workspace};
use crate::app::theme;
use crate::app::ui::{panel_style, panel_style::window, widget_ids};
use eframe::egui;

mod footer;
mod list;
mod model;
#[cfg(test)]
mod tests;

const DEFAULT_SIZE: egui::Vec2 = egui::vec2(460.0, 540.0);
const MIN_HEIGHT: f32 = 300.0;
/// The narrowest window content that shows the title and every filter on one row.
const HEADER_WIDTH: window::MeasuredWidth = window::MeasuredWidth {
    key: "status_history.header.min_width",
    estimate: 360.0,
    discard_reason: "Status header width changed",
};
const NOTE_SIZE: f32 = 12.0;

const FILTERS: [(StatusFilter, &str, &str); 4] = [
    (StatusFilter::All, "All", "Show every message"),
    (StatusFilter::Errors, "Errors", "Show only errors"),
    (StatusFilter::Warnings, "Warnings", "Show only warnings"),
    (StatusFilter::Info, "Info", "Show only information"),
];

#[derive(Default)]
pub(super) struct Actions {
    close: bool,
    select: Option<u64>,
    copy: bool,
    clear: bool,
    start_session: bool,
}

pub(crate) fn show_status_history_window(ctx: &egui::Context, app: &mut ScratchpadApp) {
    if !app.state.dialogs.status_history.is_open() {
        return;
    }
    let opening = std::mem::take(&mut app.state.dialogs.status_history.opening);
    if opening {
        app.state.dialogs.status_history.new_from = app.state.status.seen_until;
    }
    // Everything listed while the panel is open counts as seen.
    app.state.status.mark_seen();

    let dialog = &app.state.dialogs.status_history;
    let counts = model::Counts::of(app.state.status.history.iter());
    let rows = model::rows(app.state.status.history.iter(), dialog.filter);
    let selected = dialog
        .selected
        .and_then(|id| rows.iter().position(|row| row.id == id))
        .or((!rows.is_empty()).then_some(0));
    let view = list::ListView {
        rows: &rows,
        selected,
        new_from: dialog.new_from,
        empty: model::empty_message(dialog.filter),
        take_focus: opening,
    };

    let mut actions = Actions::default();
    let layer = widget_ids::area_layer_id(egui::Order::Foreground, "status_history_window");
    let outside = window::OutsideClick::before_show(ctx, app, layer);
    let frame = panel_style::window_frame(ctx, app.state.app_settings.editor_background_color());
    // Window sizes include the frame, so add its margin to the header content width.
    let min_size = egui::vec2(
        HEADER_WIDTH.get(ctx) + frame.total_margin().sum().x,
        MIN_HEIGHT,
    );
    let response =
        window::resizable(ctx, "Status", layer, frame, DEFAULT_SIZE, min_size).show(ctx, |ui| {
            window::contents(ui, "status_history_dialog", frame.fill, |ui| {
                actions.close |= header(ui, app, counts, outside.needs_close_button());
                notices(ui, app, &mut actions);
                summary(ui, app, &rows, view.new_from, &mut actions);
                footer::panel(ui, selected.map(|index| &rows[index]), &mut actions);
                list::show(ui, &view, &mut actions);
            });
        });
    if let Some(response) = response {
        // Ignore the opening click and interactions with menus.
        actions.close |= outside.after_show(ctx, app, &response.response);
    }
    actions.close |= ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
    apply(ctx, app, &rows, selected, actions);
}

fn apply(
    ctx: &egui::Context,
    app: &mut ScratchpadApp,
    rows: &[model::StatusRow],
    selected: Option<usize>,
    actions: Actions,
) {
    let dialog = &mut app.state.dialogs.status_history;
    if let Some(id) = actions.select {
        dialog.selected = Some(id);
    }
    if actions.copy
        && let Some(row) = selected.map(|index| &rows[index])
    {
        ctx.copy_text(row.copy_text());
    }
    if actions.clear {
        app.state.status.history.clear();
        dialog.selected = None;
    }
    if actions.start_session {
        dialog.filter = StatusFilter::All;
        crate::app::services::session_manager::start_new_session(app, ctx);
    }
    if actions.close {
        app.state.dialogs.status_history.close();
        workspace::accessors::request_focus_for_active_view(app);
    }
}

/// Title on the left; the severity switch with counts on the right. Returns true on close.
fn header(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    counts: model::Counts,
    show_close: bool,
) -> bool {
    let close = show_close.then_some(("status_history.close", "Close status"));
    let header = window::header(ui, "Status", close, |switch| {
        let filter = &mut app.state.dialogs.status_history.filter;
        for (candidate, label, hint) in FILTERS.into_iter().rev() {
            let count = counts.get(candidate);
            let text = if candidate == StatusFilter::All || count == 0 {
                label.to_owned()
            } else {
                format!("{label} {count}")
            };
            let response = switch
                .ui
                .selectable_value(filter, candidate, text)
                .on_hover_text(hint);
            switch.measure(&response);
        }
    });
    HEADER_WIDTH.remember(ui.ctx(), header.required_width);
    header.close
}

/// Session reset and document recovery, which need action before the message list.
fn notices(ui: &mut egui::Ui, app: &mut ScratchpadApp, actions: &mut Actions) {
    egui::ScrollArea::vertical()
        .id_salt("status_history.notices")
        .max_height(ui.available_height() * 0.5)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            session_notice(ui, app, actions);
            // Recovery is shared with Settings; keep its headings at panel size.
            let heading = panel_style::font(ui, 13.0);
            ui.style_mut()
                .text_styles
                .insert(egui::TextStyle::Heading, heading);
            super::recovery::render(ui, app);
        });
}

fn session_notice(ui: &mut egui::Ui, app: &ScratchpadApp, actions: &mut Actions) {
    if !app.state.persistence.session_store.storage_unavailable() {
        return;
    }
    let muted = theme::text_muted(ui);
    ui.label(egui::RichText::new("Start a new session").strong());
    if let Some(detail) = app
        .state
        .status
        .history
        .iter()
        .rev()
        .find(|row| row.domain == StatusDomain::Session && row.severity == StatusSeverity::Error)
        .and_then(|row| row.detail.as_ref())
    {
        ui.add(
            egui::Label::new(
                egui::RichText::new(detail)
                    .size(NOTE_SIZE)
                    .monospace()
                    .color(ui.visuals().warn_fg_color),
            )
            .wrap(),
        );
    }
    ui.add(
        egui::Label::new(
            egui::RichText::new(
                "The previous session could not be opened. Starting a new session will move the old session file to a temporary backup and create a new file at the usual location. The backup may be deleted during temporary-file cleanup.",
            )
            .size(NOTE_SIZE)
            .color(muted),
        )
        .wrap(),
    );
    let path = app
        .state
        .persistence
        .session_store
        .root()
        .join("session.db")
        .display()
        .to_string();
    ui.add(
        egui::Label::new(
            egui::RichText::new(format!("Session file: {path}"))
                .size(NOTE_SIZE)
                .color(muted),
        )
        .truncate(),
    );
    let resetting = app.state.persistence.session_reset.is_some();
    let busy = app.state.background_io.has_pending_background_actions();
    let enabled = !resetting && !busy;
    ui.horizontal(|ui| {
        if busy && !resetting {
            ui.label(
                egui::RichText::new("Waiting for current file operations to finish…")
                    .size(NOTE_SIZE)
                    .color(muted),
            );
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let text = if resetting {
                "Creating new session…"
            } else {
                "Start new session"
            };
            let button = window::primary_button(ui, text, enabled);
            actions.start_session |= ui.add_enabled(enabled, button).clicked();
        });
    });
    ui.separator();
}

/// One muted line above the list, like Search's: what is listed and how much is new.
fn summary(
    ui: &mut egui::Ui,
    app: &ScratchpadApp,
    rows: &[model::StatusRow],
    new_from: u64,
    actions: &mut Actions,
) {
    let listed: usize = rows.iter().map(|row| row.repeats).sum();
    let new = app
        .state
        .status
        .history
        .iter()
        .filter(|status| {
            status.id >= new_from
                && app
                    .state
                    .dialogs
                    .status_history
                    .filter
                    .matches(status.severity)
        })
        .count();
    let count = match listed {
        1 => "1 message".to_owned(),
        n => format!("{n} messages"),
    };
    let text = if new > 0 {
        format!("{count} · {new} new")
    } else {
        count
    };
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            actions.clear |= ui
                .add_enabled(
                    !app.state.status.history.is_empty(),
                    egui::Button::new(egui::RichText::new("Clear").size(NOTE_SIZE)).frame(false),
                )
                .on_hover_text("Remove every message from this list")
                .clicked();
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(text)
                            .size(NOTE_SIZE)
                            .color(theme::text_muted(ui)),
                    )
                    .truncate(),
                );
            });
        });
    });
}
