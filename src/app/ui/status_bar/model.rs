use super::{
    CONTROL_CHAR_ICON, HIDDEN_CONTROL_CHAR_ICON, STATUS_ICON_CELL_SIZE, STATUS_PATH_MIN_WIDTH,
    STATUS_SEPARATOR_VISUAL_WIDTH, status_attention_color,
};
use crate::app::app_state::ScratchpadApp;
use crate::app::domain::{
    BufferFreshness, BufferState, BufferViewStatus, platform_default_line_ending,
};
use crate::app::services::settings_store::TabListPosition;
use crate::app::theme::{TEXT_MUTED, TEXT_PRIMARY, text_primary};
use eframe::egui;

pub(super) struct ActiveStatusDetails {
    pub(super) path_label: String,
    pub(super) count_label: String,
    pub(super) cursor_label: Option<String>,
    pub(super) selection_label: Option<String>,
    pub(super) encoding: EncodingStatus,
    pub(super) has_non_compliant_characters: bool,
    pub(super) line_endings: LineEndingStatus,
    pub(super) control_chars: ControlCharStatus,
    pub(super) disk: DiskStatus,
}

pub(super) struct EncodingStatus {
    pub(super) label: String,
    pub(super) is_non_default: bool,
}

pub(super) struct LineEndingStatus {
    pub(super) label: String,
    pub(super) is_non_default: bool,
}

pub(super) struct ControlCharStatus {
    pub(super) icon: &'static str,
    pub(super) icon_tooltip: &'static str,
    pub(super) icon_color: egui::Color32,
    pub(super) is_available: bool,
}

pub(super) struct DiskStatus {
    pub(super) label: String,
    pub(super) color: egui::Color32,
}

#[derive(Clone, Copy)]
pub(super) enum StatusBarItemKind {
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

pub(super) struct StatusBarItem {
    pub(super) kind: StatusBarItemKind,
    pub(super) width: f32,
}

#[derive(Debug, PartialEq)]
pub(super) struct StatusBarPathLayout {
    pub(super) visible_start: usize,
    pub(super) path_width: f32,
}

pub(super) fn collect_active_status_details(
    app: &ScratchpadApp,
    dark_mode: bool,
) -> Option<ActiveStatusDetails> {
    let tab = app.tab_manager.active_tab()?;
    let buffer = tab.active_buffer();
    let file_length = buffer.current_file_length();
    let view_status = tab
        .active_view()
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
        encoding: EncodingStatus {
            label: buffer.format.encoding_label(),
            is_non_default: status_bar_encoding_is_non_default(&buffer.format),
        },
        has_non_compliant_characters: buffer.has_non_compliant_characters,
        line_endings: LineEndingStatus {
            label: buffer.format.line_endings_label().to_owned(),
            is_non_default: buffer.format.preferred_line_ending_style()
                != platform_default_line_ending(),
        },
        control_chars: ControlCharStatus {
            icon,
            icon_tooltip,
            icon_color,
            is_available: buffer.has_visible_control_substitutions(),
        },
        disk: DiskStatus {
            label: disk_state_label(buffer),
            color: disk_state_color(buffer, dark_mode),
        },
    })
}

pub(super) fn status_cursor_range(
    view: &crate::app::domain::EditorViewState,
) -> Option<crate::app::ui::editor_content::native_editor::CursorRange> {
    view.cursor_range.or(view.pending_cursor_range)
}

pub(super) fn active_status_items(
    ui: &egui::Ui,
    details: &ActiveStatusDetails,
) -> Vec<StatusBarItem> {
    let mut items = vec![StatusBarItem {
        kind: StatusBarItemKind::LineCount,
        width: estimated_text_item_width(ui, &details.count_label),
    }];

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

    items.extend([
        StatusBarItem {
            kind: StatusBarItemKind::DiskState,
            width: estimated_text_item_width(ui, &details.disk.label),
        },
        StatusBarItem {
            kind: StatusBarItemKind::Encoding,
            width: estimated_text_item_width(ui, &details.encoding.label),
        },
        StatusBarItem {
            kind: StatusBarItemKind::LineEndings,
            width: estimated_text_item_width(ui, &format!("EOL: {}", details.line_endings.label)),
        },
        StatusBarItem {
            kind: StatusBarItemKind::ControlChars,
            width: estimated_icon_item_width(ui),
        },
        StatusBarItem {
            kind: StatusBarItemKind::TextHistory,
            width: estimated_icon_item_width(ui),
        },
        StatusBarItem {
            kind: StatusBarItemKind::Settings,
            width: estimated_icon_item_width(ui),
        },
    ]);

    if details.has_non_compliant_characters {
        items.push(StatusBarItem {
            kind: StatusBarItemKind::NonCompliant,
            width: estimated_text_item_width(ui, "Non compliant characters"),
        });
    }
    items
}

pub(super) fn status_path_min_width(ui: &egui::Ui, app: &ScratchpadApp) -> f32 {
    if app.state.app_settings.tab_list_position() != TabListPosition::Left {
        return STATUS_PATH_MIN_WIDTH;
    }

    let path_start_x = ui.available_rect_before_wrap().min.x;
    let tab_border_x =
        ui.max_rect().left() + crate::app::app_state::settings_state::vertical_tab_list_width(app);
    STATUS_PATH_MIN_WIDTH.max(tab_border_x - path_start_x)
}

pub(super) fn status_bar_path_layout(
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

pub(super) fn left_truncated_text_for_width(ui: &egui::Ui, text: &str, width: f32) -> String {
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

fn status_bar_encoding_is_non_default(format: &crate::app::domain::TextFormatMetadata) -> bool {
    !format.encoding_name.eq_ignore_ascii_case("UTF-8") || format.has_bom
}

pub(super) fn artifact_icon(
    show_control_chars: bool,
    dark_mode: bool,
) -> (&'static str, &'static str, egui::Color32) {
    if show_control_chars {
        (
            CONTROL_CHAR_ICON,
            "Control characters",
            status_attention_color(dark_mode),
        )
    } else {
        (
            HIDDEN_CONTROL_CHAR_ICON,
            "Control characters",
            plain_text_icon_color(dark_mode),
        )
    }
}

pub(super) fn plain_text_icon_color(dark_mode: bool) -> egui::Color32 {
    if dark_mode {
        TEXT_PRIMARY.gamma_multiply(0.8)
    } else {
        egui::Color32::from_rgb(28, 35, 45).gamma_multiply(0.8)
    }
}
