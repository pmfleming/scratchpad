use super::common::show_callout;
use crate::app::app_state::ScratchpadApp;
use crate::app::fonts::EDITOR_FONT_FAMILY;
use crate::app::services::file_controller::FileController;
use crate::app::services::file_service::COMMON_TEXT_ENCODINGS;
use crate::app::theme::{action_hover_bg, border, text_muted, text_primary};
use crate::app::ui::{callout, settings};
use eframe::egui;
use egui_phosphor::regular::{ARROW_COUNTER_CLOCKWISE, FILE_TEXT, FLOPPY_DISK, WARNING};

const ENCODING_DIALOG_SIZE: egui::Vec2 = egui::vec2(760.0, 516.0);
const ENCODING_DIALOG_CONTENT_WIDTH: f32 = 700.0;
const ENCODING_TITLE_SIZE: f32 = 24.0;
const ENCODING_CARD_MIN_HEIGHT: f32 = 84.0;
const ENCODING_CONTROL_WIDTH: f32 = 160.0;
const ENCODING_ACTION_BUTTON_SIZE: egui::Vec2 = egui::vec2(104.0, 40.0);
const ENCODING_CARD_CORNER_RADIUS: u8 = 12;
const ENCODING_COMBO_FILL: egui::Color32 = egui::Color32::from_rgb(74, 72, 68);
const ENCODING_COMBO_FILL_HOVER: egui::Color32 = egui::Color32::from_rgb(84, 82, 78);
const ENCODING_CLOSE_FILL: egui::Color32 = egui::Color32::from_rgb(155, 66, 58);
const ENCODING_CLOSE_FILL_HOVER: egui::Color32 = egui::Color32::from_rgb(177, 77, 67);
const ENCODING_FILE_ICON: egui::Color32 = egui::Color32::from_rgb(107, 158, 248);
const ENCODING_WARNING_FILL: egui::Color32 = egui::Color32::from_rgb(55, 46, 45);
const ENCODING_WARNING_ICON: egui::Color32 = egui::Color32::from_rgb(246, 177, 150);
const ENCODING_WARNING_TITLE: egui::Color32 = egui::Color32::from_rgb(246, 177, 150);

struct EncodingDialogState {
    active_index: usize,
    buffer_label: String,
    has_saved_path: bool,
    is_dirty: bool,
}

#[derive(Clone, Copy)]
struct EncodingActionSpec<'a> {
    icon: &'a str,
    title: &'a str,
    subtitle: &'a str,
    tooltip: &'a str,
    enabled: bool,
    action: fn(&mut ScratchpadApp, usize, &str) -> bool,
}

impl EncodingDialogState {
    fn from_app(app: &ScratchpadApp) -> Self {
        let active_index = app.active_tab_index();
        let (buffer_label, has_saved_path, is_dirty) = app
            .active_tab()
            .map(|tab| {
                (
                    tab.active_buffer().name.clone(),
                    tab.active_buffer().path.is_some(),
                    tab.active_buffer().is_dirty,
                )
            })
            .unwrap_or_else(|| ("Untitled".to_owned(), false, false));

        Self {
            active_index,
            buffer_label,
            has_saved_path,
            is_dirty,
        }
    }
}

pub(super) fn show_encoding_window(ctx: &egui::Context, app: &mut ScratchpadApp) {
    if !app.encoding_dialog_open {
        return;
    }

    let state = EncodingDialogState::from_app(app);
    let mut close_requested = false;

    show_callout(
        ctx,
        "encoding_overlay_v1",
        callout::centered_position(ctx, ENCODING_DIALOG_SIZE),
        ENCODING_DIALOG_SIZE.x,
        |ui| render_encoding_dialog(ui, app, &state, &mut close_requested),
    );

    if close_requested {
        app.close_encoding_dialog();
    }
}

fn render_encoding_dialog(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    state: &EncodingDialogState,
    close_requested: &mut bool,
) {
    settings::apply_dialog_typography(ui);
    callout::apply_spacing(ui);
    apply_encoding_dialog_typography(ui);
    ui.spacing_mut().item_spacing = egui::vec2(10.0, 12.0);

    render_dialog_title_bar(ui);
    ui.add_space(18.0);

    ui.horizontal(|ui| {
        let content_width = ui.available_width().min(ENCODING_DIALOG_CONTENT_WIDTH);
        let side_margin = ((ui.available_width() - content_width) * 0.5).max(0.0);
        if side_margin > 0.0 {
            ui.add_space(side_margin);
        }

        ui.vertical(|ui| {
            ui.set_width(content_width);
            ui.set_max_width(content_width);

            if render_selected_file_card(ui, state) {
                *close_requested = true;
            }
            ui.add_space(settings::dialog_card_gap());

            render_encoding_protocol_card(ui, app);

            for spec in encoding_action_specs(state) {
                ui.add_space(settings::dialog_card_gap());
                if trigger_encoding_action(ui, app, state.active_index, spec) {
                    *close_requested = true;
                }
            }

            ui.add_space(settings::dialog_card_gap());
            render_encoding_warning(ui);
        });
    });
}

fn encoding_action_specs(state: &EncodingDialogState) -> [EncodingActionSpec<'static>; 2] {
    [
        EncodingActionSpec {
            icon: ARROW_COUNTER_CLOCKWISE,
            title: "Reopen with",
            subtitle: if !state.has_saved_path {
                "Reopen the file using the selected encoding after it has been saved."
            } else if state.is_dirty {
                "Save or discard local changes before reopening with a different encoding."
            } else {
                "Reopen the file using the selected encoding."
            },
            tooltip: "Reopen active file with selected encoding",
            enabled: state.has_saved_path && !state.is_dirty,
            action: FileController::reopen_buffer_with_encoding,
        },
        EncodingActionSpec {
            icon: FLOPPY_DISK,
            title: "Save with",
            subtitle: if state.has_saved_path {
                "Commit the file to disk using the selected encoding."
            } else {
                "Choose a path and save the file using the selected encoding."
            },
            tooltip: "Save active file using selected encoding",
            enabled: true,
            action: FileController::save_file_with_encoding_at,
        },
    ]
}

fn trigger_encoding_action(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    active_index: usize,
    spec: EncodingActionSpec<'_>,
) -> bool {
    render_encoding_action_card(ui, spec) && {
        let encoding_name = std::mem::take(&mut app.encoding_dialog_choice);
        let result = (spec.action)(app, active_index, &encoding_name);
        app.encoding_dialog_choice = encoding_name;
        result
    }
}

fn apply_encoding_dialog_typography(ui: &mut egui::Ui) {
    let font_family = egui::FontFamily::Name(EDITOR_FONT_FAMILY.into());
    let style = ui.style_mut();
    style.override_font_id = Some(egui::FontId::new(15.0, font_family.clone()));
    style
        .text_styles
        .insert(egui::TextStyle::Body, egui::FontId::new(15.0, font_family.clone()));
    style.text_styles.insert(
        egui::TextStyle::Button,
        egui::FontId::new(14.0, font_family.clone()),
    );
    style
        .text_styles
        .insert(egui::TextStyle::Small, egui::FontId::new(13.0, font_family));
}

fn render_dialog_title_bar(ui: &mut egui::Ui) {
    ui.label(
        egui::RichText::new("Encoding")
            .size(ENCODING_TITLE_SIZE)
            .color(callout::text(ui)),
    );
}

fn render_selected_file_card(ui: &mut egui::Ui, state: &EncodingDialogState) -> bool {
    let mut close_requested = false;
    encoding_card(ui, |ui| {
        render_dialog_card_row(
            ui,
            FILE_TEXT,
            "Selected file",
            Some(&state.buffer_label),
            true,
            |ui| {
                close_requested =
                    render_close_card_button(ui, "Close encoding actions").clicked();
            },
        );
    });
    close_requested
}

fn render_encoding_protocol_card(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    encoding_card(ui, |ui| {
        render_dialog_card_row(
            ui,
            "Aあ",
            "Encoding protocol",
            Some("Choose how this file should be opened or saved."),
            false,
            |ui| {
                ui.allocate_ui(egui::vec2(ENCODING_CONTROL_WIDTH, 0.0), |ui| {
                    ui.set_width(ENCODING_CONTROL_WIDTH);
                    ui.set_max_width(ENCODING_CONTROL_WIDTH);
                    render_encoding_combo(ui, &mut app.encoding_dialog_choice);
                });
            },
        );
    });
}

fn render_encoding_action_card(ui: &mut egui::Ui, spec: EncodingActionSpec<'_>) -> bool {
    let mut clicked = false;
    encoding_card(ui, |ui| {
        render_dialog_card_row(ui, spec.icon, spec.title, Some(spec.subtitle), false, |ui| {
            clicked = ui
                .add_enabled(
                    spec.enabled,
                    egui::Button::new(
                        egui::RichText::new(action_label(spec.title)).color(text_primary(ui)),
                    )
                    .min_size(ENCODING_ACTION_BUTTON_SIZE)
                    .fill(action_hover_bg(ui))
                    .stroke(egui::Stroke::new(1.0, border(ui).gamma_multiply(0.75)))
                    .corner_radius(egui::CornerRadius::same(8)),
                )
                .on_hover_text(spec.tooltip)
                .clicked();
        });
    });
    clicked
}

fn action_label(title: &str) -> &'static str {
    match title {
        "Reopen with" => "Reopen",
        "Save with" => "Save",
        _ => "Apply",
    }
}

fn render_encoding_warning(ui: &mut egui::Ui) {
    encoding_card_frame(ui)
        .fill(ENCODING_WARNING_FILL)
        .stroke(egui::Stroke::new(1.0, settings::dialog_card_border(ui).gamma_multiply(0.55)))
        .show(ui, |ui| {
            ui.with_layout(egui::Layout::left_to_right(egui::Align::TOP), |ui| {
                ui.allocate_ui(egui::vec2(28.0, 28.0), |ui| {
                    ui.with_layout(
                        egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
                        |ui| {
                            ui.label(
                                egui::RichText::new(WARNING)
                                    .size(18.0)
                                    .color(ENCODING_WARNING_ICON),
                            );
                        },
                    );
                });
                ui.add_space(12.0);
                ui.vertical(|ui| {
                    ui.label(
                        egui::RichText::new("Compatibility warning")
                            .size(15.0)
                            .color(ENCODING_WARNING_TITLE),
                    );
                    ui.add_space(2.0);
                    ui.label(
                        egui::RichText::new(
                            "Using an incompatible encoding may permanently corrupt characters or lose character mapping data. Proceed with caution.",
                        )
                        .size(12.5)
                        .color(text_primary(ui).gamma_multiply(0.82)),
                    );
                });
            });
        });
}

fn render_dialog_card_row(
    ui: &mut egui::Ui,
    icon: &str,
    title: &str,
    description: Option<&str>,
    truncate_description: bool,
    add_trailing: impl FnOnce(&mut egui::Ui),
) {
    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.set_min_height(ENCODING_CARD_MIN_HEIGHT);
        render_card_icon(ui, icon);
        ui.add_space(16.0);

        let trailing_width = ENCODING_CONTROL_WIDTH.max(ui.available_width().min(220.0));
        let text_width = (ui.available_width() - trailing_width - 12.0).max(180.0);

        ui.allocate_ui_with_layout(
            egui::vec2(text_width, 0.0),
            egui::Layout::top_down(egui::Align::LEFT),
            |ui| {
                ui.set_width(text_width);
                ui.label(
                    egui::RichText::new(title)
                        .size(15.5)
                        .color(text_primary(ui)),
                );
                if let Some(description) = description {
                    ui.add_space(2.0);
                    let description_label = egui::Label::new(
                        egui::RichText::new(description)
                            .size(12.5)
                            .color(text_muted(ui)),
                    );
                    if truncate_description {
                        ui.add_sized(egui::vec2(text_width, 0.0), description_label.truncate());
                    } else {
                        ui.add_sized(egui::vec2(text_width, 0.0), description_label.wrap());
                    }
                }
            },
        );

        ui.add_space(12.0);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.set_min_width(trailing_width);
            add_trailing(ui);
        });
    });
}

fn render_card_icon(ui: &mut egui::Ui, icon: &str) {
    ui.allocate_ui(egui::vec2(28.0, 28.0), |ui| {
        ui.with_layout(
            egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
            |ui| {
                let color = if icon == FILE_TEXT {
                    ENCODING_FILE_ICON
                } else {
                    text_muted(ui)
                };
                ui.label(egui::RichText::new(icon).size(18.0).color(color));
            },
        );
    });
}

fn encoding_card(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui)) {
    let card_width = ui.available_width().min(ENCODING_DIALOG_CONTENT_WIDTH);
    ui.set_width(card_width);
    ui.set_max_width(card_width);
    encoding_card_frame(ui).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.set_max_width(ui.available_width());
        add_contents(ui);
    });
}

fn encoding_card_frame(ui: &egui::Ui) -> egui::Frame {
    settings::dialog_card_frame(ui)
        .corner_radius(egui::CornerRadius::same(ENCODING_CARD_CORNER_RADIUS))
        .inner_margin(egui::Margin::symmetric(22, 18))
}

fn render_close_card_button(ui: &mut egui::Ui, tooltip: &str) -> egui::Response {
    ui.scope(|ui| {
        let text_color = text_primary(ui);
        let visuals = ui.visuals_mut();
        visuals.widgets.inactive.bg_fill = ENCODING_CLOSE_FILL;
        visuals.widgets.hovered.bg_fill = ENCODING_CLOSE_FILL_HOVER;
        visuals.widgets.active.bg_fill = ENCODING_CLOSE_FILL_HOVER;
        visuals.widgets.inactive.weak_bg_fill = ENCODING_CLOSE_FILL;
        visuals.widgets.hovered.weak_bg_fill = ENCODING_CLOSE_FILL_HOVER;
        visuals.widgets.active.weak_bg_fill = ENCODING_CLOSE_FILL_HOVER;
        visuals.widgets.inactive.bg_stroke = egui::Stroke::NONE;
        visuals.widgets.hovered.bg_stroke = egui::Stroke::NONE;
        visuals.widgets.active.bg_stroke = egui::Stroke::NONE;
        visuals.widgets.inactive.fg_stroke.color = text_color;
        visuals.widgets.hovered.fg_stroke.color = text_color;
        visuals.widgets.active.fg_stroke.color = text_color;

        ui.add(
            egui::Button::new(egui::RichText::new(egui_phosphor::regular::X).size(20.0))
                .min_size(egui::vec2(48.0, 48.0))
                .corner_radius(egui::CornerRadius::same(8)),
        )
        .on_hover_text(tooltip)
    })
    .inner
}

fn render_encoding_combo(ui: &mut egui::Ui, selected_encoding: &mut String) {
    ui.scope(|ui| {
        let text_color = text_primary(ui);
        let visuals = ui.visuals_mut();
        visuals.widgets.inactive.bg_fill = ENCODING_COMBO_FILL;
        visuals.widgets.hovered.bg_fill = ENCODING_COMBO_FILL_HOVER;
        visuals.widgets.active.bg_fill = ENCODING_COMBO_FILL_HOVER;
        visuals.widgets.open.bg_fill = ENCODING_COMBO_FILL_HOVER;
        visuals.widgets.inactive.weak_bg_fill = ENCODING_COMBO_FILL;
        visuals.widgets.hovered.weak_bg_fill = ENCODING_COMBO_FILL_HOVER;
        visuals.widgets.active.weak_bg_fill = ENCODING_COMBO_FILL_HOVER;
        visuals.widgets.open.weak_bg_fill = ENCODING_COMBO_FILL_HOVER;
        visuals.widgets.inactive.fg_stroke.color = text_color;
        visuals.widgets.hovered.fg_stroke.color = text_color;
        visuals.widgets.active.fg_stroke.color = text_color;
        visuals.widgets.open.fg_stroke.color = text_color;
        visuals.widgets.inactive.bg_stroke = egui::Stroke::NONE;
        visuals.widgets.hovered.bg_stroke = egui::Stroke::NONE;
        visuals.widgets.active.bg_stroke = egui::Stroke::NONE;
        visuals.widgets.open.bg_stroke = egui::Stroke::NONE;

        egui::ComboBox::from_id_salt("encoding_dialog_combo")
            .selected_text(
                egui::RichText::new(selected_encoding.as_str())
                    .size(13.5)
                    .color(text_primary(ui)),
            )
            .icon(|ui, rect, visuals, _is_open| {
                let painter = ui.painter();
                let center = rect.center();
                let width = 7.0;
                let height = 4.0;
                let points = vec![
                    egui::pos2(center.x - width, center.y - height),
                    egui::pos2(center.x + width, center.y - height),
                    egui::pos2(center.x, center.y + height),
                ];
                painter.add(egui::Shape::convex_polygon(
                    points,
                    visuals.fg_stroke.color,
                    egui::Stroke::NONE,
                ));
            })
            .width(ui.available_width())
            .show_ui(ui, |ui| {
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                for option in COMMON_TEXT_ENCODINGS {
                    ui.selectable_value(
                        selected_encoding,
                        option.canonical_name.to_owned(),
                        option.label,
                    );
                }
            });
    });
}
