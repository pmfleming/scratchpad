use super::*;

pub(super) fn expandable_card(
    ui: &mut egui::Ui,
    id_source: &str,
    icon: &str,
    title: &str,
    description: &str,
    default_open: bool,
    add_body: impl FnOnce(&mut egui::Ui),
) {
    let id = ui.make_persistent_id(id_source);
    let is_open = ui
        .data_mut(|data| data.get_persisted::<bool>(id))
        .unwrap_or(default_open);

    settings_card_frame(ui, |ui| {
        let response = clickable_card_header(ui, id, icon, title, Some(description), |ui| {
            let chevron = if is_open {
                egui_phosphor::regular::CARET_UP
            } else {
                egui_phosphor::regular::CARET_DOWN
            };
            ui.label(egui::RichText::new(chevron).size(18.0).color(SETTINGS_ICON));
        });

        if response.clicked() {
            ui.data_mut(|data| data.insert_persisted(id, !is_open));
        }

        if is_open {
            inner_divider(ui);
            ui.add_space(4.0);
            add_body(ui);
        }
    });
}

pub(super) fn toggle_card(
    ui: &mut egui::Ui,
    icon: &str,
    title: &str,
    description: &str,
    current_value: bool,
    on_change: impl FnOnce(bool),
) {
    let mut next_value = current_value;
    settings_card_frame(ui, |ui| {
        card_header(ui, icon, title, Some(description), |ui| {
            ui.horizontal(|ui| {
                let response = toggle_switch(ui, &mut next_value);
                ui.add_space(12.0);
                ui.label(
                    egui::RichText::new(if next_value { "On" } else { "Off" }).color(TEXT_PRIMARY),
                );
                if response.changed() {
                    ui.ctx().request_repaint();
                }
            });
        });
    });

    if next_value != current_value {
        on_change(next_value);
    }
}

pub(super) fn settings_file_card(
    ui: &mut egui::Ui,
    icon: &str,
    title: &str,
    description: &str,
    app: &mut ScratchpadApp,
) {
    let mut clicked = false;
    let settings_path = app.settings_path().display().to_string();

    settings_card_frame(ui, |ui| {
        card_header(ui, icon, title, Some(description), |ui| {
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.set_width(SETTINGS_CONTROL_WIDTH);
                value_pill(ui, &settings_path);
                ui.add_space(SETTINGS_CONTROL_GAP);
                clicked = phosphor_button(
                    ui,
                    egui_phosphor::regular::FOLDER_OPEN,
                    egui::vec2(SETTINGS_ICON_BUTTON_SIZE, SETTINGS_ICON_BUTTON_SIZE),
                    ACTION_BG,
                    ACTION_HOVER_BG,
                    "Open settings file",
                )
                .clicked();
            });
        });
    });

    if clicked {
        app.open_settings_file_tab();
    }
}

pub(super) fn action_card(
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
            clicked = phosphor_button(
                ui,
                icon,
                egui::vec2(SETTINGS_ICON_BUTTON_SIZE, SETTINGS_ICON_BUTTON_SIZE),
                ACTION_BG,
                ACTION_HOVER_BG,
                action_tooltip,
            )
            .clicked();
        });
    });

    if clicked {
        on_click(app);
    }
}

pub(super) fn inner_select_row(
    ui: &mut egui::Ui,
    label: &str,
    description: Option<&str>,
    add_control: impl FnOnce(&mut egui::Ui),
) {
    ui.horizontal(|ui| {
        ui.set_min_height(SETTINGS_INNER_ROW_HEIGHT);
        ui.add_space(40.0);
        ui.vertical(|ui| {
            ui.set_width((ui.available_width() - 250.0).max(180.0));
            ui.label(egui::RichText::new(label).color(TEXT_PRIMARY));
            if let Some(description) = description {
                ui.add_space(2.0);
                ui.label(
                    egui::RichText::new(description)
                        .size(SETTINGS_DESCRIPTION_FONT_SIZE)
                        .color(TEXT_MUTED),
                );
            }
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            add_control(ui);
        });
    });
}

pub(super) fn inner_divider(ui: &mut egui::Ui) {
    let width = (ui.available_width() - 40.0).max(0.0);
    ui.horizontal(|ui| {
        ui.add_space(40.0);
        let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 1.0), egui::Sense::hover());
        ui.painter()
            .rect_filled(rect, 0.0, SETTINGS_CARD_BORDER.gamma_multiply(0.7));
    });
}

pub(super) fn render_preview_panel(ui: &mut egui::Ui, app: &ScratchpadApp) {
    egui::Frame::new()
        .fill(SETTINGS_PREVIEW_BG.gamma_multiply(0.96))
        .stroke(egui::Stroke::new(1.0, SETTINGS_CARD_BORDER))
        .corner_radius(egui::CornerRadius::same(SETTINGS_CARD_RADIUS))
        .inner_margin(egui::Margin {
            left: 20,
            right: 20,
            top: 20,
            bottom: 20,
        })
        .show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(4.0);
                let preview_family = egui::FontFamily::Name(EDITOR_FONT_FAMILY.into());
                ui.label(
                    egui::RichText::new(SETTINGS_PREVIEW_TEXT)
                        .family(preview_family)
                        .size(app.font_size())
                        .color(TEXT_PRIMARY),
                );
                ui.add_space(16.0);
                ui.horizontal_centered(|ui| {
                    info_chip(ui, app.editor_font().label());
                    ui.add_space(8.0);
                    info_chip(ui, &format!("{:.0} pt", app.font_size()));
                    ui.add_space(8.0);
                    info_chip(ui, &format!("{} px gutter", app.editor_gutter()));
                });
            });
        });
}

fn settings_card_frame(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(SETTINGS_CARD_BG)
        .stroke(egui::Stroke::new(1.0, SETTINGS_CARD_BORDER))
        .corner_radius(egui::CornerRadius::same(SETTINGS_CARD_RADIUS))
        .inner_margin(egui::Margin {
            left: 18,
            right: 18,
            top: 14,
            bottom: 14,
        })
        .show(ui, add_contents);
}

fn clickable_card_header(
    ui: &mut egui::Ui,
    id: egui::Id,
    icon: &str,
    title: &str,
    description: Option<&str>,
    add_trailing: impl FnOnce(&mut egui::Ui),
) -> egui::Response {
    let inner = card_header(ui, icon, title, description, add_trailing);
    ui.interact(inner.response.rect, id, egui::Sense::click())
}

fn card_header(
    ui: &mut egui::Ui,
    icon: &str,
    title: &str,
    description: Option<&str>,
    add_trailing: impl FnOnce(&mut egui::Ui),
) -> egui::InnerResponse<()> {
    ui.horizontal(|ui| {
        ui.set_min_height(SETTINGS_CARD_MIN_HEIGHT);
        icon_slot(ui, icon);
        ui.add_space(12.0);
        ui.vertical(|ui| {
            ui.set_width((ui.available_width() - 240.0).max(220.0));
            ui.label(egui::RichText::new(title).strong().color(TEXT_PRIMARY));
            if let Some(description) = description {
                ui.add_space(2.0);
                ui.label(
                    egui::RichText::new(description)
                        .size(SETTINGS_DESCRIPTION_FONT_SIZE)
                        .color(TEXT_MUTED),
                );
            }
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            add_trailing(ui);
        });
    })
}

fn icon_slot(ui: &mut egui::Ui, icon: &str) {
    ui.allocate_ui(egui::vec2(28.0, 28.0), |ui| {
        ui.with_layout(
            egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
            |ui| {
                ui.label(egui::RichText::new(icon).size(18.0).color(SETTINGS_ICON));
            },
        );
    });
}

fn toggle_switch(ui: &mut egui::Ui, value: &mut bool) -> egui::Response {
    let desired_size = egui::vec2(42.0, 22.0);
    let (rect, mut response) = ui.allocate_exact_size(desired_size, egui::Sense::click());
    if response.clicked() {
        *value = !*value;
        response.mark_changed();
    }

    let how_on = ui.ctx().animate_bool(response.id, *value);
    let radius = rect.height() * 0.5;
    let track_fill = if *value {
        SETTINGS_ACCENT
    } else {
        SETTINGS_CONTROL_BG
    };

    ui.painter().rect(
        rect,
        radius,
        track_fill,
        egui::Stroke::new(1.0, SETTINGS_CARD_BORDER),
        egui::StrokeKind::Inside,
    );

    let thumb_x = egui::lerp((rect.left() + radius)..=(rect.right() - radius), how_on);
    ui.painter().circle_filled(
        egui::pos2(thumb_x, rect.center().y),
        radius - 3.0,
        egui::Color32::WHITE,
    );

    response
}

fn info_chip(ui: &mut egui::Ui, text: &str) {
    egui::Frame::new()
        .fill(ACTION_HOVER_BG.gamma_multiply(0.72))
        .stroke(egui::Stroke::new(1.0, BORDER.gamma_multiply(0.7)))
        .corner_radius(egui::CornerRadius::same(127))
        .inner_margin(egui::Margin {
            left: 10,
            right: 10,
            top: 5,
            bottom: 5,
        })
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(text)
                    .size(SETTINGS_DESCRIPTION_FONT_SIZE)
                    .color(TEXT_MUTED),
            );
        });
}

fn value_pill(ui: &mut egui::Ui, text: &str) {
    egui::Frame::new()
        .fill(SETTINGS_CONTROL_BG)
        .stroke(egui::Stroke::new(1.0, BORDER.gamma_multiply(0.75)))
        .corner_radius(egui::CornerRadius::same(8))
        .inner_margin(egui::Margin {
            left: 12,
            right: 12,
            top: 8,
            bottom: 8,
        })
        .show(ui, |ui| {
            ui.set_width(SETTINGS_PILL_WIDTH);
            ui.set_max_width(SETTINGS_PILL_WIDTH);
            ui.label(
                egui::RichText::new(text)
                    .size(SETTINGS_DESCRIPTION_FONT_SIZE)
                    .color(TEXT_MUTED),
            );
        });
}
