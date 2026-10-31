use super::{HistoryTab, model, scope_buffer, scope_key};
use crate::app::{
    app_state::ScratchpadApp,
    chrome::tab_label_font_id,
    domain::BufferState,
    text_history::TextHistoryEntryView,
    theme::{self, TAB_HEIGHT},
    ui::{callout, widget_ids},
};
use eframe::egui;
use std::collections::HashSet;

pub(super) fn show(
    ui: &mut egui::Ui,
    picker: &egui::Response,
    app: &ScratchpadApp,
    entries: &[TextHistoryEntryView],
    selected: &mut HistoryTab,
) -> Option<u64> {
    let mut picked = None;
    egui::Popup::menu(picker)
        .frame(callout::frame(ui).corner_radius(0))
        .show(|ui| {
            let font = tab_label_font_id(app.state.app_settings.font_size());
            ui.style_mut().override_font_id = Some(font.clone());
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.spacing_mut().scroll = theme::tab_list_scroll_style();
            ui.set_width(360.0_f32.min((ui.ctx().content_rect().width() - 32.0).max(0.0)));
            let counts = model::entry_counts(entries);
            let mut seen = HashSet::new();
            let files: Vec<_> = app
                .tab_manager
                .tabs
                .as_slice()
                .iter()
                .flat_map(|tab| tab.buffers())
                .filter_map(|buffer| {
                    counts
                        .get(&buffer.id)
                        .copied()
                        .filter(|&count| count > 0)
                        .filter(|_| seen.insert(buffer.id))
                        .map(|count| (buffer, count))
                })
                .collect();
            if files.is_empty() {
                ui.add_sized(
                    egui::vec2(ui.available_width(), TAB_HEIGHT),
                    egui::Label::new("No files with changes").selectable(false),
                );
                return;
            }
            egui::ScrollArea::vertical()
                .id_salt("history.file_picker")
                .auto_shrink([false, true])
                .max_height(TAB_HEIGHT * 8.0)
                .show(ui, |ui| {
                    let selected_id = scope_buffer(*selected, app);
                    for (buffer, count) in files {
                        if row(ui, buffer, count, selected_id == Some(buffer.id), &font).clicked() {
                            *selected = HistoryTab::File(buffer.id);
                            picked = Some(buffer.id);
                            ui.ctx()
                                .data_mut(|data| data.insert_temp(scope_key(), buffer.id));
                            ui.close();
                        }
                    }
                });
        });
    picked
}

fn row(
    ui: &mut egui::Ui,
    buffer: &BufferState,
    count: usize,
    selected: bool,
    font: &egui::FontId,
) -> egui::Response {
    let response = widget_ids::surface_response(
        ui,
        ("history.file_picker", buffer.id),
        widget_ids::WidgetRole::ActionButton,
        |ui| {
            ui.add_sized(
                egui::vec2(ui.available_width(), TAB_HEIGHT),
                egui::Button::new("").frame(false),
            )
        },
    );
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::Role::Button,
            true,
            selected,
            format!("{}  ·  {count}", buffer.name),
        )
    });
    let rect = response.rect;
    if selected || response.hovered() || response.has_focus() {
        ui.painter().rect_filled(
            rect,
            0,
            if selected {
                theme::tab_active_bg(ui)
            } else {
                theme::tab_hover_bg(ui)
            },
        );
    }
    if selected || response.has_focus() {
        ui.painter().rect_stroke(
            rect,
            0,
            egui::Stroke::new(1.0, theme::tab_selected_accent(ui)),
            egui::StrokeKind::Inside,
        );
    }
    let count_galley =
        ui.painter()
            .layout_no_wrap(count.to_string(), font.clone(), theme::text_muted(ui));
    let count_pos = egui::pos2(
        rect.right() - 10.0 - count_galley.size().x,
        rect.center().y - count_galley.size().y * 0.5,
    );
    ui.painter()
        .galley(count_pos, count_galley, theme::text_muted(ui));
    let mut label = egui::text::LayoutJob::simple_singleline(
        buffer.name.clone(),
        font.clone(),
        theme::text_primary(ui),
    );
    label.wrap.max_width = (count_pos.x - rect.left() - 24.0).max(0.0);
    label.wrap.max_rows = 1;
    label.wrap.break_anywhere = true;
    let label = ui.fonts_mut(|fonts| fonts.layout_job(label));
    ui.painter().galley(
        egui::pos2(rect.left() + 8.0, rect.center().y - label.size().y * 0.5),
        label,
        theme::text_primary(ui),
    );
    let path = buffer
        .path
        .as_ref()
        .map_or_else(|| buffer.name.clone(), |path| path.display().to_string());
    response.on_hover_text(format!("{path}\n{count} changes"))
}
