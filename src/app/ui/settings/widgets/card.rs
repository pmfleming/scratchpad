use super::{ScratchpadApp, SettingsUi, egui, record_settings_control_box, text_muted};
use crate::app::{
    services::settings_store::{IndentationStyle, TabDisplayMode},
    ui::{settings::EDITOR_FONT_FAMILY, widget_ids},
};

pub(in crate::app::ui::settings) fn settings_card_frame(
    ui: &mut egui::Ui,
    contents: impl FnOnce(&mut egui::Ui),
) {
    let width = ui.available_width().min(SettingsUi::LAYOUT.card_max_width);
    let response = SettingsUi::card_frame(ui).show(ui, |ui| {
        ui.set_width((width - 38.0).max(0.0));
        contents(ui);
    });
    super::layout_state::record_settings_card_box("settings_card_frame", response.response.rect);
}

/// Both previews share font, palette, highlight, padding and wrapping. Quotes
/// align right/center; the indentation sample keeps its meaningful leading edge.
pub(in crate::app::ui::settings) fn render_preview_panel(
    ui: &mut egui::Ui,
    app: &ScratchpadApp,
    tabs: bool,
) {
    let settings = &app.state.app_settings;
    ui.add_space(12.0);
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(if tabs { "Tab preview" } else { "Text preview" })
                .size(12.5)
                .color(text_muted(ui)),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                egui::RichText::new(format!(
                    "{} · {:.0} pt",
                    settings.editor_font_selection().label(),
                    settings.font_size()
                ))
                .size(12.5)
                .color(text_muted(ui)),
            );
        });
    });
    let width = ui.available_width();
    let padding = settings.editor_gutter() as i8;
    let response = egui::Frame::new()
        .fill(settings.editor_background_color())
        .stroke(egui::Stroke::new(1.0, SettingsUi::card_border(ui)))
        .corner_radius(8)
        .inner_margin(egui::Margin::same(padding))
        .show(ui, |ui| {
            ui.set_width((width - 2.0 * f32::from(padding) - 2.0).max(0.0));
            ui.set_min_height(96.0);
            let (text, highlight) = if tabs {
                (
                    tab_preview_text(
                        settings.indentation_style(),
                        settings.tab_display(),
                        settings.editor_tab_width(),
                    ),
                    "A small idea",
                )
            } else {
                let (text, highlight) = crate::app::ui::settings::PREVIEW_QUOTES
                    [app.state.settings_preview_quote_index];
                (text.to_owned(), highlight)
            };
            let font = egui::FontId::new(
                settings.font_size(),
                egui::FontFamily::Name(EDITOR_FONT_FAMILY.into()),
            );
            let format = egui::TextFormat {
                font_id: font.clone(),
                color: settings.editor_text_color(),
                ..Default::default()
            };
            let highlighted = egui::TextFormat {
                background: settings.editor_text_highlight_color(),
                color: settings.editor_text_highlight_text_color(),
                ..format.clone()
            };
            let mut job = egui::text::LayoutJob::default();
            job.wrap.max_width = if settings.word_wrap() {
                ui.available_width()
            } else {
                f32::INFINITY
            };
            let start = text.find(highlight).unwrap_or(0);
            let end = start + highlight.len();
            job.append(&text[..start], 0.0, format.clone());
            job.append(&text[start..end], 0.0, highlighted);
            job.append(&text[end..], 0.0, format);
            // An unwrapped preview must not widen the settings page.
            let preview_width = ui.available_width();
            egui::ScrollArea::horizontal()
                .id_salt(("settings.preview", tabs))
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    if tabs {
                        ui.label(job);
                    } else {
                        right_centered_quote(ui, job, preview_width);
                    }
                });
        });
    record_settings_control_box(
        if tabs { "preview.tabs" } else { "preview.text" },
        response.response.rect,
    );
}

fn right_centered_quote(ui: &mut egui::Ui, mut job: egui::text::LayoutJob, viewport_width: f32) {
    job.halign = egui::Align::RIGHT;
    let galley = ui.fonts_mut(|fonts| fonts.layout_job(job));
    // Grow for tall wrapped text or wide unwrapped text, preserving scrolling
    // instead of clipping or widening the settings card.
    let content = widget_ids::allocate_exact_rect(
        ui,
        egui::vec2(
            viewport_width.max(galley.size().x),
            96.0_f32.max(galley.size().y),
        ),
    );
    let text = egui::Rect::from_min_size(
        egui::pos2(
            content.right() - galley.size().x,
            content.center().y - galley.size().y / 2.0,
        ),
        galley.size(),
    );
    let response = widget_ids::rect_scope_with_layout(
        ui,
        text,
        "settings.preview.quote",
        egui::Layout::top_down(egui::Align::LEFT),
        |ui| ui.label(galley),
    );
    record_settings_control_box("preview.quote.content", content);
    record_settings_control_box("preview.quote.text", response.inner.rect);
}

fn tab_preview_text(style: IndentationStyle, display: TabDisplayMode, columns: u8) -> String {
    let indent = match (style, display) {
        (IndentationStyle::Spaces, _) => " ".repeat(usize::from(columns)),
        (_, TabDisplayMode::Hidden) => "\t".to_owned(),
        (_, TabDisplayMode::Character) => {
            format!("→{}", " ".repeat(usize::from(columns.saturating_sub(1))))
        }
        (_, TabDisplayMode::Tablines) => {
            format!("│{}", " ".repeat(usize::from(columns.saturating_sub(1))))
        }
    };
    format!("Project notes\n{indent}A small idea\n{indent}Something to come back to")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tab_markers_only_represent_actual_tabs() {
        for mode in TabDisplayMode::ALL {
            assert!(
                tab_preview_text(IndentationStyle::Spaces, mode, 4).contains("\n    A small idea")
            );
        }
        assert!(
            tab_preview_text(IndentationStyle::TabCharacter, TabDisplayMode::Character, 4)
                .contains("\n→   A small idea")
        );
        assert!(
            tab_preview_text(IndentationStyle::TabCharacter, TabDisplayMode::Tablines, 2)
                .contains("\n│ A small idea")
        );
    }
}
