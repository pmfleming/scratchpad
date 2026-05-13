use super::layout_state::record_settings_card_box;
use super::record_settings_control_box;
use crate::app::ui::editor_content::EditorHighlightStyle;
use crate::app::ui::settings::{
    EDITOR_FONT_FAMILY, ScratchpadApp, SettingsUi, action_hover_bg, border, egui, text_muted,
    text_primary,
};
use crate::app::ui::widget_ids;

pub(in crate::app::ui::settings) fn render_preview_panel(ui: &mut egui::Ui, app: &ScratchpadApp) {
    let preview_width = SettingsUi::preview_width(ui);
    ui.horizontal(|ui| {
        let leading_space = (ui.available_width() - preview_width).max(0.0);
        ui.add_space(leading_space);
        ui.allocate_ui_with_layout(
            egui::vec2(preview_width, 0.0),
            egui::Layout::top_down(egui::Align::LEFT),
            |ui| {
                ui.set_width(preview_width);
                ui.set_max_width(preview_width);
                SettingsUi::preview_frame(ui, app.state.app_settings.editor_background_color())
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.add_space(4.0);
                        let preview_family = egui::FontFamily::Name(EDITOR_FONT_FAMILY.into());
                        render_preview_text(ui, app, preview_family);
                        ui.add_space(16.0);
                        ui.horizontal_wrapped(|ui| {
                            info_chip(ui, app.state.app_settings.editor_font().label());
                            ui.add_space(8.0);
                            info_chip(ui, &format!("{:.0} pt", app.state.app_settings.font_size()));
                            ui.add_space(8.0);
                            info_chip(
                                ui,
                                &format!("{} px gutter", app.state.app_settings.editor_gutter()),
                            );
                        });
                    });
            },
        );
    });
}

fn render_preview_text(ui: &mut egui::Ui, app: &ScratchpadApp, preview_family: egui::FontFamily) {
    let (text, highlighted_text) =
        crate::app::ui::settings::PREVIEW_QUOTES[app.state.settings_preview_quote_index];
    let mut job = egui::text::LayoutJob::default();
    job.wrap.max_width = ui.available_width();

    let base_format = egui::TextFormat {
        font_id: egui::FontId::new(app.state.app_settings.font_size(), preview_family.clone()),
        color: app.state.app_settings.editor_text_color(),
        ..Default::default()
    };
    let highlight_format = EditorHighlightStyle::new(
        app.state.app_settings.editor_text_highlight_color(),
        app.state.app_settings.editor_text_highlight_text_color(),
    )
    .active_text_format(
        egui::FontId::new(app.state.app_settings.font_size(), preview_family),
        ui.visuals().dark_mode,
    );

    let start = text.find(highlighted_text).unwrap_or(0);
    let end = start + highlighted_text.len();
    job.append(&text[..start], 0.0, base_format.clone());
    job.append(&text[start..end], 0.0, highlight_format);
    job.append(&text[end..], 0.0, base_format);

    ui.label(job);
}

pub(in crate::app::ui::settings) fn settings_card_frame(
    ui: &mut egui::Ui,
    add_contents: impl FnOnce(&mut egui::Ui),
) {
    let card_width = SettingsUi::card_width(ui);
    let leading_inset = SettingsUi::card_leading_inset(ui);
    let stroke_width = SettingsUi::CARD_STROKE_WIDTH;
    let margin = SettingsUi::MARGINS.card_inner;
    let horizontal_inset = f32::from(margin.left + margin.right) + stroke_width * 2.0;
    let vertical_inset = f32::from(margin.top + margin.bottom) + stroke_width * 2.0;
    let card_content_width = (card_width - horizontal_inset).max(0.0);

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.add_space(leading_inset);
        let background_shape = ui.painter().add(egui::Shape::Noop);

        let response = ui.allocate_ui_with_layout(
            egui::vec2(card_width, 0.0),
            egui::Layout::top_down(egui::Align::LEFT),
            |ui| {
                ui.set_width(card_width);
                ui.set_max_width(card_width);
                ui.add_space(f32::from(margin.top) + stroke_width);
                ui.horizontal(|ui| {
                    ui.add_space(f32::from(margin.left) + stroke_width);
                    ui.allocate_ui_with_layout(
                        egui::vec2(card_content_width, 0.0),
                        egui::Layout::top_down(egui::Align::LEFT),
                        |ui| {
                            ui.set_width(card_content_width);
                            ui.set_max_width(card_content_width);
                            add_contents(ui);
                        },
                    );
                });
                ui.add_space(f32::from(margin.bottom) + stroke_width);
            },
        );

        let outer_rect = egui::Rect::from_min_size(
            response.response.rect.min,
            egui::vec2(
                card_width,
                response.response.rect.height().max(vertical_inset),
            ),
        );
        ui.painter().set(
            background_shape,
            egui::Shape::Rect(egui::epaint::RectShape::new(
                outer_rect,
                egui::CornerRadius::same(SettingsUi::LAYOUT.card_radius),
                SettingsUi::card_bg(ui),
                egui::Stroke::new(stroke_width, SettingsUi::card_border(ui)),
                egui::StrokeKind::Inside,
            )),
        );
        record_settings_card_box("settings_card_frame", outer_rect);
    });
}

pub(super) fn clickable_card_header(
    ui: &mut egui::Ui,
    id: egui::Id,
    icon: &str,
    title: &str,
    description: Option<&str>,
    add_trailing: impl FnOnce(&mut egui::Ui),
) -> egui::Response {
    let inner = card_header(ui, icon, title, description, add_trailing);
    widget_ids::interact(
        ui,
        inner.response.rect,
        widget_ids::child(id, widget_ids::WidgetRole::SettingsCardHeader),
        egui::Sense::click(),
        "settings_card_header",
    )
}

pub(in crate::app::ui::settings) fn card_header(
    ui: &mut egui::Ui,
    icon: &str,
    title: &str,
    description: Option<&str>,
    add_trailing: impl FnOnce(&mut egui::Ui),
) -> egui::InnerResponse<()> {
    let trailing_width = SettingsUi::header_trailing_width(ui);
    card_header_with_trailing_width(ui, icon, title, description, trailing_width, add_trailing)
}

pub(super) fn card_header_with_trailing_width(
    ui: &mut egui::Ui,
    icon: &str,
    title: &str,
    description: Option<&str>,
    trailing_width: f32,
    add_trailing: impl FnOnce(&mut egui::Ui),
) -> egui::InnerResponse<()> {
    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.set_min_height(SettingsUi::LAYOUT.card_min_height);
        icon_slot(ui, icon);
        ui.add_space(12.0);
        let header_width = SettingsUi::header_text_width_for_trailing(ui, trailing_width);
        let header_rect = widget_ids::allocate_exact_rect(
            ui,
            egui::vec2(header_width, SettingsUi::LAYOUT.card_min_height),
        );
        widget_ids::rect_scope_with_layout(
            ui,
            header_rect,
            ("settings.card_header.text", title),
            egui::Layout::top_down(egui::Align::LEFT).with_main_align(egui::Align::Center),
            |ui| {
                label_stack(
                    ui,
                    header_width,
                    egui::RichText::new(title).strong().color(text_primary(ui)),
                    description,
                );
            },
        );
        ui.add_space(SettingsUi::CONTROLS.gap);
        let trailing_rect = widget_ids::allocate_exact_rect(
            ui,
            egui::vec2(trailing_width, SettingsUi::LAYOUT.card_min_height),
        );
        record_settings_control_box(format!("card_header.{title}"), trailing_rect);
        widget_ids::rect_scope_with_layout(
            ui,
            trailing_rect,
            ("settings.card_header.trailing", title),
            egui::Layout::right_to_left(egui::Align::Center),
            |ui| {
                ui.set_width(trailing_width);
                ui.set_max_width(trailing_width);
                ui.set_min_height(SettingsUi::LAYOUT.card_min_height);
                add_trailing(ui);
            },
        );
    })
}

fn icon_slot(ui: &mut egui::Ui, icon: &str) {
    ui.allocate_ui(egui::vec2(28.0, 28.0), |ui| {
        ui.with_layout(
            egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
            |ui| {
                ui.label(
                    egui::RichText::new(icon)
                        .size(18.0)
                        .color(SettingsUi::icon_color(ui)),
                );
            },
        );
    });
}

pub(super) fn label_stack(
    ui: &mut egui::Ui,
    width: f32,
    title: egui::RichText,
    description: Option<&str>,
) {
    ui.set_width(width);
    ui.set_max_width(width);
    ui.label(title);
    if let Some(description) = description {
        ui.add_space(2.0);
        ui.label(
            egui::RichText::new(description)
                .size(SettingsUi::TYPOGRAPHY.description)
                .color(text_muted(ui)),
        );
    }
}

fn info_chip(ui: &mut egui::Ui, text: &str) {
    egui::Frame::new()
        .fill(action_hover_bg(ui).gamma_multiply(0.72))
        .stroke(egui::Stroke::new(1.0, border(ui).gamma_multiply(0.7)))
        .corner_radius(egui::CornerRadius::same(127))
        .inner_margin(SettingsUi::MARGINS.info_chip_inner)
        .show(ui, |ui| {
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.label(
                    egui::RichText::new(text)
                        .size(SettingsUi::TYPOGRAPHY.description)
                        .color(text_muted(ui)),
                );
            });
        });
}
