use super::BufferState;

#[test]
fn visible_control_availability_includes_line_breaks_tabs_and_format_controls() {
    assert!(
        BufferState::new("sample.txt".to_owned(), "a\nb".to_owned(), None)
            .has_visible_control_substitutions()
    );
    assert!(
        BufferState::new("sample.txt".to_owned(), "a\tb".to_owned(), None)
            .has_visible_control_substitutions()
    );
    assert!(
        BufferState::new("sample.txt".to_owned(), "a\u{200E}b".to_owned(), None)
            .has_visible_control_substitutions()
    );
    assert!(
        BufferState::new(
            "sample.txt".to_owned(),
            "a\u{200B}\u{2060}\u{FEFF}b".to_owned(),
            None
        )
        .has_visible_control_substitutions()
    );
    assert!(
        !BufferState::new("sample.txt".to_owned(), "plain".to_owned(), None)
            .has_visible_control_substitutions()
    );
}

#[test]
fn visible_control_mode_auto_clears_when_last_substitution_disappears() {
    let mut buffer = BufferState::new("sample.txt".to_owned(), "a\u{200E}b".to_owned(), None);
    buffer.show_control_chars = true;

    buffer.replace_text("plain".to_owned());

    assert!(!buffer.show_control_chars);
}
