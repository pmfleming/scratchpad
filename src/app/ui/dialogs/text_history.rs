mod list;
mod model;
mod persistence;

use self::model::{
    TextHistoryAction, TextHistoryFileGroup, TextHistoryRow, file_groups_from_entries,
    timeline_rows_from_entries,
};
use self::persistence::{read_active_tab, read_follow_focus, write_active_tab, write_follow_focus};
use super::common::show_centered_callout;
use crate::app::app_state::ScratchpadApp;
use crate::app::theme::{action_bg, border, tab_selected_accent, tab_selected_bg};
use crate::app::ui::settings::dialog_card_frame;
use crate::app::ui::{callout, settings, widget_ids};
use eframe::egui;
use egui_phosphor::regular::{CLOCK_COUNTER_CLOCKWISE, CROSSHAIR, FILES, TRASH};

pub(super) const TEXT_HISTORY_SIZE: egui::Vec2 = egui::vec2(
    crate::app::ui::search_replace::SEARCH_DIALOG_WIDTH - 20.0,
    520.0,
);
const TEXT_HISTORY_TITLE_SIZE: f32 = 24.0;
pub(super) const TEXT_HISTORY_LIST_MIN_HEIGHT: f32 = 330.0;
pub(super) const HISTORY_PILL_CORNER_RADIUS: u8 = 8;
pub(super) const HISTORY_PILL_INNER_MARGIN: i8 = 10;
pub(super) const HISTORY_PILL_SPACING: f32 = 6.0;
const TAB_BUTTON_HEIGHT: f32 = 30.0;
const HISTORY_CARD_CORNER_RADIUS: u8 = 12;
const UNDONE_OPACITY: f32 = 0.55;

#[derive(Clone, Copy, PartialEq, Eq)]
enum HistoryTab {
    Timeline,
    ByFile,
}

struct TextHistoryWindowInputs<'a> {
    timeline_rows: &'a [TextHistoryRow],
    file_groups: &'a [TextHistoryFileGroup],
    active_tab: HistoryTab,
    follow_focus: bool,
}

struct TextHistoryWindowState<'a> {
    next_tab: &'a mut HistoryTab,
    next_follow_focus: &'a mut bool,
    action: &'a mut Option<TextHistoryAction>,
    close_requested: &'a mut bool,
    clear_requested: &'a mut bool,
}

pub(crate) fn show_text_history_window(ctx: &egui::Context, app: &mut ScratchpadApp) {
    if !app.state.text_history_open {
        return;
    }

    let entries = app.cached_text_history_entries();
    let timeline_rows = timeline_rows_from_entries(entries.iter());
    let file_groups = file_groups_from_entries(entries.iter());

    let mut action: Option<TextHistoryAction> = None;
    let mut close_requested = false;
    let mut clear_requested = false;
    let active_tab = read_active_tab(ctx);
    let mut next_tab = active_tab;
    let follow_focus = read_follow_focus(ctx);
    let mut next_follow_focus = follow_focus;

    show_centered_callout(ctx, "text_history_window", TEXT_HISTORY_SIZE, |ui| {
        widget_ids::feature_scope(ui, "text_history_dialog", |ui| {
            render_text_history_window(
                ui,
                TextHistoryWindowInputs {
                    timeline_rows: &timeline_rows,
                    file_groups: &file_groups,
                    active_tab,
                    follow_focus,
                },
                TextHistoryWindowState {
                    next_tab: &mut next_tab,
                    next_follow_focus: &mut next_follow_focus,
                    action: &mut action,
                    close_requested: &mut close_requested,
                    clear_requested: &mut clear_requested,
                },
            );
        });
    });

    if next_tab != active_tab {
        write_active_tab(ctx, next_tab);
    }
    if next_follow_focus != follow_focus {
        write_follow_focus(ctx, next_follow_focus);
    }
    if close_requested {
        crate::app::commands::close_text_history(app);
    }
    if clear_requested {
        let _ = app.clear_text_history();
    }
    if let Some(action) = action {
        let _ =
            app.apply_text_history_to_entry(action.buffer_id, action.entry_id, next_follow_focus);
    }
}

fn render_text_history_window(
    ui: &mut egui::Ui,
    inputs: TextHistoryWindowInputs<'_>,
    state: TextHistoryWindowState<'_>,
) {
    settings::apply_dialog_typography(ui);
    callout::apply_spacing(ui);
    ui.spacing_mut().item_spacing = egui::vec2(8.0, 12.0);
    if render_header(ui) {
        *state.close_requested = true;
    }
    ui.add_space(4.0);
    history_card(ui, |ui| {
        render_controls(
            ui,
            inputs.active_tab,
            state.next_tab,
            inputs.follow_focus,
            state.next_follow_focus,
            !inputs.timeline_rows.is_empty(),
            state.clear_requested,
        );
    });

    match inputs.active_tab {
        HistoryTab::Timeline => list::render_timeline(ui, inputs.timeline_rows, state.action),
        HistoryTab::ByFile => list::render_by_file(ui, inputs.file_groups, state.action),
    }
}

fn history_card<R>(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let width = ui.available_width();
    let content_width = (width - 24.0).max(0.0);
    dialog_card_frame(ui)
        .corner_radius(egui::CornerRadius::same(HISTORY_CARD_CORNER_RADIUS))
        .show(ui, |ui| {
            ui.set_width(content_width);
            ui.set_min_width(content_width);
            ui.set_max_width(content_width);
            add_contents(ui)
        })
        .inner
}

fn render_header(ui: &mut egui::Ui) -> bool {
    callout::header_row(ui, "text_history.header", "Close history", |ui| {
        ui.label(
            egui::RichText::new("History")
                .size(TEXT_HISTORY_TITLE_SIZE)
                .color(callout::text(ui)),
        );
    })
}

fn render_controls(
    ui: &mut egui::Ui,
    active: HistoryTab,
    next: &mut HistoryTab,
    follow_focus: bool,
    next_follow_focus: &mut bool,
    has_history: bool,
    clear_requested: &mut bool,
) {
    let tabs = [
        (
            "timeline",
            CLOCK_COUNTER_CLOCKWISE,
            "Timeline",
            HistoryTab::Timeline,
        ),
        ("by_file", FILES, "By file", HistoryTab::ByFile),
    ];

    ui.horizontal(|ui| {
        for (id_source, icon, tooltip, tab) in tabs {
            if control_icon_button(ui, id_source, icon, tooltip, active == tab, true).clicked() {
                *next = tab;
            }
        }
        if control_icon_button(
            ui,
            "follow_focus",
            CROSSHAIR,
            if follow_focus {
                "Follow undo is on"
            } else {
                "Follow undo is off"
            },
            follow_focus,
            true,
        )
        .clicked()
        {
            *next_follow_focus = !follow_focus;
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if control_icon_button(
                ui,
                "clear_history",
                TRASH,
                "Clear all text history",
                false,
                has_history,
            )
            .clicked()
            {
                *clear_requested = true;
            }
        });
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
            .font(egui::FontId::proportional(16.0))
            .color(if enabled {
                text_color
            } else {
                text_color.gamma_multiply(UNDONE_OPACITY)
            }),
    )
    .min_size(egui::vec2(36.0, TAB_BUTTON_HEIGHT))
    .fill(fill)
    .stroke(egui::Stroke::new(1.0, stroke_color))
    .corner_radius(egui::CornerRadius::same(8));
    widget_ids::scope(ui, ("text_history.control", id_source), |ui| {
        ui.add_enabled(enabled, button)
    })
    .inner
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
        egui::vec2(width, 0.0),
        egui::Label::new(egui::RichText::new(text).size(size).color(color))
            .truncate()
            .sense(sense),
    )
}

#[cfg(test)]
mod tests;
