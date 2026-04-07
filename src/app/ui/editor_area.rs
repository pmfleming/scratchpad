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
                    ui.label(format!("Lines: {}", tab.buffer.line_count));
                    ui.separator();
                    ui.label(&tab.buffer.encoding);
                    if tab.buffer.content.len() > 5 * 1024 * 1024 {
                        ui.separator();
                        ui.label(
                            egui::RichText::new("⚠ Large file: performance may be degraded")
                                .color(egui::Color32::YELLOW),
                        );
                    }
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

        handle_editor_zoom(ctx, ui, app);
        app.active_tab_index = app.active_tab_index.min(app.tabs.len() - 1);
        let active_tab_index = app.active_tab_index;
        let editor_changed = render_editor(ui, app, active_tab_index);
        if editor_changed {
            apply_editor_change(app, active_tab_index);
        }
    });
}

fn handle_editor_zoom(ctx: &egui::Context, ui: &egui::Ui, app: &mut ScratchpadApp) {
    let panel_rect = ui.max_rect();
    let pointer_over_editor = ui.rect_contains_pointer(panel_rect);
    let zoom_factor = ctx.input(|input| input.zoom_delta());
    if pointer_over_editor && zoom_factor != 1.0 {
        app.font_size = (app.font_size * zoom_factor).clamp(8.0, 72.0);
        app.mark_session_dirty();
    }
}

fn render_editor(ui: &mut egui::Ui, app: &mut ScratchpadApp, active_tab_index: usize) -> bool {
    let font_id = egui::FontId::monospace(app.font_size);
    let editor_font_id = font_id.clone();
    let word_wrap = app.word_wrap;

    let buffer = &mut app.tabs[active_tab_index].buffer;
    let mut layouter = build_layouter(font_id, word_wrap);

    egui::ScrollArea::both()
        .id_source(("editor_scroll", active_tab_index))
        .auto_shrink([false, false])
        .show(ui, |ui| {
            render_editor_text_edit(ui, buffer, word_wrap, &editor_font_id, &mut layouter)
        })
        .inner
}

fn build_layouter(
    font_id: egui::FontId,
    word_wrap: bool,
) -> impl FnMut(&egui::Ui, &str, f32) -> std::sync::Arc<egui::Galley> {
    move |ui: &egui::Ui, text: &str, wrap_width: f32| {
        let mut job = egui::text::LayoutJob::default();
        job.wrap.max_width = if word_wrap { wrap_width } else { f32::INFINITY };
        job.append(
            text,
            0.0,
            egui::TextFormat {
                font_id: font_id.clone(),
                color: TEXT_PRIMARY,
                ..Default::default()
            },
        );
        ui.fonts(|fonts| fonts.layout_job(job))
    }
}

fn render_editor_text_edit(
    ui: &mut egui::Ui,
    buffer: &mut crate::app::domain::BufferState,
    word_wrap: bool,
    editor_font_id: &egui::FontId,
    layouter: &mut impl FnMut(&egui::Ui, &str, f32) -> std::sync::Arc<egui::Galley>,
) -> bool {
    let editor = egui::TextEdit::multiline(&mut buffer.content)
        .font(editor_font_id.clone())
        .desired_width(if word_wrap {
            ui.available_width()
        } else {
            f32::INFINITY
        })
        .desired_rows(buffer.line_count)
        .lock_focus(true)
        .layouter(layouter);

    ui.add(editor).changed()
}

fn apply_editor_change(app: &mut ScratchpadApp, active_tab_index: usize) {
    let buffer = &mut app.tabs[active_tab_index].buffer;
    buffer.line_count = buffer.content.lines().count().max(1);
    buffer.is_dirty = true;
    app.status_message = None;
    app.mark_session_dirty();
}
