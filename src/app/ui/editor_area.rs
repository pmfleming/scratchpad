use crate::app::app_state::ScratchpadApp;
use crate::app::theme::*;
use eframe::egui;

pub(crate) fn show_status_bar(ctx: &egui::Context, app: &mut ScratchpadApp) {
    egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
        ui.horizontal(|ui| {
            if let Some(tab) = app.active_tab() {
                ui.label(format!(
                    "Path: {}",
                    tab.buffer
                        .path
                        .as_ref()
                        .map(|path| path.to_string_lossy())
                        .unwrap_or_else(|| "Untitled".into())
                ));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(format!("Lines: {}", tab.buffer.content.lines().count()));
                });
            }
            if let Some(message) = &app.status_message {
                ui.separator();
                ui.label(message);
            }
        });
    });
}

pub(crate) fn show_editor(ctx: &egui::Context, app: &mut ScratchpadApp) {
    egui::CentralPanel::default().show(ctx, |ui| {
        if app.tabs.is_empty() {
            return;
        }

        let panel_rect = ui.max_rect();
        let pointer_over_editor = ui.rect_contains_pointer(panel_rect);
        let zoom_factor = ctx.input(|input| input.zoom_delta());
        if pointer_over_editor && zoom_factor != 1.0 {
            app.font_size = (app.font_size * zoom_factor).clamp(8.0, 72.0);
            app.mark_session_dirty();
        }

        app.active_tab_index = app.active_tab_index.min(app.tabs.len() - 1);
        let active_tab_index = app.active_tab_index;
        let font_id = egui::FontId::monospace(app.font_size);
        let editor_font_id = font_id.clone();
        let text_color = TEXT_PRIMARY;
        let word_wrap = app.word_wrap;
        let mut editor_changed = false;

        {
            let buffer = &mut app.tabs[active_tab_index].buffer;
            let line_count = buffer.content.lines().count().max(1);
            let mut layouter = move |ui: &egui::Ui, text: &str, wrap_width: f32| {
                let mut job = egui::text::LayoutJob::default();
                job.wrap.max_width = if word_wrap { wrap_width } else { f32::INFINITY };
                job.append(
                    text,
                    0.0,
                    egui::TextFormat {
                        font_id: font_id.clone(),
                        color: text_color,
                        ..Default::default()
                    },
                );
                ui.fonts(|fonts| fonts.layout_job(job))
            };

            egui::ScrollArea::both()
                .id_source(("editor_scroll", active_tab_index))
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let editor = egui::TextEdit::multiline(&mut buffer.content)
                        .font(editor_font_id)
                        .desired_width(if word_wrap {
                            ui.available_width()
                        } else {
                            f32::INFINITY
                        })
                        .desired_rows(line_count)
                        .lock_focus(true)
                        .layouter(&mut layouter);

                    if ui.add(editor).changed() {
                        buffer.is_dirty = true;
                        editor_changed = true;
                    }
                });
        }

        if editor_changed {
            app.status_message = None;
            app.mark_session_dirty();
        }
    });
}