use crate::app::app_state::ScratchpadApp;
use crate::app::domain::{RenderedLayout, platform_default_line_ending};
use crate::app::theme::*;
use eframe::egui;

#[derive(Default)]
struct StatusBarActions {
    toggle_line_numbers: bool,
    toggle_control_chars: bool,
    open_transaction_log: bool,
    open_encoding_dialog: bool,
}

struct ActiveStatusDetails {
    path_label: String,
    count_label: String,
    encoding_label: String,
    encoding_tooltip: String,
    encoding_is_non_default: bool,
    has_non_compliant_characters: bool,
    line_endings_label: String,
    line_endings_are_non_default: bool,
    icon: &'static str,
    icon_tooltip: &'static str,
    icon_color: egui::Color32,
    freshness_label: Option<String>,
    is_large_file: bool,
    has_control_chars: bool,
}

pub(crate) fn show_status_bar(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    egui::Panel::bottom("status").show_inside(ui, |ui| {
        ui.horizontal(|ui| {
            if app.showing_settings() {
                ui.label("Settings");
                ui.separator();
                ui.label(app.settings_path().display().to_string());
                if let Some(message) = &app.status_message {
                    ui.separator();
                    ui.label(message);
                }
                return;
            }

            let mut actions = StatusBarActions::default();

            if let Some(details) = collect_active_status_details(app, ui.visuals().dark_mode) {
                render_active_status(ui, &details, &mut actions);
            }

            if let Some(message) = &app.status_message {
                ui.separator();
                ui.label(message);
            }

            apply_status_actions(app, actions);
        });
    });
}

fn collect_active_status_details(
    app: &ScratchpadApp,
    dark_mode: bool,
) -> Option<ActiveStatusDetails> {
    let tab = app.active_tab()?;
    let line_count = tab.buffer.line_count;
    let visual_row_count = tab
        .active_view()
        .and_then(|view| view.latest_layout.as_ref())
        .map(RenderedLayout::visual_row_count)
        .unwrap_or(line_count);
    let show_control_chars = tab
        .active_view()
        .map(|view| view.show_control_chars)
        .unwrap_or(false);
    let has_control_chars = tab.buffer.artifact_summary.has_control_chars();
    let (icon, icon_tooltip, icon_color) =
        artifact_icon(has_control_chars, show_control_chars, dark_mode);

    Some(ActiveStatusDetails {
        path_label: tab
            .buffer
            .path
            .as_ref()
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Untitled".to_owned()),
        count_label: line_count_label(line_count, visual_row_count),
        encoding_label: tab.buffer.format.encoding_label(),
        encoding_tooltip: tab.buffer.format.encoding_tooltip(),
        encoding_is_non_default: status_bar_encoding_is_non_default(&tab.buffer.format),
        has_non_compliant_characters: tab
            .buffer
            .format
            .has_non_compliant_characters(tab.buffer.text()),
        line_endings_label: tab.buffer.format.line_endings_label().to_owned(),
        line_endings_are_non_default: tab.buffer.format.preferred_line_ending_style()
            != platform_default_line_ending(),
        icon,
        icon_tooltip,
        icon_color,
        freshness_label: tab.buffer.disk_status_label().map(str::to_owned),
        is_large_file: tab.buffer.text().len() > 5 * 1024 * 1024,
        has_control_chars,
    })
}

fn render_active_status(
    ui: &mut egui::Ui,
    details: &ActiveStatusDetails,
    actions: &mut StatusBarActions,
) {
    ui.label(format!("Path: {}", details.path_label));
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        show_status_warnings(ui, details);
        show_transaction_log_button(ui, actions);
        show_control_char_toggle(ui, details, actions);
        show_line_endings(
            ui,
            &details.line_endings_label,
            details.line_endings_are_non_default,
        );
        let encoding_response = show_encoding(
            ui,
            &details.encoding_label,
            &details.encoding_tooltip,
            details.encoding_is_non_default,
        );
        if encoding_response.clicked() {
            actions.open_encoding_dialog = true;
        }
        show_line_count(ui, &details.count_label, actions);
    });
}

fn show_line_count(ui: &mut egui::Ui, count_label: &str, actions: &mut StatusBarActions) {
    let line_count_response = ui
        .label(count_label)
        .on_hover_text("Double-click to toggle line numbers");
    if line_count_response.double_clicked() {
        actions.toggle_line_numbers = true;
    }
}

fn show_encoding(
    ui: &mut egui::Ui,
    encoding: &str,
    tooltip: &str,
    highlight: bool,
) -> egui::Response {
    ui.separator();
    ui.add(egui::Label::new(status_format_text(encoding, highlight)).sense(egui::Sense::click()))
        .on_hover_text(format!("{tooltip}\nClick for encoding actions"))
}

fn show_line_endings(ui: &mut egui::Ui, line_endings_label: &str, highlight: bool) {
    ui.separator();
    ui.label(status_format_text(
        &format!("EOL: {line_endings_label}"),
        highlight,
    ));
}

fn status_format_text(label: &str, highlight: bool) -> egui::RichText {
    let mut text = egui::RichText::new(label);
    if highlight {
        text = text.color(egui::Color32::YELLOW);
    }
    text
}

fn status_bar_encoding_is_non_default(format: &crate::app::domain::TextFormatMetadata) -> bool {
    !format.encoding_name.eq_ignore_ascii_case("UTF-8") || format.has_bom
}

fn show_transaction_log_button(ui: &mut egui::Ui, actions: &mut StatusBarActions) {
    ui.separator();
    let response = ui
        .button("TXN")
        .on_hover_text("Open the workspace transaction log");
    if response.clicked() {
        actions.open_transaction_log = true;
    }
}

fn show_control_char_toggle(
    ui: &mut egui::Ui,
    details: &ActiveStatusDetails,
    actions: &mut StatusBarActions,
) {
    ui.separator();
    let button_response = ui.add(
        egui::Button::new("")
            .min_size(egui::vec2(22.0, 22.0))
            .fill(egui::Color32::TRANSPARENT)
            .stroke(egui::Stroke::NONE),
    );
    ui.painter().text(
        button_response.rect.center(),
        egui::Align2::CENTER_CENTER,
        details.icon,
        egui::FontId::proportional(16.0),
        details.icon_color,
    );
    if button_response.hovered() {
        button_response.clone().on_hover_text(details.icon_tooltip);
    }
    if details.has_control_chars && button_response.clicked() {
        actions.toggle_control_chars = true;
    }
}

fn show_status_warnings(ui: &mut egui::Ui, details: &ActiveStatusDetails) {
    if let Some(freshness_label) = &details.freshness_label {
        ui.separator();
        ui.label(egui::RichText::new(freshness_label).color(egui::Color32::YELLOW));
    }

    if details.is_large_file {
        ui.separator();
        ui.label(
            egui::RichText::new("Large file: performance may be degraded")
                .color(egui::Color32::YELLOW),
        );
    }

    if details.has_non_compliant_characters {
        ui.separator();
        ui.label(egui::RichText::new("Non compliant characters").color(egui::Color32::RED));
    }
}

fn apply_status_actions(app: &mut ScratchpadApp, actions: StatusBarActions) {
    if actions.toggle_line_numbers
        && let Some(tab) = app.active_tab_mut()
    {
        let next_visible = !tab.line_numbers_visible();
        tab.set_line_numbers_visible(next_visible);
        app.mark_session_dirty();
    }

    if actions.toggle_control_chars {
        let can_toggle = app
            .active_tab()
            .map(|tab| tab.buffer.artifact_summary.has_control_chars())
            .unwrap_or(false);
        if can_toggle && let Some(view) = app.active_view_mut() {
            view.show_control_chars = !view.show_control_chars;
            app.mark_session_dirty();
        }
    }

    if actions.open_transaction_log {
        app.open_transaction_log();
    }

    if actions.open_encoding_dialog {
        app.open_encoding_dialog();
    }
}

fn line_count_label(line_count: usize, visual_row_count: usize) -> String {
    if visual_row_count > line_count {
        format!("Lines: {line_count} ({visual_row_count} rows)")
    } else {
        format!("Lines: {line_count}")
    }
}

fn artifact_icon(
    has_control_chars: bool,
    show_control_chars: bool,
    dark_mode: bool,
) -> (&'static str, &'static str, egui::Color32) {
    if has_control_chars {
        if show_control_chars {
            (
                egui_phosphor::regular::TEXT_OUTDENT,
                "Visible control-character inspection active; click to return to raw-text editing",
                egui::Color32::YELLOW,
            )
        } else {
            (
                egui_phosphor::regular::TEXT_ALIGN_JUSTIFY,
                "Control characters detected; raw-text editing remains enabled; click to inspect them",
                egui::Color32::LIGHT_GREEN,
            )
        }
    } else {
        (
            egui_phosphor::regular::TEXT_ALIGN_JUSTIFY,
            "Plain text",
            plain_text_icon_color(dark_mode),
        )
    }
}

fn plain_text_icon_color(dark_mode: bool) -> egui::Color32 {
    if dark_mode {
        TEXT_PRIMARY.gamma_multiply(0.8)
    } else {
        egui::Color32::from_rgb(28, 35, 45).gamma_multiply(0.8)
    }
}
