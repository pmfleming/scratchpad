use super::native_editor::{EditorWidgetOutcome, TextEditOptions, render_read_only_text_edit};
use crate::app::domain::{BufferState, EditorViewState, RenderedLayout};

pub fn render_artifact_view(
    ui: &mut eframe::egui::Ui,
    buffer: &mut BufferState,
    view: &mut EditorViewState,
    previous_layout: Option<&RenderedLayout>,
    options: TextEditOptions<'_>,
) -> EditorWidgetOutcome {
    if view.show_control_chars {
        try_render_visible_artifact_window(
            ui,
            buffer,
            view,
            previous_layout,
            options,
            make_control_chars_visible,
        )
        .unwrap_or_else(|| {
            render_read_only_text_edit(
                ui,
                view,
                make_control_chars_visible(&buffer.text()),
                buffer.line_count,
                options,
            )
        })
    } else {
        try_render_visible_artifact_window(
            ui,
            buffer,
            view,
            previous_layout,
            options,
            make_control_chars_clean,
        )
        .unwrap_or_else(|| {
            let clean_text = make_control_chars_clean(&buffer.text());
            render_read_only_text_edit(ui, view, clean_text, buffer.line_count, options)
        })
    }
}

fn try_render_visible_artifact_window(
    ui: &mut eframe::egui::Ui,
    buffer: &BufferState,
    view: &mut EditorViewState,
    previous_layout: Option<&RenderedLayout>,
    options: TextEditOptions<'_>,
    transform: impl Fn(&str) -> String,
) -> Option<EditorWidgetOutcome> {
    if options.word_wrap {
        return None;
    }

    let visible_lines = previous_layout?.visible_line_range();
    if visible_lines.is_empty() {
        return None;
    }

    let mut visible_window = buffer.visible_line_window(visible_lines.clone());
    let top_padding_lines = visible_window.line_range.start;
    let bottom_padding_lines = buffer
        .line_count
        .saturating_sub(visible_window.line_range.end);
    let row_height = ui.fonts_mut(|fonts| fonts.row_height(options.editor_font_id));

    if top_padding_lines > 0 {
        ui.add_space(row_height * top_padding_lines as f32);
    }

    let outcome = render_read_only_text_edit(
        ui,
        view,
        transform(&visible_window.text),
        visible_window.line_range.len().max(1),
        options,
    );
    if let Some(layout) = view.latest_layout.as_mut() {
        layout.offset_line_numbers(visible_window.line_range.start);
        visible_window.row_range = 0..layout.row_count();
        layout.set_visible_text(visible_window);
    }

    if bottom_padding_lines > 0 {
        ui.add_space(row_height * bottom_padding_lines as f32);
    }

    Some(outcome)
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
                use std::fmt::Write;
                let _ = write!(visible, "\\x{:02X}", ch as u32);
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
            '\r' => clean.push('\n'),
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

#[cfg(test)]
mod tests {
    use super::{make_control_chars_clean, make_control_chars_visible};

    #[test]
    fn visible_control_char_rendering_preserves_line_count() {
        let text = "alpha\tbeta\n\x1bgamma";
        let rendered = make_control_chars_visible(text);

        assert_eq!(text.lines().count(), rendered.lines().count());
    }

    #[test]
    fn clean_control_char_rendering_preserves_carriage_return_line_breaks() {
        let text = "alpha\rbeta\r\ngamma";
        let rendered = make_control_chars_clean(text);

        assert_eq!(rendered, "alpha\nbeta\ngamma");
        assert_eq!(rendered.lines().count(), 3);
    }
}
