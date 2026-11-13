use super::state::{SearchStripActions, SearchStripState};
mod shortcuts;
use crate::app::app_state::{SearchFocusTarget, SearchStatus};
use crate::app::services::search::SearchMode;
use crate::app::ui::panel_style;
use eframe::egui;
use egui_phosphor::regular as icons;
use shortcuts::{TextInputKind, consume_search_strip_shortcuts, consume_text_input_keys};

const TOGGLE_TEXT_SIZE: f32 = 13.0;
const TOGGLE_ICON_SIZE: f32 = 15.0;

pub(super) fn show_search_controls(
    ui: &mut egui::Ui,
    state: &mut SearchStripState,
    actions: &mut SearchStripActions,
    find_id: egui::Id,
    replace_id: egui::Id,
) {
    recent_keys(ui, state, actions, find_id);
    let (caret_width, find) = ui
        .horizontal(|ui| {
            let caret = replace_caret(ui, state);
            (
                caret.rect.width() + ui.spacing().item_spacing.x,
                find_field(ui, state, actions, find_id),
            )
        })
        .inner;
    state.sync_focus(&find, SearchFocusTarget::FindInput);
    if find.has_focus() {
        consume_text_input_keys(ui, actions, TextInputKind::Find);
    }
    if let SearchStatus::InvalidQuery(error) = &state.progress.status {
        ui.horizontal(|ui| {
            ui.add_space(caret_width);
            ui.label(
                egui::RichText::new(error)
                    .monospace()
                    .size(11.0)
                    .color(ui.visuals().error_fg_color),
            );
        });
    }
    let mut replace_focused = false;
    if state.replace_open {
        let replace = ui
            .horizontal(|ui| {
                ui.add_space(caret_width);
                field(
                    ui,
                    "Replace with",
                    &mut state.replacement,
                    replace_id,
                    false,
                    |ui| {
                        flag(
                            ui,
                            &mut state.preserve_case,
                            "AB",
                            "Preserve case (Alt+P): needle → thread, Needle → Thread, NEEDLE → THREAD",
                        );
                    },
                )
            })
            .inner;
        state.sync_focus(&replace, SearchFocusTarget::ReplaceInput);
        replace_focused = replace.has_focus();
        if replace_focused {
            consume_text_input_keys(ui, actions, TextInputKind::Replace);
        }
    }
    consume_search_strip_shortcuts(ui, state, actions, find.has_focus() || replace_focused);
}

fn replace_caret(ui: &mut egui::Ui, state: &mut SearchStripState) -> egui::Response {
    let (icon, label) = if state.replace_open {
        (icons::CARET_DOWN, "Hide replace")
    } else {
        (icons::CARET_RIGHT, "Show replace")
    };
    let response = panel_style::icon_button(ui, "search.replace_toggle", icon, label, true);
    if response.clicked() {
        state.replace_open = !state.replace_open;
    }
    response
}

/// A frameless toggle that shows its state with the selection fill, like History's tabs.
fn toggle(
    ui: &mut egui::Ui,
    value: &mut bool,
    text: egui::RichText,
    name: &str,
    tooltip: &str,
) -> egui::Response {
    let response = ui
        .selectable_label(*value, text)
        .accessible_name(name)
        .on_hover_text(tooltip);
    if response.clicked() {
        *value = !*value;
    }
    response
}

fn flag(ui: &mut egui::Ui, value: &mut bool, label: &str, tooltip: &str) {
    toggle(
        ui,
        value,
        egui::RichText::new(label)
            .monospace()
            .size(TOGGLE_TEXT_SIZE),
        tooltip.split('\n').next().unwrap_or(tooltip),
        tooltip,
    );
}

fn icon(glyph: &str) -> egui::RichText {
    egui::RichText::new(glyph).font(egui_phosphor::font_id(TOGGLE_ICON_SIZE))
}

/// A full-width flat input with its toggles inside the same frame.
fn field(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut String,
    id: egui::Id,
    invalid: bool,
    controls: impl FnOnce(&mut egui::Ui),
) -> egui::Response {
    let multi_id = id.with("multiline");
    let mut multiline =
        ui.data(|d| d.get_temp::<bool>(multi_id)).unwrap_or(false) || value.contains('\n');
    let stroke = if invalid {
        egui::Stroke::new(1.0, ui.visuals().error_fg_color)
    } else if ui.memory(|m| m.has_focus(id)) {
        ui.visuals().selection.stroke
    } else {
        ui.visuals().widgets.inactive.bg_stroke
    };
    let align = if multiline {
        egui::Align::Min
    } else {
        egui::Align::Center
    };
    let response = egui::Frame::NONE
        .fill(ui.visuals().extreme_bg_color)
        .stroke(stroke)
        .inner_margin(egui::Margin::symmetric(4, 2))
        .show(ui, |ui| {
            ui.with_layout(egui::Layout::right_to_left(align), |ui| {
                toggle(
                    ui,
                    &mut multiline,
                    icon(icons::ARROWS_OUT_LINE_VERTICAL),
                    &format!("Expand {label} to multiple lines"),
                    "Multiple lines; Ctrl+Shift+Enter inserts a newline",
                );
                controls(ui);
                ui.with_layout(egui::Layout::left_to_right(align), |ui| {
                    let edit = if multiline {
                        egui::TextEdit::multiline(value).desired_rows(3).return_key(
                            egui::KeyboardShortcut::new(
                                egui::Modifiers::CTRL | egui::Modifiers::SHIFT,
                                egui::Key::Enter,
                            ),
                        )
                    } else {
                        egui::TextEdit::singleline(value)
                    };
                    ui.add(
                        edit.id(id)
                            .desired_width(ui.available_width())
                            .hint_text(label)
                            .frame(egui::Frame::NONE),
                    )
                    .accessible_name(label)
                })
                .inner
            })
            .inner
        })
        .inner;
    ui.data_mut(|d| d.insert_temp(multi_id, multiline));
    response
}

fn find_field(
    ui: &mut egui::Ui,
    state: &mut SearchStripState,
    actions: &mut SearchStripActions,
    find_id: egui::Id,
) -> egui::Response {
    let invalid = matches!(state.progress.status, SearchStatus::InvalidQuery(_));
    // Right-to-left: the last control added sits nearest the text.
    field(ui, "Find", &mut state.query, find_id, invalid, |ui| {
        recent_menu(ui, &state.recent, actions);
        let mut regex = state.mode == SearchMode::Regex;
        flag(
            ui,
            &mut regex,
            ".*",
            &format!(
                "{}\nReplacement captures: $1, ${{name}}, $$",
                crate::app::shortcut_tooltips::SEARCH_MODE_REGEX
            ),
        );
        state.mode = if regex {
            SearchMode::Regex
        } else {
            SearchMode::PlainText
        };
        flag(
            ui,
            &mut state.whole_word,
            "W",
            crate::app::shortcut_tooltips::SEARCH_WHOLE_WORD,
        );
        flag(
            ui,
            &mut state.match_case,
            "Aa",
            crate::app::shortcut_tooltips::SEARCH_MATCH_CASE,
        );
    })
}

fn recent_menu(
    ui: &mut egui::Ui,
    recent: &[crate::app::app_state::search_state::recent::RecentSearch],
    actions: &mut SearchStripActions,
) {
    let button = panel_style::icon_button(
        ui,
        "search.recent",
        icons::CLOCK_COUNTER_CLOCKWISE,
        "Recent searches",
        true,
    );
    egui::Popup::menu(&button)
        .frame(egui::Frame::popup(ui.style()).corner_radius(egui::CornerRadius::ZERO))
        .show(|ui| {
            if recent.is_empty() {
                ui.label(egui::RichText::new("No recent searches").weak());
            }
            for (i, recent) in recent.iter().enumerate() {
                if ui.button(recent.query.replace('\n', "⏎")).clicked() {
                    actions.recent_index = Some(i);
                    ui.close();
                }
            }
        });
}

fn recent_keys(
    ui: &mut egui::Ui,
    state: &SearchStripState,
    actions: &mut SearchStripActions,
    find_id: egui::Id,
) {
    if !ui.memory(|m| m.has_focus(find_id)) || state.query.contains('\n') {
        return;
    }
    let history_id = find_id.with("recent_position");
    let down = ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown));
    let up = ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp));
    if (up || down) && !state.recent.is_empty() {
        let current = ui.data(|d| d.get_temp::<usize>(history_id));
        let index = if up {
            current.map_or(0, |i| (i + 1).min(state.recent.len() - 1))
        } else {
            current.unwrap_or(1).saturating_sub(1)
        };
        ui.data_mut(|d| d.insert_temp(history_id, index));
        actions.recent_index = Some(index);
    }
}
