use crate::app::app_state::{ScratchpadApp, StatusSeverity};
use crate::app::commands::AppCommand;
use crate::app::theme::*;
use crate::app::ui::widget_ids;
use eframe::egui;

mod model;

use model::{
    ActiveStatusDetails, StatusBarItemKind, active_status_items, collect_active_status_details,
    left_truncated_text_for_width, status_bar_path_layout, status_path_min_width,
};

#[cfg(test)]
use model::{
    StatusBarItem, StatusBarPathLayout, artifact_icon, plain_text_icon_color, status_cursor_range,
};

const STATUS_ICON_CELL_SIZE: egui::Vec2 = egui::vec2(28.0, 22.0);
const STATUS_ICON_FONT_SIZE: f32 = 16.0;
const STATUS_PATH_MIN_WIDTH: f32 = 92.0;
const STATUS_SEPARATOR_VISUAL_WIDTH: f32 = 6.0;
const CONTROL_CHAR_ICON: &str = "¶";
const HIDDEN_CONTROL_CHAR_ICON: &str = egui_phosphor::regular::TEXT_ALIGN_JUSTIFY;

#[derive(Default)]
struct StatusBarActions {
    toggle_line_numbers: bool,
    toggle_control_chars: bool,
    open_status_history: bool,
    open_text_history: bool,
    open_encoding_dialog: bool,
    open_settings: bool,
}

pub(crate) fn show_status_bar(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    app.queue_active_buffer_encoding_compliance_refresh();

    egui::Panel::bottom("status").show_inside(ui, |ui| {
        widget_ids::feature_scope(ui, "status_bar", |ui| {
            ui.horizontal(|ui| {
                if app.showing_settings() {
                    let mut actions = StatusBarActions::default();
                    render_settings_status(
                        ui,
                        app,
                        app.settings_path().display().to_string(),
                        &mut actions,
                    );
                    apply_status_actions(app, actions);
                    return;
                }

                let mut actions = StatusBarActions::default();

                if let Some(details) = collect_active_status_details(app, ui.visuals().dark_mode) {
                    render_active_status(ui, app, &details, &mut actions);
                }

                apply_status_actions(app, actions);
            });
        });
    });
}

fn render_active_status(
    ui: &mut egui::Ui,
    app: &ScratchpadApp,
    details: &ActiveStatusDetails,
    actions: &mut StatusBarActions,
) {
    show_status_history_button(ui, app, actions);
    ui.separator();

    let path_min_width = status_path_min_width(ui, app);
    let items = active_status_items(ui, details);
    let layout = status_bar_path_layout(ui.available_width(), path_min_width, &items);

    show_copyable_path_sized(
        ui,
        &format!("Path: {}", details.path_label),
        layout.path_width,
    );
    for item in &items[layout.visible_start..] {
        render_status_item(ui, item.kind, details, actions);
    }
}

fn render_settings_status(
    ui: &mut egui::Ui,
    app: &ScratchpadApp,
    path_label: String,
    actions: &mut StatusBarActions,
) {
    show_status_history_button(ui, app, actions);
    ui.separator();
    ui.label("Settings");
    ui.separator();

    let path_min_width = status_path_min_width(ui, app);
    let layout = status_bar_path_layout(ui.available_width(), path_min_width, &[]);
    show_copyable_path_sized(ui, &path_label, layout.path_width);
}

fn show_copyable_path_sized(ui: &mut egui::Ui, label: &str, width: f32) {
    let width = width.max(0.0);
    let display_label = left_truncated_text_for_width(ui, label, width);
    let response = widget_ids::allocate_exact_interact(
        ui,
        egui::vec2(width, 22.0),
        widget_ids::surface_child("status_path", "copyable_path"),
        egui::Sense::click(),
        "copyable_path",
    )
    .on_hover_text(format!("{label}\nDouble-click to copy path"));
    paint_left_aligned_status_text(ui, response.rect, &display_label);
    if response.double_clicked() {
        let copied = label.strip_prefix("Path: ").unwrap_or(label);
        ui.copy_text(copied.to_owned());
    }
}

fn paint_left_aligned_status_text(ui: &egui::Ui, rect: egui::Rect, text: &str) {
    let font_id = egui::TextStyle::Button.resolve(ui.style());
    let color = text_primary(ui);
    let galley = ui.fonts_mut(|fonts| fonts.layout_no_wrap(text.to_owned(), font_id, color));
    let pos = egui::pos2(rect.left(), rect.center().y - galley.size().y / 2.0);
    ui.painter().with_clip_rect(rect).galley(pos, galley, color);
}

fn show_line_count(ui: &mut egui::Ui, count_label: &str, actions: &mut StatusBarActions) {
    let line_count_response =
        widget_ids::surface_response(ui, "status_line_count", "line_count_label", |ui| {
            ui.label(count_label)
        })
        .on_hover_text("Double-click to toggle line numbers");
    if line_count_response.double_clicked() {
        actions.toggle_line_numbers = true;
    }
}

fn show_encoding(ui: &mut egui::Ui, encoding: &str, highlight: bool) -> egui::Response {
    ui.separator();
    widget_ids::surface_response(ui, "status_encoding", "encoding_label", |ui| {
        ui.add(
            egui::Label::new(status_format_text(ui, encoding, highlight))
                .sense(egui::Sense::click()),
        )
    })
    .on_hover_cursor(egui::CursorIcon::PointingHand)
    .on_hover_text("Text Encoding")
}

fn show_status_segment(ui: &mut egui::Ui, label: Option<&str>) {
    let Some(label) = label else {
        return;
    };
    ui.separator();
    widget_ids::surface_response(
        ui,
        ("status_segment", label),
        widget_ids::WidgetRole::Label,
        |ui| ui.label(label),
    );
}

fn show_line_endings(ui: &mut egui::Ui, line_endings_label: &str, highlight: bool) {
    ui.separator();
    widget_ids::surface_response(
        ui,
        "status_line_endings",
        widget_ids::WidgetRole::Label,
        |ui| {
            ui.label(status_format_text(
                ui,
                &format!("EOL: {line_endings_label}"),
                highlight,
            ))
        },
    );
}

fn status_format_text(ui: &egui::Ui, label: &str, highlight: bool) -> egui::RichText {
    let mut text = egui::RichText::new(label);
    if highlight {
        text = text.color(status_attention_color(ui.visuals().dark_mode));
    }
    text
}

fn status_attention_color(dark_mode: bool) -> egui::Color32 {
    if dark_mode {
        egui::Color32::from_rgb(245, 210, 92)
    } else {
        egui::Color32::from_rgb(154, 101, 0)
    }
}

fn show_settings_button(ui: &mut egui::Ui, actions: &mut StatusBarActions) {
    ui.separator();
    let response = status_bar_icon_button(ui, "status_settings", egui_phosphor::regular::GEAR)
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text("Settings");
    if response.clicked() {
        actions.open_settings = true;
    }
}

fn show_text_history_button(ui: &mut egui::Ui, actions: &mut StatusBarActions) {
    ui.separator();
    let response = status_bar_icon_button(
        ui,
        "status_text_history",
        egui_phosphor::regular::CLOCK_COUNTER_CLOCKWISE,
    )
    .on_hover_cursor(egui::CursorIcon::PointingHand)
    .on_hover_text("Text history");
    if response.clicked() {
        actions.open_text_history = true;
    }
}

fn show_status_history_button(
    ui: &mut egui::Ui,
    app: &ScratchpadApp,
    actions: &mut StatusBarActions,
) {
    let has_errors = app
        .status
        .history
        .iter()
        .any(|status| status.severity == StatusSeverity::Error);
    let color = if has_errors {
        egui::Color32::from_rgb(220, 64, 64)
    } else {
        status_icon_color(ui)
    };
    let tooltip = if has_errors {
        "Status History has Errors"
    } else {
        "Status history"
    };
    let response = fixed_status_icon_cell(
        ui,
        "status_message_history",
        egui_phosphor::regular::BRACKETS_SQUARE,
        color,
    )
    .on_hover_cursor(egui::CursorIcon::PointingHand)
    .on_hover_text(tooltip);
    if response.clicked() {
        actions.open_status_history = true;
    }
}

fn status_bar_icon_button(
    ui: &mut egui::Ui,
    surface_key: &'static str,
    icon: &str,
) -> egui::Response {
    fixed_status_icon_cell(ui, surface_key, icon, status_icon_color(ui))
}

fn show_control_char_toggle(
    ui: &mut egui::Ui,
    details: &ActiveStatusDetails,
    actions: &mut StatusBarActions,
) {
    ui.separator();
    let button_response = fixed_status_icon_cell(
        ui,
        "status_control_chars",
        details.control_chars.icon,
        details.control_chars.icon_color,
    );
    if button_response.hovered() {
        let tooltip = if details.control_chars.is_available
            || details.control_chars.icon == CONTROL_CHAR_ICON
        {
            details.control_chars.icon_tooltip
        } else {
            "Control characters"
        };
        button_response.clone().on_hover_text(tooltip);
    }
    if button_response.clicked()
        && (details.control_chars.is_available || details.control_chars.icon == CONTROL_CHAR_ICON)
    {
        actions.toggle_control_chars = true;
    }
}

fn fixed_status_icon_cell(
    ui: &mut egui::Ui,
    surface_key: &'static str,
    icon: &str,
    color: egui::Color32,
) -> egui::Response {
    widget_ids::surface_response(ui, surface_key, widget_ids::WidgetRole::IconButton, |ui| {
        ui.add_sized(
            STATUS_ICON_CELL_SIZE,
            egui::Label::new(
                egui::RichText::new(icon)
                    .font(egui::FontId::proportional(STATUS_ICON_FONT_SIZE))
                    .color(color),
            )
            .sense(egui::Sense::click()),
        )
    })
}

fn status_icon_color(ui: &egui::Ui) -> egui::Color32 {
    text_primary(ui)
}

fn show_disk_state(ui: &mut egui::Ui, details: &ActiveStatusDetails) {
    ui.separator();
    widget_ids::surface_response(
        ui,
        "status_disk_state",
        widget_ids::WidgetRole::Label,
        |ui| ui.label(egui::RichText::new(&details.disk.label).color(details.disk.color)),
    );
}

fn show_non_compliant_warning(ui: &mut egui::Ui, details: &ActiveStatusDetails) {
    if details.has_non_compliant_characters {
        ui.separator();
        widget_ids::surface_response(
            ui,
            "status_non_compliant_warning",
            widget_ids::WidgetRole::Label,
            |ui| {
                ui.label(egui::RichText::new("Non compliant characters").color(egui::Color32::RED))
            },
        );
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

    if actions.toggle_control_chars
        && let Some(tab) = app.active_tab_mut()
    {
        let buffer = tab.active_buffer_mut();
        buffer.show_control_chars = !buffer.show_control_chars;
        app.mark_session_dirty();
    }

    if actions.open_encoding_dialog {
        app.open_encoding_dialog();
    }

    if actions.open_text_history {
        app.handle_command(AppCommand::OpenTextHistory);
    }

    if actions.open_status_history {
        app.open_status_history();
    }

    if actions.open_settings {
        app.handle_command(AppCommand::OpenSettings);
    }
}

fn render_status_item(
    ui: &mut egui::Ui,
    kind: StatusBarItemKind,
    details: &ActiveStatusDetails,
    actions: &mut StatusBarActions,
) {
    match kind {
        StatusBarItemKind::LineCount => show_line_count(ui, &details.count_label, actions),
        StatusBarItemKind::Cursor => show_status_segment(ui, details.cursor_label.as_deref()),
        StatusBarItemKind::Selection => show_status_segment(ui, details.selection_label.as_deref()),
        StatusBarItemKind::DiskState => show_disk_state(ui, details),
        StatusBarItemKind::Encoding => {
            if show_encoding(ui, &details.encoding.label, details.encoding.is_non_default).clicked()
            {
                actions.open_encoding_dialog = true;
            }
        }
        StatusBarItemKind::LineEndings => show_line_endings(
            ui,
            &details.line_endings.label,
            details.line_endings.is_non_default,
        ),
        StatusBarItemKind::ControlChars => show_control_char_toggle(ui, details, actions),
        StatusBarItemKind::TextHistory => show_text_history_button(ui, actions),
        StatusBarItemKind::Settings => show_settings_button(ui, actions),
        StatusBarItemKind::NonCompliant => show_non_compliant_warning(ui, details),
    }
}

#[cfg(test)]
mod tests;
