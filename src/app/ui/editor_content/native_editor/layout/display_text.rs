use crate::app::domain::{SearchHighlightState, SearchReplacementPreview};
use std::borrow::Cow;

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
        clamped_lookup(&self.doc_to_display, cursor)
    }

    pub(in crate::app::ui::editor_content::native_editor) fn display_to_doc_cursor(
        &self,
        cursor: usize,
    ) -> usize {
        clamped_lookup(&self.display_to_doc, cursor)
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

/// Cursors past the end of a map clamp to its last entry.
fn clamped_lookup(map: &[usize], cursor: usize) -> usize {
    map.get(cursor).or_else(|| map.last()).copied().unwrap_or(0)
}

pub(super) struct DisplayTextSlice<'a> {
    pub(super) text: Cow<'a, str>,
    pub(super) map: Option<DisplayTextMap>,
}

impl<'a> DisplayTextSlice<'a> {
    fn unmapped(text: &'a str) -> Self {
        Self {
            text: Cow::Borrowed(text),
            map: None,
        }
    }
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

pub(super) fn display_text_slice(text: &str, show_control_chars: bool) -> DisplayTextSlice<'_> {
    if !show_control_chars {
        return DisplayTextSlice::unmapped(text);
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
        if let Some(display) = visible_control_char(ch, chars.peek().copied()) {
            visible.push_str(display.text);
            let len = display.text.chars().count();
            push_display_cursor_boundaries(
                &mut display_to_doc,
                doc_index,
                len,
                display.cursor_policy,
            );
            display_chars += len;
        } else {
            visible.push(ch);
            display_to_doc.push(doc_index + 1);
            display_chars += 1;
        }
    }
    doc_to_display.push(display_chars);

    DisplayTextSlice {
        text: Cow::Owned(visible),
        map: Some(DisplayTextMap {
            doc_to_display,
            display_to_doc,
        }),
    }
}

pub(super) fn preview_text_slice<'a>(
    text: &'a str,
    slice_range: std::ops::Range<usize>,
    preview: Option<&SearchReplacementPreview>,
) -> DisplayTextSlice<'a> {
    let visible_entries = preview.map_or_else(Vec::new, |preview| {
        visible_preview_entries(preview, &slice_range)
    });
    if visible_entries.is_empty() {
        return DisplayTextSlice::unmapped(text);
    }

    let original_chars = text.chars().collect::<Vec<_>>();
    let original_len = original_chars.len();
    let mut projection = TextProjection::new(text.len(), original_len);
    let mut original_cursor = 0usize;
    for (start, end, replacement) in slice_local_replacements(visible_entries, &slice_range) {
        // Overlapping replacements keep the first one.
        if start < original_cursor {
            continue;
        }
        projection.copy_original(&original_chars, original_cursor..start);
        projection.replace(start, end, replacement);
        original_cursor = end;
    }
    projection.copy_original(&original_chars, original_cursor..original_len);
    projection.finish(original_len)
}

fn visible_preview_entries<'a>(
    preview: &'a SearchReplacementPreview,
    slice_range: &std::ops::Range<usize>,
) -> Vec<&'a crate::app::domain::SearchReplacementPreviewEntry> {
    let start_index = preview
        .entries()
        .partition_point(|entry| entry.range.end <= slice_range.start);
    let end_index = preview
        .entries()
        .partition_point(|entry| entry.range.start < slice_range.end);
    preview.entries()[start_index..end_index].iter().collect()
}

/// Non-empty replacements clipped to the slice, in slice-local char offsets,
/// sorted by start.
fn slice_local_replacements<'a>(
    entries: Vec<&'a crate::app::domain::SearchReplacementPreviewEntry>,
    slice_range: &std::ops::Range<usize>,
) -> Vec<(usize, usize, &'a str)> {
    let mut replacements = entries
        .into_iter()
        .filter_map(|entry| {
            let start = entry.range.start.max(slice_range.start);
            let end = entry.range.end.min(slice_range.end);
            (start < end).then_some((
                start - slice_range.start,
                end - slice_range.start,
                entry.replacement.as_str(),
            ))
        })
        .collect::<Vec<_>>();
    replacements.sort_by_key(|(start, _, _)| *start);
    replacements
}

/// Projected text plus the cursor maps between it and the original text.
/// The projected cursor is always `display_to_doc.len() - 1`.
struct TextProjection {
    text: String,
    doc_to_display: Vec<usize>,
    display_to_doc: Vec<usize>,
}

impl TextProjection {
    fn new(byte_capacity: usize, doc_len: usize) -> Self {
        Self {
            text: String::with_capacity(byte_capacity),
            doc_to_display: vec![0; doc_len + 1],
            display_to_doc: vec![0],
        }
    }

    fn display_cursor(&self) -> usize {
        self.display_to_doc.len() - 1
    }

    fn copy_original(&mut self, original_chars: &[char], range: std::ops::Range<usize>) {
        for index in range {
            self.doc_to_display[index] = self.display_cursor();
            self.text.push(original_chars[index]);
            self.display_to_doc.push(index + 1);
        }
    }

    /// Project original chars `start..end` as `replacement`. Display cursors
    /// inside the replacement map back to `start`, its end maps to `end`, and
    /// original cursors inside the replaced span map to the replacement's end.
    fn replace(&mut self, start: usize, end: usize, replacement: &str) {
        self.doc_to_display[start] = self.display_cursor();
        self.text.push_str(replacement);
        let replacement_len = replacement.chars().count();
        self.display_to_doc
            .extend((1..=replacement_len).map(|boundary| {
                if boundary == replacement_len {
                    end
                } else {
                    start
                }
            }));
        let replacement_end = self.display_cursor();
        let last_doc_cursor = end.min(self.doc_to_display.len() - 1);
        for display_cursor in self
            .doc_to_display
            .iter_mut()
            .take(last_doc_cursor + 1)
            .skip(start + 1)
        {
            *display_cursor = replacement_end;
        }
    }

    fn finish(mut self, doc_len: usize) -> DisplayTextSlice<'static> {
        self.doc_to_display[doc_len] = self.display_cursor();
        DisplayTextSlice {
            text: Cow::Owned(self.text),
            map: Some(DisplayTextMap {
                doc_to_display: self.doc_to_display,
                display_to_doc: self.display_to_doc,
            }),
        }
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
    // Display boundaries before `split` stay on the substituted char; the rest
    // (including the substitution's end) move past it.
    let split = match policy {
        CursorSubstitutionPolicy::SingleCell => display_len.div_ceil(2),
        CursorSubstitutionPolicy::LineEndingMarker => display_len,
    };
    display_to_doc.extend((1..=display_len).map(|boundary| {
        if boundary < split {
            doc_index
        } else {
            doc_index + 1
        }
    }));
}

fn visible_control_char(ch: char, next: Option<char>) -> Option<VisibleControlSubstitution> {
    let (text, cursor_policy) = match (ch, next) {
        ('\t', _) => ("\u{2409}", CursorSubstitutionPolicy::SingleCell),
        ('\n', _) => ("\u{240A}\n", CursorSubstitutionPolicy::LineEndingMarker),
        ('\r', Some('\n')) => ("\u{240D}", CursorSubstitutionPolicy::SingleCell),
        ('\r', _) => ("\u{240D}\n", CursorSubstitutionPolicy::LineEndingMarker),
        ('\u{007F}', _) => ("\u{2421}", CursorSubstitutionPolicy::SingleCell),
        _ => {
            let text = visible_unicode_control_glyph(ch).or_else(|| {
                (ch.is_control() && (ch as u32) <= 0x1F).then(|| control_picture(ch))
            })?;
            (text, CursorSubstitutionPolicy::SingleCell)
        }
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
