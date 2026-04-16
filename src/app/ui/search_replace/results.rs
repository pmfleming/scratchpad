use super::state::{SearchStripActions, SearchStripState};
use crate::app::app_state::{SearchResultEntry, SearchResultGroup};
use crate::app::theme::{
    action_bg, action_hover_bg, border, tab_selected_accent, tab_selected_bg, text_muted,
    text_primary,
};
use eframe::egui;

const SEARCH_RESULTS_VIEWPORT_HEIGHT: f32 = 320.0;
const SEARCH_RESULT_ROW_HEIGHT: f32 = 40.0;

struct BufferSection<'a> {
    buffer_label: &'a str,
    entries: &'a [SearchResultEntry],
}

pub(super) fn show_search_results(
    ui: &mut egui::Ui,
    state: &SearchStripState,
    actions: &mut SearchStripActions,
) {
    if state.query.is_empty() {
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new("Type to search across the selected scope.").color(text_muted(ui)),
        );
        return;
    }

    ui.add_space(8.0);
    ui.separator();
    ui.add_space(6.0);
    show_search_progress(ui, state);

    if state.result_groups.is_empty() {
        if !state.progress.searching {
            ui.label(egui::RichText::new("No matches found.").color(text_muted(ui)));
        }
        return;
    }

    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), SEARCH_RESULTS_VIEWPORT_HEIGHT),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            egui::ScrollArea::vertical()
                .id_salt("search_results_list")
                .max_height(SEARCH_RESULTS_VIEWPORT_HEIGHT)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for group in &state.result_groups {
                        show_search_group(ui, group, actions);
                    }
                });
        },
    );
}

fn show_search_progress(ui: &mut egui::Ui, state: &SearchStripState) {
    if state.progress.searching {
        ui.label(egui::RichText::new("Searching...").color(text_muted(ui)));
        ui.add_space(4.0);
        return;
    }

    if state.progress.displayed_match_count < state.progress.total_match_count {
        ui.label(
            egui::RichText::new(format!(
                "Showing first {} of {} matches.",
                state.progress.displayed_match_count, state.progress.total_match_count
            ))
            .color(text_muted(ui)),
        );
        ui.add_space(4.0);
    }
}

fn show_search_group(
    ui: &mut egui::Ui,
    group: &SearchResultGroup,
    actions: &mut SearchStripActions,
) {
    for section in buffer_sections(group) {
        show_file_section(ui, group, section.buffer_label, section.entries, actions);
    }
}

fn buffer_sections(group: &SearchResultGroup) -> Vec<BufferSection<'_>> {
    let mut sections = Vec::new();
    let mut start = 0;

    while start < group.entries.len() {
        let buffer_id = group.entries[start].buffer_id;
        let mut end = start + 1;
        while end < group.entries.len() && group.entries[end].buffer_id == buffer_id {
            end += 1;
        }

        sections.push(BufferSection {
            buffer_label: group.entries[start].buffer_label.as_str(),
            entries: &group.entries[start..end],
        });
        start = end;
    }

    sections
}

fn show_file_section(
    ui: &mut egui::Ui,
    group: &SearchResultGroup,
    buffer_label: &str,
    entries: &[SearchResultEntry],
    actions: &mut SearchStripActions,
) {
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(egui_phosphor::regular::FILE_TEXT)
                .size(16.0)
                .color(text_muted(ui)),
        );
        ui.label(
            egui::RichText::new(buffer_label)
                .strong()
                .size(16.0)
                .color(text_primary(ui)),
        );
        show_count_badge(ui, entries.len());
    });

    if group.tab_label != buffer_label {
        ui.label(
            egui::RichText::new(&group.tab_label)
                .small()
                .color(text_muted(ui)),
        );
    }

    ui.add_space(2.0);
    for entry in entries {
        if show_result_row(ui, entry).clicked() {
            actions.selected_match_index = Some(entry.match_index);
        }

        ui.add_space(2.0);
    }
}

fn show_result_row(ui: &mut egui::Ui, entry: &SearchResultEntry) -> egui::Response {
    let inner = egui::Frame::NONE
        .fill(match_fill(ui, entry.active))
        .stroke(egui::Stroke::new(1.0, match_border(ui, entry.active)))
        .corner_radius(egui::CornerRadius::same(10))
        .inner_margin(egui::Margin::symmetric(10, 6))
        .show(ui, |ui| {
            let row_width = ui.available_width();

            ui.allocate_ui_with_layout(
                egui::vec2(row_width, SEARCH_RESULT_ROW_HEIGHT - 12.0),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.add_sized(
                        egui::vec2(row_width, 0.0),
                        egui::Label::new(match_body(ui, entry)).truncate(),
                    );
                },
            )
            .response
        });

    inner.response.interact(egui::Sense::click())
}

fn match_body(ui: &egui::Ui, entry: &SearchResultEntry) -> egui::WidgetText {
    let mut job = egui::text::LayoutJob::default();
    append_text(
        &mut job,
        &format!("{}:{} ", entry.line_number, entry.column_number),
        text_muted(ui),
        15.0,
    );
    append_text(&mut job, &match_preview(entry), text_primary(ui), 16.0);
    job.into()
}

fn match_preview(entry: &SearchResultEntry) -> String {
    if entry.preview.is_empty() {
        "Match".to_owned()
    } else {
        entry.preview.clone()
    }
}

fn append_text(job: &mut egui::text::LayoutJob, text: &str, color: egui::Color32, size: f32) {
    job.append(
        text,
        0.0,
        egui::TextFormat {
            font_id: egui::FontId::proportional(size),
            color,
            ..Default::default()
        },
    );
}

fn show_count_badge(ui: &mut egui::Ui, count: usize) {
    egui::Frame::NONE
        .fill(action_hover_bg(ui))
        .stroke(egui::Stroke::new(1.0, border(ui)))
        .corner_radius(egui::CornerRadius::same(8))
        .inner_margin(egui::Margin::symmetric(6, 2))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(count.to_string())
                    .size(14.0)
                    .color(text_muted(ui)),
            );
        });
}

fn match_fill(ui: &egui::Ui, active: bool) -> egui::Color32 {
    if active {
        tab_selected_bg(ui)
    } else {
        action_bg(ui)
    }
}

fn match_border(ui: &egui::Ui, active: bool) -> egui::Color32 {
    if active {
        tab_selected_accent(ui).gamma_multiply(0.95)
    } else {
        border(ui)
    }
}

#[cfg(test)]
mod tests {
    use super::buffer_sections;
    use crate::app::app_state::{SearchResultEntry, SearchResultGroup};

    fn entry(match_index: usize, buffer_id: u64, buffer_label: &str) -> SearchResultEntry {
        SearchResultEntry {
            match_index,
            buffer_id,
            buffer_label: buffer_label.to_owned(),
            line_number: 1,
            column_number: match_index + 1,
            preview: format!("match {match_index}"),
            active: false,
        }
    }

    #[test]
    fn buffer_sections_keep_same_name_buffers_separate() {
        let group = SearchResultGroup {
            tab_index: 0,
            tab_label: "Workspace".to_owned(),
            entries: vec![
                entry(0, 10, "mod.rs"),
                entry(1, 10, "mod.rs"),
                entry(2, 20, "mod.rs"),
            ],
        };

        let sections = buffer_sections(&group);
        assert_eq!(sections.len(), 2);
        assert_eq!(sections[0].buffer_label, "mod.rs");
        assert_eq!(sections[0].entries.len(), 2);
        assert_eq!(sections[1].buffer_label, "mod.rs");
        assert_eq!(sections[1].entries.len(), 1);
    }
}
