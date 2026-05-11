use crate::app::app_state::{ScratchpadApp, StatusSeverity};
use crate::app::commands::AppCommand;
use crate::app::domain::{
    BufferFreshness, BufferState, BufferViewStatus, platform_default_line_ending,
};
use crate::app::services::settings_store::TabListPosition;
use crate::app::theme::*;
use crate::app::ui::widget_ids;
use eframe::egui;

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
    disk_state_label: String,
    disk_state_color: egui::Color32,
}

#[derive(Clone, Copy)]
enum StatusBarItemKind {
    LineCount,
    Cursor,
    Selection,
    DiskState,
    Encoding,
    LineEndings,
    ControlChars,
    TextHistory,
    Settings,
    NonCompliant,
}

struct StatusBarItem {
    kind: StatusBarItemKind,
    width: f32,
}

#[derive(Debug, PartialEq)]
struct StatusBarPathLayout {
    visible_start: usize,
    path_width: f32,
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
        disk_state_label: disk_state_label(buffer),
        disk_state_color: disk_state_color(buffer, dark_mode),
    })
}

fn status_cursor_range(
    view: &crate::app::domain::EditorViewState,
) -> Option<crate::app::ui::editor_content::native_editor::CursorRange> {
    view.cursor_range.or(view.pending_cursor_range)
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
        .status_history
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
    let button_response =
        fixed_status_icon_cell(ui, "status_control_chars", details.icon, details.icon_color);
    if button_response.hovered() {
        let tooltip = if details.control_chars_available || details.icon == CONTROL_CHAR_ICON {
            details.icon_tooltip
        } else {
            "Control characters"
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

fn show_disk_state(ui: &mut egui::Ui, details: &ActiveStatusDetails) {
    ui.separator();
    widget_ids::surface_response(
        ui,
        "status_disk_state",
        widget_ids::WidgetRole::Label,
        |ui| {
            ui.label(egui::RichText::new(&details.disk_state_label).color(details.disk_state_color))
        },
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

fn active_status_items(ui: &egui::Ui, details: &ActiveStatusDetails) -> Vec<StatusBarItem> {
    let mut items = Vec::new();
    items.push(StatusBarItem {
        kind: StatusBarItemKind::LineCount,
        width: estimated_text_item_width(ui, &details.count_label),
    });
    if let Some(label) = details.cursor_label.as_deref() {
        items.push(StatusBarItem {
            kind: StatusBarItemKind::Cursor,
            width: estimated_text_item_width(ui, label),
        });
    }
    if let Some(label) = details.selection_label.as_deref() {
        items.push(StatusBarItem {
            kind: StatusBarItemKind::Selection,
            width: estimated_text_item_width(ui, label),
        });
    }
    items.push(StatusBarItem {
        kind: StatusBarItemKind::DiskState,
        width: estimated_text_item_width(ui, &details.disk_state_label),
    });
    items.push(StatusBarItem {
        kind: StatusBarItemKind::Encoding,
        width: estimated_text_item_width(ui, &details.encoding_label),
    });
    items.push(StatusBarItem {
        kind: StatusBarItemKind::LineEndings,
        width: estimated_text_item_width(ui, &format!("EOL: {}", details.line_endings_label)),
    });
    items.push(StatusBarItem {
        kind: StatusBarItemKind::ControlChars,
        width: estimated_icon_item_width(ui),
    });
    items.push(StatusBarItem {
        kind: StatusBarItemKind::TextHistory,
        width: estimated_icon_item_width(ui),
    });
    items.push(StatusBarItem {
        kind: StatusBarItemKind::Settings,
        width: estimated_icon_item_width(ui),
    });
    if details.has_non_compliant_characters {
        items.push(StatusBarItem {
            kind: StatusBarItemKind::NonCompliant,
            width: estimated_text_item_width(ui, "Non compliant characters"),
        });
    }
    items
}

fn status_path_min_width(ui: &egui::Ui, app: &ScratchpadApp) -> f32 {
    if app.tab_list_position() != TabListPosition::Left {
        return STATUS_PATH_MIN_WIDTH;
    }

    let path_start_x = ui.available_rect_before_wrap().min.x;
    let tab_border_x = ui.max_rect().left() + app.vertical_tab_list_width();
    STATUS_PATH_MIN_WIDTH.max(tab_border_x - path_start_x)
}

fn status_bar_path_layout(
    available_width: f32,
    path_min_width: f32,
    items: &[StatusBarItem],
) -> StatusBarPathLayout {
    let mut first_visible = 0;
    let mut item_width = items.iter().map(|item| item.width).sum::<f32>();
    if available_width >= path_min_width + item_width {
        return StatusBarPathLayout {
            visible_start: 0,
            path_width: available_width - item_width,
        };
    }

    while first_visible < items.len() && available_width < path_min_width + item_width {
        item_width -= items[first_visible].width;
        first_visible += 1;
    }

    StatusBarPathLayout {
        visible_start: first_visible,
        path_width: path_min_width.min(available_width),
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
            if show_encoding(
                ui,
                &details.encoding_label,
                &details.encoding_tooltip,
                details.encoding_is_non_default,
            )
            .clicked()
            {
                actions.open_encoding_dialog = true;
            }
        }
        StatusBarItemKind::LineEndings => show_line_endings(
            ui,
            &details.line_endings_label,
            details.line_endings_are_non_default,
        ),
        StatusBarItemKind::ControlChars => show_control_char_toggle(ui, details, actions),
        StatusBarItemKind::TextHistory => show_text_history_button(ui, actions),
        StatusBarItemKind::Settings => show_settings_button(ui, actions),
        StatusBarItemKind::NonCompliant => show_non_compliant_warning(ui, details),
    }
}

fn estimated_text_item_width(ui: &egui::Ui, text: &str) -> f32 {
    status_segment_prefix_width(ui)
        + text_width(ui, text, egui::TextStyle::Button.resolve(ui.style()))
}

fn estimated_icon_item_width(ui: &egui::Ui) -> f32 {
    status_segment_prefix_width(ui) + STATUS_ICON_CELL_SIZE.x
}

fn status_segment_prefix_width(ui: &egui::Ui) -> f32 {
    ui.spacing().item_spacing.x * 2.0 + STATUS_SEPARATOR_VISUAL_WIDTH
}

fn left_truncated_text_for_width(ui: &egui::Ui, text: &str, width: f32) -> String {
    let font_id = egui::TextStyle::Button.resolve(ui.style());
    if width <= 0.0 {
        return String::new();
    }
    if text_width(ui, text, font_id.clone()) <= width {
        return text.to_owned();
    }

    let marker = "...";
    if text_width(ui, marker, font_id.clone()) >= width {
        return marker.to_owned();
    }

    let chars = text.chars().collect::<Vec<_>>();
    let mut suffix_start = 0;
    loop {
        let suffix = chars[suffix_start..].iter().collect::<String>();
        let candidate = format!("{marker}{suffix}");
        if text_width(ui, &candidate, font_id.clone()) <= width {
            return candidate;
        }
        if suffix_start == chars.len() {
            return marker.to_owned();
        }
        suffix_start += 1;
    }
}

fn text_width(ui: &egui::Ui, text: &str, font_id: egui::FontId) -> f32 {
    ui.fonts_mut(|fonts| {
        fonts
            .layout_no_wrap(text.to_owned(), font_id, text_primary(ui))
            .size()
            .x
    })
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

fn disk_state_label(buffer: &BufferState) -> String {
    if buffer.path.is_none() {
        return "Unsaved edits".to_owned();
    }

    if buffer.freshness == BufferFreshness::MissingOnDisk
        && buffer.path.as_ref().is_some_and(|path| path.exists())
    {
        return if buffer.is_dirty {
            "Unsaved edits".to_owned()
        } else {
            "Saved".to_owned()
        };
    }

    match buffer.freshness {
        BufferFreshness::InSync if buffer.is_dirty => "Unsaved edits".to_owned(),
        BufferFreshness::InSync => "Saved".to_owned(),
        BufferFreshness::AutoReloaded => "Changed on disk".to_owned(),
        BufferFreshness::StaleOnDisk => "Changed on disk; reload failed".to_owned(),
        BufferFreshness::ConflictOnDisk => "Conflict; choose save action".to_owned(),
        BufferFreshness::MissingOnDisk => "Missing".to_owned(),
    }
}

fn disk_state_color(buffer: &BufferState, dark_mode: bool) -> egui::Color32 {
    match buffer.freshness {
        BufferFreshness::ConflictOnDisk
        | BufferFreshness::MissingOnDisk
        | BufferFreshness::StaleOnDisk => egui::Color32::from_rgb(220, 64, 64),
        BufferFreshness::AutoReloaded => egui::Color32::from_rgb(230, 132, 46),
        BufferFreshness::InSync if buffer.is_dirty => egui::Color32::from_rgb(70, 176, 96),
        BufferFreshness::InSync => {
            if dark_mode {
                TEXT_MUTED
            } else {
                egui::Color32::from_rgb(80, 91, 108)
            }
        }
    }
}

fn artifact_icon(
    show_control_chars: bool,
    dark_mode: bool,
) -> (&'static str, &'static str, egui::Color32) {
    if show_control_chars {
        (
            CONTROL_CHAR_ICON,
            "Control characters",
            egui::Color32::YELLOW,
        )
    } else {
        (
            HIDDEN_CONTROL_CHAR_ICON,
            "Control characters",
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
        CONTROL_CHAR_ICON, HIDDEN_CONTROL_CHAR_ICON, STATUS_PATH_MIN_WIDTH, StatusBarItem,
        StatusBarItemKind, StatusBarPathLayout, artifact_icon, plain_text_icon_color,
        status_attention_color, status_bar_path_layout, status_cursor_range,
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

    #[test]
    fn narrow_status_bar_drops_items_from_the_left() {
        let items = [
            StatusBarItem {
                kind: StatusBarItemKind::LineCount,
                width: 40.0,
            },
            StatusBarItem {
                kind: StatusBarItemKind::Cursor,
                width: 40.0,
            },
            StatusBarItem {
                kind: StatusBarItemKind::Settings,
                width: 40.0,
            },
        ];

        assert_eq!(
            status_bar_path_layout(STATUS_PATH_MIN_WIDTH + 120.0, STATUS_PATH_MIN_WIDTH, &items),
            StatusBarPathLayout {
                visible_start: 0,
                path_width: STATUS_PATH_MIN_WIDTH,
            }
        );
        assert_eq!(
            status_bar_path_layout(STATUS_PATH_MIN_WIDTH + 80.0, STATUS_PATH_MIN_WIDTH, &items),
            StatusBarPathLayout {
                visible_start: 1,
                path_width: STATUS_PATH_MIN_WIDTH,
            }
        );
        assert_eq!(
            status_bar_path_layout(STATUS_PATH_MIN_WIDTH + 40.0, STATUS_PATH_MIN_WIDTH, &items),
            StatusBarPathLayout {
                visible_start: 2,
                path_width: STATUS_PATH_MIN_WIDTH,
            }
        );
    }

    #[test]
    fn path_width_stays_pinned_while_items_disappear() {
        let items = [
            StatusBarItem {
                kind: StatusBarItemKind::LineCount,
                width: 40.0,
            },
            StatusBarItem {
                kind: StatusBarItemKind::Cursor,
                width: 40.0,
            },
        ];

        assert_eq!(
            status_bar_path_layout(200.0, 100.0, &items),
            StatusBarPathLayout {
                visible_start: 0,
                path_width: 120.0,
            }
        );
        assert_eq!(
            status_bar_path_layout(180.0, 100.0, &items),
            StatusBarPathLayout {
                visible_start: 0,
                path_width: 100.0,
            }
        );
        assert_eq!(
            status_bar_path_layout(160.0, 100.0, &items),
            StatusBarPathLayout {
                visible_start: 1,
                path_width: 100.0,
            }
        );
        assert_eq!(
            status_bar_path_layout(120.0, 100.0, &items),
            StatusBarPathLayout {
                visible_start: 2,
                path_width: 100.0,
            }
        );
    }

    #[test]
    fn path_width_only_shrinks_below_floor_when_no_space_remains() {
        let items = [StatusBarItem {
            kind: StatusBarItemKind::LineCount,
            width: 40.0,
        }];

        assert_eq!(
            status_bar_path_layout(80.0, 100.0, &items),
            StatusBarPathLayout {
                visible_start: 1,
                path_width: 80.0,
            }
        );
    }
}
