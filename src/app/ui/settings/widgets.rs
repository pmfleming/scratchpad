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
            ui.label(
                egui::RichText::new(chevron)
                    .size(18.0)
                    .color(SettingsUi::icon_color(ui)),
            );
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
            toggle_control(ui, &mut next_value);
        });
    });

    if next_value != current_value {
        on_change(next_value);
    }
}

pub(super) fn toggle_control(ui: &mut egui::Ui, value: &mut bool) {
    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
        let response = toggle_switch(ui, value);
        ui.add_space(12.0);
        ui.label(egui::RichText::new(if *value { "On" } else { "Off" }).color(text_primary(ui)));
        if response.changed() {
            ui.ctx().request_repaint();
        }
    });
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
                ui.set_width(SettingsUi::CONTROLS.width);
                value_pill(ui, &settings_path);
                ui.add_space(SettingsUi::CONTROLS.gap);
                clicked = phosphor_button(
                    ui,
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
                egui::vec2(
                    SettingsUi::CONTROLS.icon_button_size,
                    SettingsUi::CONTROLS.icon_button_size,
                ),
                action_bg(ui),
                action_hover_bg(ui),
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
    ui.with_layout(egui::Layout::left_to_right(egui::Align::TOP), |ui| {
        ui.set_min_height(SettingsUi::LAYOUT.inner_row_height);
        ui.add_space(40.0);
        let label_width = SettingsUi::row_label_width(ui);
        ui.allocate_ui_with_layout(
            egui::vec2(label_width, SettingsUi::LAYOUT.inner_row_height),
            egui::Layout::top_down(egui::Align::LEFT),
            |ui| {
                ui.set_width(label_width);
                ui.label(egui::RichText::new(label).color(text_primary(ui)));
                if let Some(description) = description {
                    ui.add_space(2.0);
                    ui.label(
                        egui::RichText::new(description)
                            .size(SettingsUi::TYPOGRAPHY.description)
                            .color(text_muted(ui)),
                    );
                }
            },
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
            ui.set_min_height(SettingsUi::LAYOUT.inner_row_height);
            add_control(ui);
        });
    });
}

pub(super) fn fixed_width_control(
    ui: &mut egui::Ui,
    add_control: impl FnOnce(&mut egui::Ui),
) {
    ui.allocate_ui(egui::vec2(SettingsUi::CONTROLS.width, 0.0), |ui| {
        ui.set_width(SettingsUi::CONTROLS.width);
        ui.set_max_width(SettingsUi::CONTROLS.width);
        add_control(ui);
    });
}

pub(super) fn inner_divider(ui: &mut egui::Ui) {
    let width = SettingsUi::divider_width(ui);
    ui.horizontal(|ui| {
        ui.add_space(40.0);
        let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 1.0), egui::Sense::hover());
        ui.painter()
            .rect_filled(rect, 0.0, SettingsUi::card_border(ui).gamma_multiply(0.7));
    });
}

pub(super) fn radio_option_row(
    ui: &mut egui::Ui,
    value: &mut bool,
    label: &str,
) -> egui::Response {
    ui.add_space(2.0);
    ui.add(egui::RadioButton::new(*value, label))
}

pub(super) fn render_preview_panel(ui: &mut egui::Ui, app: &ScratchpadApp) {
    let preview_width = SettingsUi::preview_width(ui);
    ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
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
                    ui.add_sized(
                        egui::vec2(ui.available_width(), 0.0),
                        egui::Label::new(
                            egui::RichText::new(SettingsUi::PREVIEW_TEXT)
                                .family(preview_family)
                                .size(app.font_size())
                                .color(app.editor_text_color()),
                        )
                        .wrap(),
                    );
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

pub(super) fn settings_card_frame(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui)) {
    let card_width = SettingsUi::card_width(ui);
    ui.set_width(card_width);
    ui.set_max_width(card_width);
    SettingsUi::card_frame(ui).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.set_max_width(ui.available_width());
        add_contents(ui);
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
    ui.interact(inner.response.rect, id, egui::Sense::click())
}

pub(super) fn card_header(
    ui: &mut egui::Ui,
    icon: &str,
    title: &str,
    description: Option<&str>,
    add_trailing: impl FnOnce(&mut egui::Ui),
) -> egui::InnerResponse<()> {
    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.set_min_height(SettingsUi::LAYOUT.card_min_height);
        icon_slot(ui, icon);
        ui.add_space(12.0);
        let header_width = SettingsUi::header_text_width(ui);
        ui.allocate_ui_with_layout(
            egui::vec2(header_width, SettingsUi::LAYOUT.card_min_height),
            egui::Layout::top_down(egui::Align::LEFT).with_main_align(egui::Align::Center),
            |ui| {
                ui.set_width(header_width);
                ui.label(egui::RichText::new(title).strong().color(text_primary(ui)));
                if let Some(description) = description {
                    ui.add_space(2.0);
                    ui.label(
                        egui::RichText::new(description)
                            .size(SettingsUi::TYPOGRAPHY.description)
                            .color(text_muted(ui)),
                    );
                }
            },
        );
        let trailing_width = ui.available_width().max(0.0);
        ui.allocate_ui_with_layout(
            egui::vec2(trailing_width, SettingsUi::LAYOUT.card_min_height),
            egui::Layout::right_to_left(egui::Align::Center),
            |ui| {
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
        SettingsUi::accent()
    } else {
        SettingsUi::control_bg(ui)
    };

    ui.painter().rect(
        rect,
        radius,
        track_fill,
        egui::Stroke::new(1.0, SettingsUi::card_border(ui)),
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
        .fill(action_hover_bg(ui).gamma_multiply(0.72))
        .stroke(egui::Stroke::new(1.0, border(ui).gamma_multiply(0.7)))
        .corner_radius(egui::CornerRadius::same(127))
        .inner_margin(SettingsUi::MARGINS.info_chip_inner)
        .show(ui, |ui| {
            ui.with_layout(
                egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
                |ui| {
                    ui.label(
                        egui::RichText::new(text)
                            .size(SettingsUi::TYPOGRAPHY.description)
                            .color(text_muted(ui)),
                    );
                },
            );
        });
}

fn value_pill(ui: &mut egui::Ui, text: &str) {
    egui::Frame::new()
        .fill(SettingsUi::control_bg(ui))
        .stroke(egui::Stroke::new(1.0, border(ui).gamma_multiply(0.75)))
        .corner_radius(egui::CornerRadius::same(8))
        .inner_margin(SettingsUi::MARGINS.value_pill_inner)
        .show(ui, |ui| {
            let width = SettingsUi::pill_width();
            ui.set_width(width);
            ui.set_max_width(width);
            ui.set_min_height(ui.spacing().interact_size.y);
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.add_sized(
                    egui::vec2(width, 0.0),
                    egui::Label::new(
                        egui::RichText::new(text)
                            .size(SettingsUi::TYPOGRAPHY.description)
                            .color(text_muted(ui)),
                    )
                    .truncate(),
                );
            });
        });
}
