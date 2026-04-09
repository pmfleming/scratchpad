use crate::app::app_state::ScratchpadApp;
use crate::app::domain::RenderedLayout;
use crate::app::theme::*;
use eframe::egui;

#[derive(Default)]
struct StatusBarActions {
    toggle_line_numbers: bool,
    toggle_control_chars: bool,
    toggle_logging: bool,
}

struct ActiveStatusDetails {
    path_label: String,
    count_label: String,
    encoding: String,
    icon: &'static str,
    icon_tooltip: &'static str,
    icon_color: egui::Color32,
    warning_label: Option<String>,
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

            if let Some(details) = collect_active_status_details(app) {
                render_active_status(ui, &details, app.logging_enabled(), &mut actions);
            }

            if let Some(message) = &app.status_message {
                ui.separator();
                ui.label(message);
            }

            apply_status_actions(app, actions);
        });
    });
}

fn collect_active_status_details(app: &ScratchpadApp) -> Option<ActiveStatusDetails> {
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
    let (icon, icon_tooltip, icon_color) = artifact_icon(has_control_chars, show_control_chars);

    Some(ActiveStatusDetails {
        path_label: tab
            .buffer
            .path
            .as_ref()
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Untitled".to_owned()),
        count_label: line_count_label(line_count, visual_row_count),
        encoding: tab.buffer.encoding.clone(),
        icon,
        icon_tooltip,
        icon_color,
        warning_label: tab
            .buffer
            .artifact_summary
            .status_text()
            .map(|warning_text| {
                if show_control_chars {
                    format!("{warning_text}; all visible")
                } else {
                    format!("{warning_text}; cleaned view")
                }
            }),
        is_large_file: tab.buffer.content.len() > 5 * 1024 * 1024,
        has_control_chars,
    })
}

fn render_active_status(
    ui: &mut egui::Ui,
    details: &ActiveStatusDetails,
    logging_enabled: bool,
    actions: &mut StatusBarActions,
) {
    ui.label(format!("Path: {}", details.path_label));
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        show_status_warnings(ui, details);
        show_control_char_toggle(ui, details, actions);
        show_logging_toggle(ui, logging_enabled, actions);
        show_encoding(ui, &details.encoding);
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

fn show_encoding(ui: &mut egui::Ui, encoding: &str) {
    ui.separator();
    ui.label(encoding);
}

fn show_logging_toggle(ui: &mut egui::Ui, logging_enabled: bool, actions: &mut StatusBarActions) {
    ui.separator();
    let logging_token = ui
        .selectable_label(logging_enabled, "LOG")
        .on_hover_text("Toggle runtime file logging");
    if logging_token.clicked() {
        actions.toggle_logging = true;
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
    if details.is_large_file {
        ui.separator();
        ui.label(
            egui::RichText::new("Large file: performance may be degraded")
                .color(egui::Color32::YELLOW),
        );
    }

    if let Some(warning_label) = &details.warning_label {
        ui.separator();
        ui.label(egui::RichText::new(warning_label).color(egui::Color32::YELLOW));
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

    if actions.toggle_logging {
        let next = !app.logging_enabled();
        app.set_logging_enabled(next);
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
) -> (&'static str, &'static str, egui::Color32) {
    if has_control_chars {
        if show_control_chars {
            (
                egui_phosphor::regular::TEXT_OUTDENT,
                "All control characters visible; click to switch to cleaned view",
                egui::Color32::YELLOW,
            )
        } else {
            (
                egui_phosphor::regular::TEXT_ALIGN_JUSTIFY,
                "Cleaned view active; click to return to all control characters",
                egui::Color32::LIGHT_GREEN,
            )
        }
    } else {
        (
            egui_phosphor::regular::TEXT_ALIGN_JUSTIFY,
            "Plain text",
            TEXT_PRIMARY.gamma_multiply(0.8),
        )
    }
}
