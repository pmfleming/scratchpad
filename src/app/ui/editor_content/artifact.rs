use super::native_editor::{EditorWidgetOutcome, TextEditOptions, render_read_only_text_edit};
use crate::app::domain::{BufferState, EditorViewState};

pub fn render_artifact_view(
    ui: &mut eframe::egui::Ui,
    buffer: &mut BufferState,
    view: &mut EditorViewState,
    options: TextEditOptions<'_>,
) -> EditorWidgetOutcome {
    let transform: fn(&str) -> String = if view.show_control_chars {
        make_control_chars_visible
    } else {
        make_control_chars_clean
    };
    let text = transform(&buffer.text());
    render_read_only_text_edit(ui, view, text, buffer.line_count, options)
}

pub fn make_control_chars_visible(text: &str) -> String {
    let mut visible = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();

    while let Some(ch) = chars.next() {
        push_visible_char(&mut visible, ch, chars.peek().copied());
    }

    visible
}

fn push_visible_char(visible: &mut String, ch: char, next: Option<char>) {
    match visible_control_char(ch, next) {
        Some(ControlCharDisplay::Literal(replacement)) => visible.push(replacement),
        Some(ControlCharDisplay::Hex) => {
            use std::fmt::Write;
            let _ = write!(visible, "\\x{:02X}", ch as u32);
        }
        None => visible.push(ch),
    }
}

enum ControlCharDisplay {
    Literal(char),
    Hex,
}

fn visible_control_char(ch: char, next: Option<char>) -> Option<ControlCharDisplay> {
    match ch {
        '\u{1B}' => Some(ControlCharDisplay::Literal('␛')),
        '\u{0008}' => Some(ControlCharDisplay::Literal('␈')),
        '\t' => Some(ControlCharDisplay::Literal('→')),
        '\r' if next == Some('\n') => Some(ControlCharDisplay::Literal('␍')),
        '\r' => Some(ControlCharDisplay::Literal('␍')),
        '\n' => None,
        _ if ch.is_control() => Some(ControlCharDisplay::Hex),
        _ => None,
    }
}

pub fn make_control_chars_clean(text: &str) -> String {
    let mut clean = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();

    while let Some(ch) = chars.next() {
        push_clean_char(&mut clean, ch, &mut chars);
    }

    clean
}

fn push_clean_char(
    clean: &mut String,
    ch: char,
    chars: &mut std::iter::Peekable<std::str::Chars<'_>>,
) {
    match ch {
        '\u{1B}' => skip_ansi_sequence(chars),
        '\u{0008}' => {
            clean.pop();
        }
        '\r' if chars.peek() == Some(&'\n') => {}
        '\r' => clean.push('\n'),
        '\n' | '\t' => clean.push(ch),
        _ if ch.is_control() => {}
        _ => clean.push(ch),
    }
}

fn skip_ansi_sequence(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) {
    if chars.next_if_eq(&'[').is_some() {
        skip_csi_sequence(chars);
    } else if chars.next_if_eq(&']').is_some() {
        skip_osc_sequence(chars);
    }
}

fn skip_csi_sequence(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) {
    for ch in chars.by_ref() {
        if ('@'..='~').contains(&ch) {
            break;
        }
    }
}

fn skip_osc_sequence(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) {
    while let Some(ch) = chars.next() {
        if ch == '\u{0007}' {
            break;
        }
        if ch == '\u{1B}' && chars.next_if_eq(&'\\').is_some() {
            break;
        }
    }
}
