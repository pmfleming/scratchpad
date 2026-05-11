use super::*;

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

pub(in crate::app::ui::settings) fn clickable_card_header(
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

pub(in crate::app::ui::settings) fn card_header_with_trailing_width(
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
