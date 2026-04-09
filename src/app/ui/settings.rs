use crate::app::app_state::ScratchpadApp;
use crate::app::fonts::{EDITOR_FONT_FAMILY, EditorFontPreset};
use crate::app::services::file_controller::FileController;
use crate::app::services::settings_store::AppSettings;
use crate::app::theme::{ACTION_BG, BORDER, EDITOR_BG, TAB_ACTIVE_BG, TEXT_MUTED, TEXT_PRIMARY};
use eframe::egui;

const SETTINGS_PAGE_MAX_WIDTH: f32 = 980.0;
const SETTINGS_CARD_RADIUS: u8 = 10;
const SETTINGS_FONT_SIZE: f32 = 15.0;
const SETTINGS_ROW_HEIGHT: f32 = 50.0;
const SETTINGS_CONTROL_WIDTH: f32 = 220.0;
const SETTINGS_PREVIEW_TEXT: &str = "The sound of ocean waves calms my soul.";
const FONT_SIZE_OPTIONS: [u32; 9] = [11, 12, 14, 16, 18, 20, 24, 28, 32];

pub(crate) fn show_page(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    egui::CentralPanel::default().show_inside(ui, |ui| {
        ui.scope(|ui| {
            apply_settings_typography(ui);

            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let available_width = ui.available_width();
                    let content_width = available_width.min(SETTINGS_PAGE_MAX_WIDTH);
                    let horizontal_margin = ((available_width - content_width) * 0.5).max(24.0);

                    ui.add_space(18.0);
                    ui.horizontal(|ui| {
                        ui.add_space(horizontal_margin);
                        ui.vertical(|ui| {
                            ui.set_width(content_width);
                            render_page_heading(ui, app);
                            ui.add_space(16.0);
                            render_font_section(ui, app);
                            ui.add_space(12.0);
                            render_diagnostics_section(ui, app);
                            ui.add_space(12.0);
                            render_settings_section(ui, app);
                        });
                    });
                    ui.add_space(24.0);
                });
        });
    });
}

fn apply_settings_typography(ui: &mut egui::Ui) {
    let font_id = egui::FontId::proportional(SETTINGS_FONT_SIZE);
    let style = ui.style_mut();
    style.override_font_id = Some(font_id.clone());
    style
        .text_styles
        .insert(egui::TextStyle::Heading, font_id.clone());
    style
        .text_styles
        .insert(egui::TextStyle::Body, font_id.clone());
    style
        .text_styles
        .insert(egui::TextStyle::Button, font_id.clone());
    style.text_styles.insert(egui::TextStyle::Small, font_id);
}

fn render_page_heading(ui: &mut egui::Ui, app: &ScratchpadApp) {
    ui.label(egui::RichText::new("Settings").strong());
    ui.add_space(8.0);
    info_chip(
        ui,
        &format!("Settings file: {}", app.settings_path().display()),
    );
}

fn render_font_section(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    settings_section(
        ui,
        "Font",
        "Choose the text appearance for the editor.",
        true,
        |ui| {
            render_font_family_row(ui, app);
            render_divider(ui);
            render_font_size_row(ui, app);
            render_divider(ui);
            render_word_wrap_row(ui, app);
            render_preview_panel(ui, app);
        },
    );
}

fn render_diagnostics_section(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    settings_section(
        ui,
        "Diagnostics",
        "Control runtime behavior while the app is open.",
        true,
        |ui| {
            render_logging_row(ui, app);
        },
    );
}

fn render_settings_section(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    settings_section(
        ui,
        "Settings",
        "Manage the stored settings for this workspace.",
        true,
        |ui| {
            render_settings_path_row(ui, app);
            render_divider(ui);
            render_settings_path_actions(ui, app);
            render_divider(ui);
            render_reset_row(ui, app);
        },
    );
}

fn settings_section(
    ui: &mut egui::Ui,
    title: &str,
    description: &str,
    default_open: bool,
    add_contents: impl FnOnce(&mut egui::Ui),
) {
    egui::Frame::new()
        .fill(TAB_ACTIVE_BG.gamma_multiply(0.72))
        .stroke(egui::Stroke::new(1.0, BORDER.gamma_multiply(0.85)))
        .corner_radius(egui::CornerRadius::same(SETTINGS_CARD_RADIUS))
        .inner_margin(egui::Margin::same(0))
        .show(ui, |ui| {
            egui::CollapsingHeader::new(
                egui::RichText::new(format!("{title}\n{description}")).strong(),
            )
            .default_open(default_open)
            .show(ui, |ui| {
                ui.add_space(8.0);
                add_contents(ui);
                ui.add_space(10.0);
            });
        });
}

fn render_font_family_row(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    settings_row(
        ui,
        "Family",
        Some("Pick the bundled font used for editor content."),
        |ui| {
            let mut selected_font = app.editor_font();
            egui::ComboBox::from_id_salt("settings_editor_font")
                .selected_text(selected_font.label())
                .width(SETTINGS_CONTROL_WIDTH)
                .show_ui(ui, |ui| {
                    for preset in EditorFontPreset::ALL {
                        ui.selectable_value(&mut selected_font, preset, preset.label());
                    }
                });
            if selected_font != app.editor_font() {
                app.set_editor_font(selected_font);
            }
        },
    );
}

fn render_font_size_row(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    settings_row(ui, "Size", Some("Adjust the editor text size."), |ui| {
        let mut selected_size = app.font_size().round() as u32;
        egui::ComboBox::from_id_salt("settings_font_size")
            .selected_text(selected_size.to_string())
            .width(SETTINGS_CONTROL_WIDTH)
            .show_ui(ui, |ui| {
                for option in FONT_SIZE_OPTIONS {
                    ui.selectable_value(&mut selected_size, option, option.to_string());
                }
            });
        let selected_size = selected_size as f32;
        if (selected_size - app.font_size()).abs() > f32::EPSILON {
            app.set_font_size(selected_size);
        }
    });
}

fn render_word_wrap_row(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    settings_row(
        ui,
        "Word wrap",
        Some("Choose how long lines behave in the editor."),
        |ui| {
            let mut word_wrap = app.word_wrap();
            segmented_toggle(ui, &mut word_wrap, [("Off", false), ("On", true)]);
            if word_wrap != app.word_wrap() {
                app.set_word_wrap(word_wrap);
            }
        },
    );
}

fn render_preview_panel(ui: &mut egui::Ui, app: &ScratchpadApp) {
    ui.add_space(12.0);
    ui.horizontal(|ui| {
        ui.add_space(18.0);
        info_chip(ui, app.editor_font().label());
        ui.add_space(8.0);
        info_chip(ui, &format!("{:.0} pt", app.font_size()));
        ui.add_space(8.0);
        info_chip(
            ui,
            if app.word_wrap() {
                "Wrap on"
            } else {
                "Wrap off"
            },
        );
    });
    ui.add_space(10.0);
    egui::Frame::new()
        .fill(EDITOR_BG.gamma_multiply(0.95))
        .stroke(egui::Stroke::new(1.0, BORDER.gamma_multiply(0.9)))
        .corner_radius(egui::CornerRadius::same(SETTINGS_CARD_RADIUS))
        .show(ui, |ui| {
            ui.add_space(20.0);
            ui.vertical_centered(|ui| {
                let preview_family = egui::FontFamily::Name(EDITOR_FONT_FAMILY.into());
                ui.label(
                    egui::RichText::new(SETTINGS_PREVIEW_TEXT)
                        .family(preview_family)
                        .size(app.font_size())
                        .color(TEXT_PRIMARY),
                );
            });
            ui.add_space(20.0);
        });
}

fn render_logging_row(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    settings_row(
        ui,
        "File logging",
        Some("Write runtime diagnostics while the app is running."),
        |ui| {
            let mut logging_enabled = app.logging_enabled();
            segmented_toggle(ui, &mut logging_enabled, [("On", true), ("Off", false)]);
            if logging_enabled != app.logging_enabled() {
                app.set_logging_enabled(logging_enabled);
            }
        },
    );
}

fn render_settings_path_row(ui: &mut egui::Ui, app: &ScratchpadApp) {
    settings_row(
        ui,
        "Location",
        Some("Stored as YAML and loaded before session restore."),
        |ui| {
            value_pill(
                ui,
                &app.settings_path().display().to_string(),
                SETTINGS_CONTROL_WIDTH,
            );
        },
    );
}

fn render_reset_row(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    settings_row(
        ui,
        "Reset",
        Some("Restore the current settings file to app defaults."),
        |ui| {
            let button = egui::Button::new("Reset to defaults")
                .min_size(egui::vec2(SETTINGS_CONTROL_WIDTH, 28.0))
                .fill(ACTION_BG)
                .stroke(egui::Stroke::new(1.0, BORDER));
            if ui.add(button).clicked() {
                app.reset_settings_to_defaults();
            }
        },
    );
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.add_space(18.0);
        info_chip(ui, AppSettings::default().editor_font.label());
        ui.add_space(8.0);
        info_chip(ui, &format!("{:.0} pt", AppSettings::default().font_size));
        ui.add_space(8.0);
        info_chip(
            ui,
            if AppSettings::default().word_wrap {
                "Wrap on"
            } else {
                "Wrap off"
            },
        );
    });
}

fn render_settings_path_actions(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    ui.add_space(8.0);
    settings_row(
        ui,
        "Open",
        Some("Open the YAML settings file in a new editor tab."),
        |ui| {
            let button = egui::Button::new("Open in new tab")
                .min_size(egui::vec2(SETTINGS_CONTROL_WIDTH, 28.0))
                .fill(ACTION_BG)
                .stroke(egui::Stroke::new(1.0, BORDER));
            if ui.add(button).clicked() {
                let path = app.settings_path();
                app.close_settings();
                FileController::open_external_paths(app, vec![path]);
            }
        },
    );
}

fn settings_row(
    ui: &mut egui::Ui,
    label: &str,
    description: Option<&str>,
    add_control: impl FnOnce(&mut egui::Ui),
) {
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.set_min_height(SETTINGS_ROW_HEIGHT);
        ui.add_space(18.0);
        ui.vertical(|ui| {
            ui.set_width((ui.available_width() - 280.0).max(220.0));
            ui.label(egui::RichText::new(label).strong());
            if let Some(description) = description {
                ui.label(egui::RichText::new(description).color(TEXT_MUTED));
            }
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_space(18.0);
            add_control(ui);
        });
        ui.add_space(18.0);
    });
    ui.add_space(4.0);
}

fn render_divider(ui: &mut egui::Ui) {
    let divider_width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(divider_width, 1.0), egui::Sense::hover());
    ui.painter()
        .rect_filled(rect, 0.0, BORDER.gamma_multiply(0.45));
}

fn segmented_toggle<T: Copy + PartialEq, const N: usize>(
    ui: &mut egui::Ui,
    value: &mut T,
    options: [(&str, T); N],
) {
    egui::Frame::new()
        .fill(ACTION_BG.gamma_multiply(0.7))
        .stroke(egui::Stroke::new(1.0, BORDER.gamma_multiply(0.8)))
        .corner_radius(egui::CornerRadius::same(9))
        .inner_margin(egui::Margin::same(4))
        .show(ui, |ui| {
            ui.set_min_width(SETTINGS_CONTROL_WIDTH);
            ui.set_max_width(SETTINGS_CONTROL_WIDTH);
            let button_width = ((SETTINGS_CONTROL_WIDTH - 8.0) / N as f32).max(54.0);
            ui.horizontal(|ui| {
                for (label, option_value) in options {
                    let selected = *value == option_value;
                    let button = egui::Button::new(label)
                        .min_size(egui::vec2(button_width, 28.0))
                        .fill(if selected {
                            TAB_ACTIVE_BG
                        } else {
                            egui::Color32::TRANSPARENT
                        })
                        .stroke(egui::Stroke::new(
                            1.0,
                            if selected {
                                BORDER
                            } else {
                                egui::Color32::TRANSPARENT
                            },
                        ));
                    if ui.add(button).clicked() {
                        *value = option_value;
                    }
                }
            });
        });
}

fn info_chip(ui: &mut egui::Ui, text: &str) {
    egui::Frame::new()
        .fill(ACTION_BG.gamma_multiply(0.72))
        .stroke(egui::Stroke::new(1.0, BORDER.gamma_multiply(0.7)))
        .corner_radius(egui::CornerRadius::same(127))
        .inner_margin(egui::Margin {
            left: 10,
            right: 10,
            top: 5,
            bottom: 5,
        })
        .show(ui, |ui| {
            ui.label(egui::RichText::new(text).color(TEXT_MUTED));
        });
}

fn value_pill(ui: &mut egui::Ui, text: &str, width: f32) {
    egui::Frame::new()
        .fill(ACTION_BG.gamma_multiply(0.72))
        .stroke(egui::Stroke::new(1.0, BORDER.gamma_multiply(0.75)))
        .corner_radius(egui::CornerRadius::same(8))
        .inner_margin(egui::Margin {
            left: 12,
            right: 12,
            top: 8,
            bottom: 8,
        })
        .show(ui, |ui| {
            ui.set_max_width(width);
            ui.label(egui::RichText::new(text).color(TEXT_MUTED));
        });
}
