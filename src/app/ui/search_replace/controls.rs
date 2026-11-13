use super::state::{SearchStripActions, SearchStripState};
mod shortcuts;
use crate::app::app_state::SearchFocusTarget;
use crate::app::services::search::SearchMode;
use crate::app::theme::{border, text_muted};
use eframe::egui;
use shortcuts::{TextInputKind, consume_search_strip_shortcuts, consume_text_input_keys};

pub(super) fn show_search_controls(
    ui: &mut egui::Ui,
    state: &mut SearchStripState,
    actions: &mut SearchStripActions,
    find_id: egui::Id,
    replace_id: egui::Id,
) {
    recent_keys(ui, state, actions, find_id);
    let find = find_field(ui, state, actions, find_id);
    state.sync_focus(&find, SearchFocusTarget::FindInput);
    if find.has_focus() {
        consume_text_input_keys(ui, actions, TextInputKind::Find);
    }
    if let crate::app::app_state::SearchStatus::InvalidQuery(error) = &state.progress.status {
        ui.label(
            egui::RichText::new(error)
                .monospace()
                .size(12.0)
                .color(ui.visuals().error_fg_color),
        );
    }
    ui.horizontal(|ui| {
        let text = if state.replace_open {
            "▾ Replace"
        } else {
            "▸ Replace"
        };
        if ui.button(text).clicked() {
            state.replace_open = !state.replace_open;
        }
    });
    let mut replace_focused = false;
    if state.replace_open {
        let replace = field(
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
        );
        state.sync_focus(&replace, SearchFocusTarget::ReplaceInput);
        replace_focused = replace.has_focus();
        if replace_focused {
            consume_text_input_keys(ui, actions, TextInputKind::Replace);
        }
    }
    consume_search_strip_shortcuts(ui, state, actions, find.has_focus() || replace_focused);
}

fn flag(ui: &mut egui::Ui, value: &mut bool, label: &str, tooltip: &str) {
    if ui
        .selectable_label(*value, label)
        .accessible_name(tooltip.split('\n').next().unwrap_or(tooltip))
        .on_hover_text(tooltip)
        .clicked()
    {
        *value = !*value;
    }
}
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
    let response = ui
        .horizontal(|ui| {
            ui.add_sized(
                [82.0, 30.0],
                egui::Label::new(egui::RichText::new(label).size(12.5).color(text_muted(ui))),
            );
            egui::Frame::NONE
                .fill(ui.visuals().extreme_bg_color)
                .stroke(egui::Stroke::new(
                    1.0,
                    if invalid {
                        ui.visuals().error_fg_color
                    } else {
                        border(ui)
                    },
                ))
                .corner_radius(7)
                .inner_margin(egui::Margin::symmetric(6, 3))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let width = (ui.available_width()
                            - if label == "Find" { 160.0 } else { 66.0 })
                        .max(60.0);
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
                        let response = ui
                            .add(
                                edit.id(id)
                                    .desired_width(width)
                                    .hint_text(label)
                                    .frame(egui::Frame::NONE),
                            )
                            .accessible_name(label);
                        controls(ui);
                        if ui
                            .small_button("↕")
                            .accessible_name(format!("Expand {label} to multiple lines"))
                            .on_hover_text("Multiple lines; Ctrl+Shift+Enter inserts a newline")
                            .clicked()
                        {
                            multiline = !multiline;
                        }
                        response
                    })
                    .inner
                })
                .inner
        })
        .inner
        .on_hover_text("Ctrl+Shift+Enter inserts a newline in expanded fields");
    ui.data_mut(|d| d.insert_temp(multi_id, multiline));
    response
}

fn find_field(
    ui: &mut egui::Ui,
    state: &mut SearchStripState,
    actions: &mut SearchStripActions,
    find_id: egui::Id,
) -> egui::Response {
    let invalid = matches!(
        state.progress.status,
        crate::app::app_state::SearchStatus::InvalidQuery(_)
    );
    field(ui, "Find", &mut state.query, find_id, invalid, |ui| {
        flag(
            ui,
            &mut state.match_case,
            "Aa",
            crate::app::shortcut_tooltips::SEARCH_MATCH_CASE,
        );
        flag(
            ui,
            &mut state.whole_word,
            "W",
            crate::app::shortcut_tooltips::SEARCH_WHOLE_WORD,
        );
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
        ui.menu_button(
            egui::RichText::new(egui_phosphor::regular::CLOCK_COUNTER_CLOCKWISE)
                .font(egui_phosphor::font_id(16.0)),
            |ui| {
                if state.recent.is_empty() {
                    ui.label("No recent searches");
                }
                for (i, recent) in state.recent.iter().enumerate() {
                    if ui.button(recent.query.replace('\n', "⏎")).clicked() {
                        actions.recent_index = Some(i);
                        ui.close();
                    }
                }
            },
        )
        .response
        .accessible_name("Recent searches");
    })
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
    if !state.query.contains('\n') && (up || down) && !state.recent.is_empty() {
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
