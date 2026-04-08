use super::text_edit::render_read_only_text_edit;
use crate::app::domain::{BufferState, EditorViewState, display_line_count};
use eframe::egui;

pub fn render_artifact_view(
    ui: &mut egui::Ui,
    buffer: &mut BufferState,
    view: &mut EditorViewState,
    word_wrap: bool,
    editor_font_id: &egui::FontId,
) -> (bool, bool) {
    let focused = if view.show_control_chars {
        render_read_only_text_edit(
            ui,
            view,
            make_control_chars_visible(&buffer.content),
            buffer.line_count,
            word_wrap,
            editor_font_id,
        )
    } else {
        let clean_text = make_control_chars_clean(&buffer.content);
        let desired_rows = display_line_count(&clean_text);
        render_read_only_text_edit(
            ui,
            view,
            clean_text,
            desired_rows,
            word_wrap,
            editor_font_id,
        )
    };

    (false, focused)
}

pub fn make_control_chars_visible(text: &str) -> String {
    let mut visible = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            '\u{1B}' => visible.push('␛'),
            '\u{0008}' => visible.push('␈'),
            '\t' => visible.push('→'),
            '\r' if chars.peek() == Some(&'\n') => visible.push('␍'),
            '\r' => visible.push('␍'),
            _ if ch.is_control() && ch != '\n' => {
                visible.push_str(&format!("\\x{:02X}", ch as u32));
            }
            _ => visible.push(ch),
        }
    }

    visible
}

pub fn make_control_chars_clean(text: &str) -> String {
    let mut clean = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            '\u{1B}' => skip_ansi_sequence(&mut chars),
            '\u{0008}' => {
                clean.pop();
            }
            '\r' if chars.peek() == Some(&'\n') => {}
            '\r' => {}
            '\n' | '\t' => clean.push(ch),
            _ if ch.is_control() => {}
            _ => clean.push(ch),
        }
    }

    clean
}

fn skip_ansi_sequence(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) {
    match chars.peek().copied() {
        Some('[') => {
            chars.next();
            for ch in chars.by_ref() {
                if ('@'..='~').contains(&ch) {
                    break;
                }
            }
        }
        Some(']') => {
            chars.next();
            while let Some(ch) = chars.next() {
                if ch == '\u{0007}' {
                    break;
                }
                if ch == '\u{1B}' && chars.peek() == Some(&'\\') {
                    chars.next();
                    break;
                }
            }
        }
        _ => {}
    }
}
