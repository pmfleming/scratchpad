use super::{TextDocumentEditOperation, TextDocumentOperationRecord};
use crate::app::ui::editor_content::native_editor::CursorRange;
use std::ops::Range;

/// After a soft divider (`,` `;` `:`) the entry is sealed only if the next
/// keystroke arrives later than this. Inside this window the entry keeps
/// growing past the soft boundary so a continuous typing burst stays one entry.
pub(crate) const TEXT_HISTORY_SOFT_DIVIDER_PAUSE: std::time::Duration =
    std::time::Duration::from_millis(400);

pub(crate) enum CoalescedEdit {
    Record(TextDocumentOperationRecord),
    Noop,
}

fn is_hard_divider(ch: char) -> bool {
    matches!(ch, '.' | '?' | '!' | '\n' | '\r')
}

fn is_soft_divider(ch: char) -> bool {
    matches!(ch, ',' | ';' | ':' | '-')
}

fn cursor_ranges_share_position(left: &CursorRange, right: &CursorRange) -> bool {
    left.primary.index == right.primary.index && left.secondary.index == right.secondary.index
}

fn coalescable_edit_text(edit: &TextDocumentEditOperation) -> bool {
    // Inserts may contain a hard divider (newline) that joins the prior entry
    // and then seals it; only the delete side is restricted to single line.
    is_single_line(&edit.deleted_text)
}

/// Decide whether the latest entry has been "sealed" by a divider in its
/// edit text. Hard dividers always seal insert and delete bursts; soft dividers
/// seal insert bursts only if the user paused after typing them.
pub(crate) fn entry_sealed_by_divider(
    latest: &TextDocumentOperationRecord,
    elapsed: Option<std::time::Duration>,
) -> bool {
    let Some(edit) = latest.edits.last() else {
        return false;
    };

    if edit.inserted_text.is_empty() && !edit.deleted_text.is_empty() {
        return edit.deleted_text.chars().any(is_hard_divider);
    }

    if let Some(last_char) = edit.inserted_text.chars().next_back() {
        if is_hard_divider(last_char) {
            return true;
        }
        if is_soft_divider(last_char) {
            return elapsed.is_none_or(|d| d >= TEXT_HISTORY_SOFT_DIVIDER_PAUSE);
        }
    }

    false
}

pub(crate) fn coalesced_local_edit_record(
    latest: TextDocumentOperationRecord,
    incoming: &TextDocumentOperationRecord,
) -> Option<CoalescedEdit> {
    let ([latest_edit], [incoming_edit]) = (latest.edits.as_slice(), incoming.edits.as_slice())
    else {
        return None;
    };
    // Compare by index only: `prefer_next_row` is a UI hint that flips at
    // soft-wrap boundaries, and treating it as a cursor jump would split
    // continuous typing into spurious entries.
    if !cursor_ranges_share_position(&latest.next_selection, &incoming.previous_selection)
        || !latest.next_selection.is_empty()
        || !incoming.next_selection.is_empty()
        || !coalescable_edit_text(latest_edit)
        || !coalescable_edit_text(incoming_edit)
    {
        return None;
    }

    if !latest_edit.inserted_text.is_empty() {
        return coalesce_into_inserted_text(latest, incoming);
    }

    if !latest_edit.deleted_text.is_empty() && latest_edit.inserted_text.is_empty() {
        return coalesce_after_delete(latest, incoming);
    }

    None
}

fn coalesce_into_inserted_text(
    mut latest: TextDocumentOperationRecord,
    incoming: &TextDocumentOperationRecord,
) -> Option<CoalescedEdit> {
    let latest_edit = latest.edits.first_mut()?;
    let incoming_edit = incoming.edits.first()?;
    let inserted_len = latest_edit.inserted_text.chars().count();
    let deleted_len = incoming_edit.deleted_text.chars().count();
    let inserted_start = latest_edit.start_char;
    let incoming_end = incoming_edit.start_char.checked_add(deleted_len)?;
    let inserted_end = inserted_start.checked_add(inserted_len)?;
    // This preserves scripted/local-correction batches where the editor keeps
    // cursor continuity while deleting the character immediately before the
    // burst. Natural click/arrow movement still starts a new history entry.
    if incoming_edit.inserted_text.is_empty()
        && deleted_len == 1
        && incoming_end == inserted_start
        && !incoming_edit.deleted_text.chars().any(is_hard_divider)
    {
        latest_edit.start_char = incoming_edit.start_char;
        latest_edit.deleted_text =
            format!("{}{}", incoming_edit.deleted_text, latest_edit.deleted_text);
        latest_edit
            .deleted_spans
            .clone_from(&incoming_edit.deleted_spans);
        latest.next_selection = incoming.next_selection;
        return Some(CoalescedEdit::Record(latest));
    }

    if incoming_edit.start_char < inserted_start || incoming_end > inserted_end {
        return None;
    }

    let relative_start = incoming_edit.start_char - inserted_start;
    let relative_end = relative_start + deleted_len;
    latest_edit.inserted_text = replace_char_range_in_text(
        &latest_edit.inserted_text,
        relative_start..relative_end,
        &incoming_edit.inserted_text,
    );
    latest.next_selection = incoming.next_selection;

    if latest_edit.deleted_text.is_empty() && latest_edit.inserted_text.is_empty() {
        return Some(CoalescedEdit::Noop);
    }

    Some(CoalescedEdit::Record(latest))
}

fn coalesce_after_delete(
    mut latest: TextDocumentOperationRecord,
    incoming: &TextDocumentOperationRecord,
) -> Option<CoalescedEdit> {
    let latest_edit = latest.edits.first_mut()?;
    let incoming_edit = incoming.edits.first()?;
    let latest_start = latest_edit.start_char;
    let incoming_deleted_len = incoming_edit.deleted_text.chars().count();
    if incoming_edit.deleted_text.chars().any(is_hard_divider) {
        return None;
    }
    let merged = match (
        incoming_edit.deleted_text.is_empty(),
        incoming_edit.inserted_text.is_empty(),
    ) {
        (true, false) if incoming_edit.start_char == latest_start => {
            latest_edit
                .inserted_text
                .clone_from(&incoming_edit.inserted_text);
            true
        }
        (false, true) if incoming_edit.start_char == latest_start => {
            latest_edit
                .deleted_text
                .push_str(&incoming_edit.deleted_text);
            latest_edit.deleted_spans.clear();
            true
        }
        (false, true) if incoming_edit.start_char + incoming_deleted_len == latest_start => {
            latest_edit.start_char = incoming_edit.start_char;
            latest_edit.deleted_text =
                format!("{}{}", incoming_edit.deleted_text, latest_edit.deleted_text);
            latest_edit.deleted_spans.clear();
            true
        }
        _ => false,
    };
    if !merged {
        return None;
    }
    latest.next_selection = incoming.next_selection;
    Some(CoalescedEdit::Record(latest))
}

fn replace_char_range_in_text(text: &str, range: Range<usize>, replacement: &str) -> String {
    let start = byte_index_for_char(text, range.start);
    let end = byte_index_for_char(text, range.end);
    let mut result = String::with_capacity(text.len() + replacement.len());
    result.push_str(&text[..start]);
    result.push_str(replacement);
    result.push_str(&text[end..]);
    result
}

fn byte_index_for_char(text: &str, char_index: usize) -> usize {
    text.char_indices()
        .map(|(index, _)| index)
        .nth(char_index)
        .unwrap_or(text.len())
}

fn is_single_line(text: &str) -> bool {
    !text.contains('\n') && !text.contains('\r')
}

#[cfg(test)]
mod tests {
    use super::{
        CoalescedEdit, TEXT_HISTORY_SOFT_DIVIDER_PAUSE, TextDocumentEditOperation,
        TextDocumentOperationRecord, coalesced_local_edit_record, entry_sealed_by_divider,
    };
    use crate::app::ui::editor_content::native_editor::{CharCursor, CursorRange};
    use std::time::Duration;

    fn cursor(index: usize) -> CursorRange {
        CursorRange::one(CharCursor::new(index))
    }

    fn record(
        start_char: usize,
        deleted_text: &str,
        inserted_text: &str,
        previous: usize,
        next: usize,
    ) -> TextDocumentOperationRecord {
        TextDocumentOperationRecord {
            previous_selection: cursor(previous),
            next_selection: cursor(next),
            edits: vec![TextDocumentEditOperation {
                start_char,
                deleted_text: deleted_text.to_owned(),
                inserted_text: inserted_text.to_owned(),
                deleted_spans: Vec::new(),
            }],
        }
    }

    #[test]
    fn soft_dividers_seal_only_after_pause() {
        for divider in [",", ";", ":", "-"] {
            let latest = record(0, "", &format!("word{divider}"), 0, 5);

            assert!(!entry_sealed_by_divider(
                &latest,
                Some(
                    TEXT_HISTORY_SOFT_DIVIDER_PAUSE
                        .checked_sub(Duration::from_millis(1))
                        .expect("soft divider pause exceeds 1ms")
                )
            ));
            assert!(entry_sealed_by_divider(
                &latest,
                Some(TEXT_HISTORY_SOFT_DIVIDER_PAUSE)
            ));
            assert!(entry_sealed_by_divider(&latest, None));
        }
    }

    #[test]
    fn hard_dividers_seal_immediately_for_insert_and_delete() {
        for divider in [".", "?", "!", "\n", "\r"] {
            let insert = record(0, "", &format!("word{divider}"), 0, 5);
            let delete = record(4, divider, "", 5, 4);

            assert!(entry_sealed_by_divider(
                &insert,
                Some(Duration::from_millis(0))
            ));
            assert!(entry_sealed_by_divider(
                &delete,
                Some(Duration::from_millis(0))
            ));
        }
    }

    #[test]
    fn continuous_insert_records_merge_in_place() {
        let latest = record(0, "", "ab", 0, 2);
        let incoming = record(2, "", "c", 2, 3);

        let merged = coalesced_local_edit_record(latest, &incoming);

        match merged {
            Some(CoalescedEdit::Record(record)) => {
                assert_eq!(record.edits[0].inserted_text, "abc");
                assert_eq!(record.next_selection, cursor(3));
            }
            _ => panic!("expected merged insert record"),
        }
    }

    #[test]
    fn deleting_the_only_inserted_char_removes_transient_entry() {
        let latest = record(0, "", "x", 0, 1);
        let incoming = record(0, "x", "", 1, 0);

        assert!(matches!(
            coalesced_local_edit_record(latest, &incoming),
            Some(CoalescedEdit::Noop)
        ));
    }

    #[test]
    fn delete_records_do_not_merge_across_newline() {
        let latest = record(2, "b", "", 3, 2);
        let incoming = record(1, "\n", "", 2, 1);

        assert!(coalesced_local_edit_record(latest, &incoming).is_none());
    }

    #[test]
    fn cursor_jump_prevents_coalescing() {
        let latest = record(0, "", "a", 0, 1);
        let incoming = record(0, "", "b", 0, 1);

        assert!(coalesced_local_edit_record(latest, &incoming).is_none());
    }
}
