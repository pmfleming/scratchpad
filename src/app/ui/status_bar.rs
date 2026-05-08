use crate::app::app_state::ScratchpadApp;
use crate::app::commands::AppCommand;
use crate::app::domain::{
    BufferFreshness, BufferState, BufferViewStatus, platform_default_line_ending,
};
use crate::app::theme::*;
use crate::app::ui::widget_ids;
use eframe::egui;

const STATUS_ICON_CELL_SIZE: egui::Vec2 = egui::vec2(28.0, 22.0);
const STATUS_ICON_FONT_SIZE: f32 = 16.0;
const CONTROL_CHAR_ICON: &str = "¶";
const HIDDEN_CONTROL_CHAR_ICON: &str = egui_phosphor::regular::TEXT_ALIGN_JUSTIFY;

#[derive(Default)]
struct StatusBarActions {
    toggle_line_numbers: bool,
    toggle_control_chars: bool,
    open_text_history: bool,
    open_encoding_dialog: bool,
    open_settings: bool,
}

struct ActiveStatusDetails {
    path_label: String,
    count_label: String,
    cursor_label: Option<String>,
    selection_label: Option<String>,
    encoding_label: String,
    encoding_tooltip: String,
    encoding_is_non_default: bool,
    has_non_compliant_characters: bool,
    line_endings_label: String,
    line_endings_are_non_default: bool,
    icon: &'static str,
    icon_tooltip: &'static str,
    icon_color: egui::Color32,
    control_chars_available: bool,
    freshness_label: Option<String>,
}

pub(crate) fn show_status_bar(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    app.queue_active_buffer_encoding_compliance_refresh();

    egui::Panel::bottom("status").show_inside(ui, |ui| {
        widget_ids::feature_scope(ui, "status_bar", |ui| {
            ui.horizontal(|ui| {
                if app.showing_settings() {
                    ui.label("Settings");
                    ui.separator();
                    show_copyable_path(ui, &app.settings_path().display().to_string());
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
    });
}

fn collect_active_status_details(
    app: &ScratchpadApp,
    dark_mode: bool,
) -> Option<ActiveStatusDetails> {
    let tab = app.active_tab()?;
    let buffer = tab.active_buffer();
    let file_length = buffer.current_file_length();
    let active_view = tab.active_view();
    let view_status = active_view
        .map(|view| buffer.view_status(status_cursor_range(view)))
        .unwrap_or_default();
    let (icon, icon_tooltip, icon_color) = artifact_icon(buffer.show_control_chars, dark_mode);

    Some(ActiveStatusDetails {
        path_label: buffer
            .path
            .as_ref()
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Untitled".to_owned()),
        count_label: line_count_label(file_length.lines),
        cursor_label: cursor_label(&view_status),
        selection_label: selection_label(&view_status),
        encoding_label: buffer.format.encoding_label(),
        encoding_tooltip: buffer.format.encoding_tooltip(),
        encoding_is_non_default: status_bar_encoding_is_non_default(&buffer.format),
        has_non_compliant_characters: buffer.has_non_compliant_characters,
        line_endings_label: buffer.format.line_endings_label().to_owned(),
        line_endings_are_non_default: buffer.format.preferred_line_ending_style()
            != platform_default_line_ending(),
        icon,
        icon_tooltip,
        icon_color,
        control_chars_available: buffer.has_visible_control_substitutions(),
        freshness_label: visible_disk_status_label(buffer).map(str::to_owned),
    })
}

fn status_cursor_range(
    view: &crate::app::domain::EditorViewState,
) -> Option<crate::app::ui::editor_content::native_editor::CursorRange> {
    view.cursor_range.or(view.pending_cursor_range)
}

fn render_active_status(
    ui: &mut egui::Ui,
    details: &ActiveStatusDetails,
    actions: &mut StatusBarActions,
) {
    show_copyable_path(ui, &format!("Path: {}", details.path_label));
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        show_status_warnings(ui, details);
        show_settings_button(ui, actions);
        show_text_history_button(ui, actions);
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
        show_status_segment(ui, details.selection_label.as_deref());
        show_status_segment(ui, details.cursor_label.as_deref());
        show_line_count(ui, &details.count_label, actions);
    });
}

fn show_copyable_path(ui: &mut egui::Ui, label: &str) {
    let response = widget_ids::surface_response(ui, "status_path", "copyable_path", |ui| {
        ui.add(
            egui::Button::new(label)
                .frame(false)
                .stroke(egui::Stroke::NONE)
                .fill(egui::Color32::TRANSPARENT)
                .min_size(egui::vec2(0.0, 22.0)),
        )
    })
    .on_hover_text("Double-click to copy path");
    if response.double_clicked() {
        let copied = label.strip_prefix("Path: ").unwrap_or(label);
        ui.copy_text(copied.to_owned());
    }
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

fn show_encoding(
    ui: &mut egui::Ui,
    encoding: &str,
    tooltip: &str,
    highlight: bool,
) -> egui::Response {
    ui.separator();
    widget_ids::surface_response(ui, "status_encoding", "encoding_label", |ui| {
        ui.add(
            egui::Label::new(status_format_text(ui, encoding, highlight))
                .sense(egui::Sense::click()),
        )
    })
    .on_hover_cursor(egui::CursorIcon::PointingHand)
    .on_hover_text(format!("{tooltip}\nClick for encoding actions"))
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

fn status_bar_encoding_is_non_default(format: &crate::app::domain::TextFormatMetadata) -> bool {
    !format.encoding_name.eq_ignore_ascii_case("UTF-8") || format.has_bom
}

fn show_settings_button(ui: &mut egui::Ui, actions: &mut StatusBarActions) {
    ui.separator();
    let response = status_bar_icon_button(ui, "status_settings", egui_phosphor::regular::GEAR)
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text("Open settings");
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
    .on_hover_text("Open text history");
    if response.clicked() {
        actions.open_text_history = true;
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
    let button_response =
        fixed_status_icon_cell(ui, "status_control_chars", details.icon, details.icon_color);
    if button_response.hovered() {
        let tooltip = if details.control_chars_available || details.icon == CONTROL_CHAR_ICON {
            details.icon_tooltip
        } else {
            "No control characters to show"
        };
        button_response.clone().on_hover_text(tooltip);
    }
    if button_response.clicked()
        && (details.control_chars_available || details.icon == CONTROL_CHAR_ICON)
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

fn show_status_warnings(ui: &mut egui::Ui, details: &ActiveStatusDetails) {
    if let Some(freshness_label) = &details.freshness_label {
        ui.separator();
        widget_ids::surface_response(
            ui,
            "status_freshness_warning",
            widget_ids::WidgetRole::Label,
            |ui| ui.label(egui::RichText::new(freshness_label).color(egui::Color32::YELLOW)),
        );
    }

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

    if actions.open_settings {
        app.handle_command(AppCommand::OpenSettings);
    }
}

fn line_count_label(line_count: usize) -> String {
    format!("Lines: {line_count}")
}

fn cursor_label(status: &BufferViewStatus) -> Option<String> {
    Some(format!(
        "Ln {}, Col {}",
        status.cursor_line?, status.cursor_column?
    ))
}

fn selection_label(status: &BufferViewStatus) -> Option<String> {
    (status.selection_chars > 0).then_some(format!("Sel {}", status.selection_chars))
}

fn visible_disk_status_label(buffer: &BufferState) -> Option<&'static str> {
    if buffer.freshness == BufferFreshness::MissingOnDisk
        && buffer.path.as_ref().is_some_and(|path| path.exists())
    {
        return None;
    }

    buffer.disk_status_label()
}

fn artifact_icon(
    show_control_chars: bool,
    dark_mode: bool,
) -> (&'static str, &'static str, egui::Color32) {
    if show_control_chars {
        (
            CONTROL_CHAR_ICON,
            "Hide Control Chars",
            egui::Color32::YELLOW,
        )
    } else {
        (
            HIDDEN_CONTROL_CHAR_ICON,
            "Show Control Chars",
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

#[cfg(test)]
mod tests {
    use super::{
        CONTROL_CHAR_ICON, HIDDEN_CONTROL_CHAR_ICON, artifact_icon, plain_text_icon_color,
        status_attention_color, status_cursor_range,
    };
    use crate::app::domain::EditorViewState;
    use crate::app::ui::editor_content::native_editor::{CharCursor, CursorRange};

    #[test]
    fn status_prefers_live_cursor_over_pending_cursor() {
        let mut view = EditorViewState::new(1);
        view.cursor_range = Some(CursorRange::two(0, 8));
        view.pending_cursor_range = Some(CursorRange::one(CharCursor::new(3)));

        assert_eq!(status_cursor_range(&view), Some(CursorRange::two(0, 8)));
    }

    #[test]
    fn light_plain_text_icon_is_dark_enough_to_see() {
        let color = plain_text_icon_color(false);

        assert!(color.r() < 80);
        assert!(color.g() < 90);
        assert!(color.b() < 100);
    }

    #[test]
    fn light_status_attention_color_is_readable_on_light_status_bar() {
        let color = status_attention_color(false);

        assert!(color.r() < 180);
        assert!(color.g() < 130);
        assert!(color.b() < 40);
    }

    #[test]
    fn dark_status_attention_color_stays_warm_without_neon_yellow() {
        let color = status_attention_color(true);

        assert!(color.r() >= 220);
        assert!(color.g() >= 170);
        assert!(color.b() >= 60);
        assert!(color.b() < 140);
    }

    #[test]
    fn control_character_status_uses_conventional_marker() {
        assert_eq!(artifact_icon(false, true).0, HIDDEN_CONTROL_CHAR_ICON);
        assert_eq!(artifact_icon(true, true).0, CONTROL_CHAR_ICON);
    }
}
