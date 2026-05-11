use crate::app::domain::{SearchHighlightState, SearchReplacementPreview};

#[derive(Clone, Debug)]
pub(in crate::app::ui::editor_content::native_editor) struct DisplayTextMap {
    doc_to_display: Vec<usize>,
    display_to_doc: Vec<usize>,
}

impl DisplayTextMap {
    pub(in crate::app::ui::editor_content::native_editor) fn doc_to_display_cursor(
        &self,
        cursor: usize,
    ) -> usize {
        self.doc_to_display
            .get(cursor)
            .copied()
            .unwrap_or_else(|| *self.doc_to_display.last().unwrap_or(&0))
    }

    pub(in crate::app::ui::editor_content::native_editor) fn display_to_doc_cursor(
        &self,
        cursor: usize,
    ) -> usize {
        self.display_to_doc
            .get(cursor)
            .copied()
            .unwrap_or_else(|| *self.display_to_doc.last().unwrap_or(&0))
    }

    pub(in crate::app::ui::editor_content::native_editor) fn display_len(&self) -> usize {
        self.display_to_doc.len().saturating_sub(1)
    }

    pub(in crate::app::ui::editor_content::native_editor) fn doc_range_to_display(
        &self,
        range: std::ops::Range<usize>,
    ) -> Option<std::ops::Range<usize>> {
        // Search and selection painting use this for visible spans only:
        // `None` means the range has no drawable extent for those callers.
        // Cursor code should use the cursor mapping helpers instead.
        let start = self.doc_to_display_cursor(range.start);
        let end = self.doc_to_display_cursor(range.end);
        (start < end).then_some(start..end)
    }
}

pub(super) struct DisplayTextSlice {
    pub(super) text: String,
    pub(super) map: Option<DisplayTextMap>,
}

pub(super) struct PreviewTextSlice {
    pub(super) text: String,
    pub(super) map: Option<DisplayTextMap>,
}

enum CursorSubstitutionPolicy {
    SingleCell,
    LineEndingMarker,
}

struct VisibleControlSubstitution {
    text: &'static str,
    cursor_policy: CursorSubstitutionPolicy,
}

const UNICODE_CONTROL_GLYPHS: &[(char, &str)] = &[
    ('\u{200B}', "\u{F000}"),
    ('\u{200C}', "\u{F001}"),
    ('\u{200D}', "\u{F002}"),
    ('\u{200E}', "\u{F003}"),
    ('\u{200F}', "\u{F004}"),
    ('\u{202A}', "\u{F005}"),
    ('\u{202B}', "\u{F006}"),
    ('\u{202C}', "\u{F007}"),
    ('\u{202D}', "\u{F008}"),
    ('\u{202E}', "\u{F009}"),
    ('\u{2060}', "\u{F00A}"),
    ('\u{2061}', "\u{F00B}"),
    ('\u{2062}', "\u{F00C}"),
    ('\u{2063}', "\u{F00D}"),
    ('\u{2064}', "\u{F00E}"),
    ('\u{2066}', "\u{F00F}"),
    ('\u{2067}', "\u{F010}"),
    ('\u{2068}', "\u{F011}"),
    ('\u{2069}', "\u{F012}"),
    ('\u{FEFF}', "\u{F013}"),
    ('\u{061C}', "\u{F014}"),
    ('\u{206A}', "\u{F015}"),
    ('\u{206B}', "\u{F016}"),
    ('\u{206C}', "\u{F017}"),
    ('\u{206D}', "\u{F018}"),
    ('\u{206E}', "\u{F019}"),
    ('\u{206F}', "\u{F01A}"),
];

const C0_CONTROL_PICTURES: [&str; 32] = [
    "\u{2400}", "\u{2401}", "\u{2402}", "\u{2403}", "\u{2404}", "\u{2405}", "\u{2406}", "\u{2407}",
    "\u{2408}", "\u{2409}", "\u{240A}", "\u{240B}", "\u{240C}", "\u{240D}", "\u{240E}", "\u{240F}",
    "\u{2410}", "\u{2411}", "\u{2412}", "\u{2413}", "\u{2414}", "\u{2415}", "\u{2416}", "\u{2417}",
    "\u{2418}", "\u{2419}", "\u{241A}", "\u{241B}", "\u{241C}", "\u{241D}", "\u{241E}", "\u{241F}",
];

pub(super) fn display_text_slice(text: &str, show_control_chars: bool) -> DisplayTextSlice {
    if !show_control_chars {
        return DisplayTextSlice {
            text: text.to_owned(),
            map: None,
        };
    }

    let doc_len = text.chars().count();
    let mut visible = String::with_capacity(text.len());
    let mut doc_to_display = Vec::with_capacity(doc_len + 1);
    let mut display_to_doc = vec![0];
    let mut display_chars = 0usize;
    let mut chars = text.chars().peekable();

    for doc_index in 0..doc_len {
        let ch = chars.next().unwrap_or_default();
        doc_to_display.push(display_chars);
        match visible_control_char(ch, chars.peek().copied()) {
            Some(display) => {
                visible.push_str(display.text);
                let len = display.text.chars().count();
                push_display_cursor_boundaries(
                    &mut display_to_doc,
                    doc_index,
                    len,
                    display.cursor_policy,
                );
                display_chars += len;
            }
            None => {
                visible.push(ch);
                display_to_doc.push(doc_index + 1);
                display_chars += 1;
            }
        }
    }
    doc_to_display.push(display_chars);

    DisplayTextSlice {
        text: visible,
        map: Some(DisplayTextMap {
            doc_to_display,
            display_to_doc,
        }),
    }
}

pub(super) fn preview_text_slice(
    text: &str,
    slice_range: std::ops::Range<usize>,
    preview: Option<&SearchReplacementPreview>,
) -> PreviewTextSlice {
    let Some(preview) = preview.filter(|preview| !preview.entries.is_empty()) else {
        return PreviewTextSlice {
            text: text.to_owned(),
            map: None,
        };
    };

    let original_chars = text.chars().collect::<Vec<_>>();
    let original_len = original_chars.len();
    let mut projected = String::with_capacity(text.len());
    let mut doc_to_display = vec![0; original_len + 1];
    let mut display_to_doc = vec![0];
    let mut original_cursor = 0usize;
    let mut projected_cursor = 0usize;

    let mut entries = preview
        .entries
        .iter()
        .filter_map(|entry| {
            let start = entry.range.start.max(slice_range.start);
            let end = entry.range.end.min(slice_range.end);
            (start < end).then_some((
                start.saturating_sub(slice_range.start),
                end.saturating_sub(slice_range.start),
                entry.replacement.as_str(),
            ))
        })
        .collect::<Vec<_>>();
    entries.sort_by_key(|(start, _, _)| *start);

    for (start, end, replacement) in entries {
        if start < original_cursor {
            continue;
        }
        copy_original_chars(
            &original_chars,
            original_cursor,
            start,
            &mut projected,
            &mut doc_to_display,
            &mut display_to_doc,
            &mut projected_cursor,
        );

        doc_to_display[start] = projected_cursor;
        projected.push_str(replacement);
        let replacement_len = replacement.chars().count();
        for boundary in 1..=replacement_len {
            projected_cursor += 1;
            if boundary == replacement_len {
                display_to_doc.push(end);
            } else {
                display_to_doc.push(start);
            }
        }
        for display_cursor in doc_to_display
            .iter_mut()
            .take(end.min(original_len) + 1)
            .skip(start + 1)
        {
            *display_cursor = projected_cursor;
        }
        original_cursor = end;
    }

    copy_original_chars(
        &original_chars,
        original_cursor,
        original_len,
        &mut projected,
        &mut doc_to_display,
        &mut display_to_doc,
        &mut projected_cursor,
    );
    doc_to_display[original_len] = projected_cursor;

    PreviewTextSlice {
        text: projected,
        map: Some(DisplayTextMap {
            doc_to_display,
            display_to_doc,
        }),
    }
}

fn copy_original_chars(
    original_chars: &[char],
    start: usize,
    end: usize,
    projected: &mut String,
    doc_to_display: &mut [usize],
    display_to_doc: &mut Vec<usize>,
    projected_cursor: &mut usize,
) {
    for index in start..end {
        doc_to_display[index] = *projected_cursor;
        projected.push(original_chars[index]);
        *projected_cursor += 1;
        display_to_doc.push(index + 1);
    }
}

pub(super) fn compose_display_maps(
    preview_map: Option<&DisplayTextMap>,
    display_map: Option<&DisplayTextMap>,
) -> Option<DisplayTextMap> {
    match (preview_map, display_map) {
        (None, None) => None,
        (Some(map), None) | (None, Some(map)) => Some(map.clone()),
        (Some(preview), Some(display)) => {
            let doc_to_display = preview
                .doc_to_display
                .iter()
                .map(|projected_cursor| display.doc_to_display_cursor(*projected_cursor))
                .collect();
            let display_to_doc = display
                .display_to_doc
                .iter()
                .map(|projected_cursor| preview.display_to_doc_cursor(*projected_cursor))
                .collect();
            Some(DisplayTextMap {
                doc_to_display,
                display_to_doc,
            })
        }
    }
}

fn push_display_cursor_boundaries(
    display_to_doc: &mut Vec<usize>,
    doc_index: usize,
    display_len: usize,
    policy: CursorSubstitutionPolicy,
) {
    for boundary in 1..=display_len {
        let doc_cursor = match policy {
            CursorSubstitutionPolicy::SingleCell => {
                if boundary < display_len && boundary < display_len.div_ceil(2) {
                    doc_index
                } else {
                    doc_index + 1
                }
            }
            CursorSubstitutionPolicy::LineEndingMarker => {
                if boundary < display_len {
                    doc_index
                } else {
                    doc_index + 1
                }
            }
        };
        display_to_doc.push(doc_cursor);
    }
}

fn visible_control_char(ch: char, next: Option<char>) -> Option<VisibleControlSubstitution> {
    let (text, cursor_policy) = if ch == '\t' {
        ("\u{2409}", CursorSubstitutionPolicy::SingleCell)
    } else if ch == '\n' {
        ("\u{240A}\n", CursorSubstitutionPolicy::LineEndingMarker)
    } else if ch == '\r' && next == Some('\n') {
        ("\u{240D}", CursorSubstitutionPolicy::SingleCell)
    } else if ch == '\r' {
        ("\u{240D}\n", CursorSubstitutionPolicy::LineEndingMarker)
    } else if ch == '\u{007F}' {
        ("\u{2421}", CursorSubstitutionPolicy::SingleCell)
    } else {
        let text = visible_unicode_control_glyph(ch)
            .or_else(|| (ch.is_control() && (ch as u32) <= 0x1F).then(|| control_picture(ch)))?;
        (text, CursorSubstitutionPolicy::SingleCell)
    };
    Some(VisibleControlSubstitution {
        text,
        cursor_policy,
    })
}

fn visible_unicode_control_glyph(ch: char) -> Option<&'static str> {
    UNICODE_CONTROL_GLYPHS
        .iter()
        .find_map(|(control, glyph)| (*control == ch).then_some(*glyph))
}

fn control_picture(ch: char) -> &'static str {
    C0_CONTROL_PICTURES
        .get(ch as usize)
        .copied()
        .unwrap_or("\u{2426}")
}

pub(super) fn display_search_highlights(
    highlights: &SearchHighlightState,
    map: Option<&DisplayTextMap>,
) -> SearchHighlightState {
    let Some(map) = map else {
        return highlights.clone();
    };
    SearchHighlightState {
        ranges: highlights
            .ranges
            .iter()
            .filter_map(|range| map.doc_range_to_display(range.clone()))
            .collect(),
        active_range_index: highlights.active_range_index,
    }
}

pub(super) fn display_selection_highlight(
    selection: Option<std::ops::Range<usize>>,
    map: Option<&DisplayTextMap>,
) -> Option<std::ops::Range<usize>> {
    match (selection, map) {
        (Some(range), Some(map)) => map.doc_range_to_display(range),
        (selection, None) => selection,
        (None, Some(_)) => None,
    }
}
