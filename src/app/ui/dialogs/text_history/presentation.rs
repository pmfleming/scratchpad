//! Read-only labels for the controls; persisted operation summaries stay intact.
use super::style;
use crate::app::{
    domain::{BufferState, buffer::PieceSource},
    theme,
};
use eframe::egui;
use egui_phosphor::regular as icons;

pub(super) struct Change {
    pub title: String,
    pub icon: &'static str,
    pub color: egui::Color32,
    pub edited_at: u64,
    whitespace: bool,
}

impl Change {
    pub(super) fn display_title(&self) -> &str {
        ["Insert ", "Delete ", "Paste ", "Cut ", "Replace with "]
            .iter()
            .find_map(|prefix| self.title.strip_prefix(prefix))
            .map_or(self.title.as_str(), |text| {
                text.strip_prefix('"')
                    .and_then(|text| text.strip_suffix('"'))
                    .unwrap_or(text)
            })
    }
    pub(super) fn paint_title(&self, ui: &egui::Ui, rect: egui::Rect, color: egui::Color32) {
        let title = self.display_title();
        if self.whitespace {
            let width = ui
                .painter()
                .layout_no_wrap(title.to_owned(), style::font(ui, 11.0), color)
                .size()
                .x
                + 12.0;
            let token = egui::Rect::from_center_size(
                egui::pos2(rect.left() + width.min(rect.width()) * 0.5, rect.center().y),
                egui::vec2(width.min(rect.width()).max(0.0), 18.0),
            );
            ui.painter().rect_filled(token, 0, theme::action_bg(ui));
            style::text(ui, token.shrink2(egui::vec2(6.0, 0.0)), title, 11.0, color);
        } else {
            style::text(ui, rect, title, 13.0, color);
        }
    }
}

pub(super) fn change(ui: &egui::Ui, buffer: &BufferState, id: u64) -> Change {
    let entries = buffer.document().history_entries();
    let entry = entries
        .binary_search_by_key(&id, |entry| entry.id)
        .ok()
        .map(|i| &entries[i]);
    let Some(entry) = entry else {
        return Change {
            title: "Empty document".into(),
            icon: icons::FILE,
            color: theme::text_muted(ui),
            edited_at: 0,
            whitespace: false,
        };
    };
    let (icon, color) = if entry.summary == "Initial contents" {
        (icons::FILE_TEXT, theme::text_muted(ui))
    } else if entry.source == PieceSource::Paste {
        (icons::CLIPBOARD_TEXT, theme::diff_add(ui))
    } else if entry.source == PieceSource::Cut {
        (icons::SCISSORS, theme::diff_del(ui))
    } else if entry.source == PieceSource::SearchReplace {
        (icons::MAGNIFYING_GLASS, theme::tab_selected_accent(ui))
    } else if entry.edits.len() == 1 && entry.edits[0].deleted_spans().is_empty() {
        (icons::PLUS, theme::diff_add(ui))
    } else if entry.edits.len() == 1 && entry.edits[0].inserted_span().is_none() {
        (icons::MINUS, theme::diff_del(ui))
    } else {
        (icons::SWAP, theme::tab_selected_accent(ui))
    };
    let mut title = entry.summary.clone();
    let mut is_whitespace = false;
    // A short, single payload can be classified without reconstructing a document
    // or allocating a potentially large paste/deletion. Larger/mixed edits retain
    // their existing summary rather than claiming a misleading whitespace token.
    if entry.edits.len() == 1 && entry.source != PieceSource::Load {
        let edit = &entry.edits[0];
        let mut spans = edit.spans();
        if let Some(span) = spans.next()
            && spans.next().is_none()
            && span.byte_len <= 256
        {
            let payload = buffer.document().piece_tree().text_for_span(span);
            if let Some(token) = whitespace(&payload) {
                let verb = if entry.source == PieceSource::Paste {
                    "Paste"
                } else if entry.source == PieceSource::Cut {
                    "Cut"
                } else if edit.inserted_span().is_some() {
                    "Insert"
                } else {
                    "Delete"
                };
                title = format!("{verb} {token}");
                is_whitespace = true;
            }
        }
    }
    Change {
        title,
        icon,
        color,
        edited_at: entry.edited_at,
        whitespace: is_whitespace,
    }
}

fn whitespace(text: &str) -> Option<String> {
    let (unit, count) = if !text.is_empty() && text.chars().all(|c| c == ' ') {
        ("space", text.len())
    } else if !text.is_empty() && text.chars().all(|c| c == '\t') {
        ("tab", text.len())
    } else if !text.is_empty() && text.replace("\r\n", "\n").chars().all(|c| c == '\n') {
        ("newline", text.bytes().filter(|&b| b == b'\n').count())
    } else {
        return None;
    };
    Some(if count == 1 {
        unit.into()
    } else {
        format!("{count} {unit}s")
    })
}

pub(super) fn age(seconds: u64) -> String {
    if seconds == 0 {
        return "Not edited".into();
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    match now.saturating_sub(seconds) {
        0..60 => "Now".into(),
        n @ 60..3600 => format!("{}m ago", n / 60),
        n @ 3600..86400 => format!("{}h ago", n / 3600),
        n => format!("{}d ago", n / 86400),
    }
}

pub(super) fn clock(seconds: u64) -> String {
    if seconds == 0 {
        return String::new();
    }
    clock_at(seconds, chrono::Local::now())
}

pub(super) fn clock_at(seconds: u64, now: chrono::DateTime<chrono::Local>) -> String {
    use chrono::Datelike;
    chrono::DateTime::from_timestamp(seconds as i64, 0).map_or_else(String::new, |date| {
        let date = date.with_timezone(&chrono::Local);
        date.format(if date.date_naive() == now.date_naive() {
            "%H:%M"
        } else if date.year() == now.year() {
            "%b %-d"
        } else {
            "%b %-d %Y"
        })
        .to_string()
    })
}

pub(super) fn full_time(seconds: u64) -> String {
    chrono::DateTime::from_timestamp(seconds as i64, 0).map_or_else(String::new, |date| {
        date.with_timezone(&chrono::Local)
            .format("%Y-%m-%d %H:%M:%S %:z")
            .to_string()
    })
}
