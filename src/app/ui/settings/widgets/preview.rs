use super::*;
use crate::app::ui::editor_content::EditorHighlightStyle;
use std::path::Path;

pub(in crate::app::ui::settings) fn settings_file_card(
    ui: &mut egui::Ui,
    icon: &str,
    title: &str,
    description: &str,
    app: &mut ScratchpadApp,
) {
    let mut clicked = false;
    let settings_path = app.settings_path();
    let settings_path_text = settings_path.display().to_string();
    let settings_path_tail = path_tail_text(settings_path, 3);

    settings_card_frame(ui, |ui| {
        let group_width = SettingsUi::header_trailing_width(ui).min(ui.available_width());
        card_header_with_trailing_width(ui, icon, title, Some(description), group_width, |ui| {
            let path_pill_width =
                (group_width - SettingsUi::CONTROLS.gap - SettingsUi::CONTROLS.icon_button_size)
                    .max(0.0);
            let group_rect = widget_ids::allocate_exact_rect(
                ui,
                egui::vec2(group_width, SettingsUi::LAYOUT.card_min_height),
            );
            record_settings_control_box("settings_file_card.trailing_group", group_rect);
            widget_ids::rect_scope_with_layout(
                ui,
                group_rect,
                ("settings_file_card.trailing_group", title),
                egui::Layout::right_to_left(egui::Align::Center),
                |ui| {
                    ui.set_width(group_width);
                    ui.set_max_width(group_width);
                    clicked = phosphor_button(
                        ui,
                        ("settings_file_card", title),
                        egui_phosphor::regular::FOLDER_OPEN,
                        egui::vec2(
                            SettingsUi::CONTROLS.icon_button_size,
                            SettingsUi::CONTROLS.icon_button_size,
                        ),
                        action_bg(ui),
                        action_hover_bg(ui),
                        "Open settings file",
                    )
                    .clicked();
                    ui.add_space(SettingsUi::CONTROLS.gap);
                    let path_response = value_pill(
                        ui,
                        &settings_path_tail,
                        Some(&settings_path_text),
                        path_pill_width,
                    );
                    record_settings_control_box("settings_file_card.path_pill", path_response.rect);
                },
            );
        });
    });

    if clicked {
        app.open_settings_file_tab();
    }
}

pub(in crate::app::ui::settings) fn action_card(
    ui: &mut egui::Ui,
    icon: &str,
    title: &str,
    description: &str,
    action_tooltip: &str,
    on_click: impl FnOnce(&mut ScratchpadApp),
    app: &mut ScratchpadApp,
) {
    let mut clicked = false;
    settings_card_frame(ui, |ui| {
        card_header(ui, icon, title, Some(description), |ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let response = phosphor_button(
                    ui,
                    ("settings_action_card", title),
                    icon,
                    egui::vec2(
                        SettingsUi::CONTROLS.icon_button_size,
                        SettingsUi::CONTROLS.icon_button_size,
                    ),
                    action_bg(ui),
                    action_hover_bg(ui),
                    action_tooltip,
                );
                record_settings_control_box(format!("action_card.{title}"), response.rect);
                clicked = response.clicked();
            });
        });
    });

    if clicked {
        on_click(app);
    }
}

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
                SettingsUi::preview_frame(ui, app.editor_background_color()).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.add_space(4.0);
                    let preview_family = egui::FontFamily::Name(EDITOR_FONT_FAMILY.into());
                    render_preview_text(ui, app, preview_family);
                    ui.add_space(16.0);
                    ui.horizontal_wrapped(|ui| {
                        info_chip(ui, app.editor_font().label());
                        ui.add_space(8.0);
                        info_chip(ui, &format!("{:.0} pt", app.font_size()));
                        ui.add_space(8.0);
                        info_chip(ui, &format!("{} px gutter", app.editor_gutter()));
                    });
                });
            },
        );
    });
}

fn render_preview_text(ui: &mut egui::Ui, app: &ScratchpadApp, preview_family: egui::FontFamily) {
    let (text, highlighted_text) =
        crate::app::ui::settings::PREVIEW_QUOTES[app.settings_preview_quote_index];
    let mut job = egui::text::LayoutJob::default();
    job.wrap.max_width = ui.available_width();

    let base_format = egui::TextFormat {
        font_id: egui::FontId::new(app.font_size(), preview_family.clone()),
        color: app.editor_text_color(),
        ..Default::default()
    };
    let highlight_format = EditorHighlightStyle::new(
        app.editor_text_highlight_color(),
        app.editor_text_highlight_text_color(),
    )
    .active_text_format(
        egui::FontId::new(app.font_size(), preview_family),
        ui.visuals().dark_mode,
    );

    let start = text.find(highlighted_text).unwrap_or(0);
    let end = start + highlighted_text.len();
    job.append(&text[..start], 0.0, base_format.clone());
    job.append(&text[start..end], 0.0, highlight_format);
    job.append(&text[end..], 0.0, base_format);

    ui.label(job);
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

fn value_pill(
    ui: &mut egui::Ui,
    text: &str,
    hover_text: Option<&str>,
    width: f32,
) -> egui::Response {
    let margin = SettingsUi::MARGINS.value_pill_inner;
    let outer_width = width.max(0.0);
    let outer_height = ui.spacing().interact_size.y + (margin.top as f32) + (margin.bottom as f32);
    let response = widget_ids::allocate_exact_rect_interact(
        ui,
        egui::vec2(outer_width, outer_height),
        ("settings.value_pill", text, widget_ids::WidgetRole::Label),
        egui::Sense::hover(),
        "settings_value_pill",
    );

    ui.painter().rect(
        response.rect,
        egui::CornerRadius::same(8),
        SettingsUi::control_bg(ui),
        egui::Stroke::new(1.0, border(ui).gamma_multiply(0.75)),
        egui::StrokeKind::Inside,
    );

    let text_rect = response.rect.shrink2(egui::vec2(
        (margin.left + margin.right) as f32 * 0.5,
        (margin.top + margin.bottom) as f32 * 0.5,
    ));
    ui.painter().with_clip_rect(text_rect).text(
        text_rect.right_center(),
        egui::Align2::RIGHT_CENTER,
        text,
        egui::FontId::proportional(SettingsUi::TYPOGRAPHY.description),
        text_muted(ui),
    );
    if let Some(hover_text) = hover_text {
        return response.on_hover_text(hover_text);
    }
    response
}

fn path_tail_text(path: &Path, component_count: usize) -> String {
    let components: Vec<String> = path
        .components()
        .filter_map(|component| match component {
            std::path::Component::Normal(part) => Some(part.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect();

    if components.len() <= component_count {
        return path.display().to_string();
    }

    let separator = std::path::MAIN_SEPARATOR.to_string();
    let tail = components[components.len() - component_count..].join(&separator);
    format!("...{separator}{tail}")
}
