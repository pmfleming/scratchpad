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
