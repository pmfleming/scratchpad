use crate::app::domain::BufferId;
use crate::app::domain::buffer::{TextDocumentEditOperation, TextDocumentOperationRecord};
use std::ops::Range;
use std::time::{Duration, Instant};

const MAX_TEXT_HISTORY_ENTRIES: usize = 1000;
const TEXT_HISTORY_COALESCE_WINDOW: Duration = Duration::from_millis(1200);
const TEXT_HISTORY_PREVIEW_MAX_CHARS: usize = 80;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TextHistorySource {
    Editor,
    SearchReplace,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextHistoryEntry {
    pub(crate) id: u64,
    pub(crate) buffer_id: BufferId,
    pub(crate) file_identity: String,
    pub(crate) label: String,
    pub(crate) source: TextHistorySource,
    pub(crate) summary: String,
    pub(crate) created_at: Instant,
    pub(crate) updated_at: Instant,
    pub(crate) undone_at: Option<Instant>,
    pub(crate) operation: TextDocumentOperationRecord,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextHistoryAction {
    pub(crate) entry_id: u64,
    pub(crate) buffer_id: BufferId,
    pub(crate) operation: TextDocumentOperationRecord,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TextHistoryPlanError {
    MissingEntry,
    AlreadyUndone,
    NotUndone,
    RebaseConflict,
}

#[derive(Default)]
pub(crate) struct TextHistoryLedger {
    next_id: u64,
    entries: Vec<TextHistoryEntry>,
}

impl TextHistoryLedger {
    pub(crate) fn record_components(
        &mut self,
        buffer_id: BufferId,
        file_identity: String,
        label: String,
        source: TextHistorySource,
        operation: TextDocumentOperationRecord,
    ) {
        let now = Instant::now();
        if self.try_coalesce_latest(buffer_id, source, &operation, now) {
            return;
        }

        let summary = operation_summary(source, &operation);
        self.entries.push(TextHistoryEntry {
            id: self.next_id,
            buffer_id,
            file_identity,
            label,
            source,
            summary,
            created_at: now,
            updated_at: now,
            undone_at: None,
            operation,
        });
        self.next_id = self.next_id.saturating_add(1);
        self.enforce_retention_limit();
    }

    fn try_coalesce_latest(
        &mut self,
        buffer_id: BufferId,
        source: TextHistorySource,
        operation: &TextDocumentOperationRecord,
        now: Instant,
    ) -> bool {
        let Some(latest) = self.entries.last_mut() else {
            return false;
        };
        if latest.buffer_id != buffer_id
            || latest.source != source
            || source != TextHistorySource::Editor
            || now.duration_since(latest.updated_at) > TEXT_HISTORY_COALESCE_WINDOW
        {
            return false;
        }

        let Some((latest_edit, incoming_edit)) = coalescable_adjacent_insert(
            latest.operation.edits.as_mut_slice(),
            operation.edits.as_slice(),
        ) else {
            return false;
        };
        latest_edit
            .inserted_text
            .push_str(&incoming_edit.inserted_text);
        latest.operation.next_selection = operation.next_selection;
        latest.summary = operation_summary(latest.source, &latest.operation);
        latest.updated_at = now;
        true
    }

    fn enforce_retention_limit(&mut self) {
        if self.entries.len() > MAX_TEXT_HISTORY_ENTRIES {
            let overflow = self.entries.len() - MAX_TEXT_HISTORY_ENTRIES;
            self.entries.drain(0..overflow);
        }
    }

    pub(crate) fn prune_buffer(&mut self, buffer_id: BufferId) {
        self.entries.retain(|entry| entry.buffer_id != buffer_id);
    }

    pub(crate) fn prune_buffers(&mut self, buffer_ids: impl IntoIterator<Item = BufferId>) {
        for buffer_id in buffer_ids {
            self.prune_buffer(buffer_id);
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.entries().count()
    }

    pub(crate) fn len_for_buffer(&self, buffer_id: BufferId) -> usize {
        self.entries
            .iter()
            .filter(|entry| entry.buffer_id == buffer_id && !entry.is_undone())
            .count()
    }

    pub(crate) fn entries(&self) -> impl Iterator<Item = &TextHistoryEntry> {
        self.entries.iter().filter(|entry| !entry.is_undone())
    }

    pub(crate) fn all_entries(&self) -> impl Iterator<Item = &TextHistoryEntry> {
        self.entries.iter()
    }

    pub(crate) fn entries_for_buffer(
        &self,
        buffer_id: BufferId,
    ) -> impl Iterator<Item = &TextHistoryEntry> {
        self.entries()
            .filter(move |entry| entry.buffer_id == buffer_id)
    }

    pub(crate) fn len_for_source(&self, source: TextHistorySource) -> usize {
        self.entries()
            .filter(move |entry| entry.source == source)
            .count()
    }

    pub(crate) fn redo_len(&self) -> usize {
        self.entries
            .iter()
            .filter(|entry| entry.is_undone())
            .count()
    }

    pub(crate) fn latest_entry_id_for_buffer(&self, buffer_id: BufferId) -> Option<u64> {
        self.entries_for_buffer(buffer_id)
            .last()
            .map(|entry| entry.id)
    }

    pub(crate) fn prepare_selective_undo(
        &self,
        entry_id: u64,
    ) -> Result<TextHistoryAction, TextHistoryPlanError> {
        let (index, entry) = self.entry_with_index(entry_id)?;
        if entry.is_undone() {
            return Err(TextHistoryPlanError::AlreadyUndone);
        }
        Ok(TextHistoryAction {
            entry_id,
            buffer_id: entry.buffer_id,
            operation: self.rebased_operation(index, SelectiveReplayDirection::Undo)?,
        })
    }

    pub(crate) fn prepare_selective_redo(
        &self,
        entry_id: u64,
    ) -> Result<TextHistoryAction, TextHistoryPlanError> {
        let (index, entry) = self.entry_with_index(entry_id)?;
        if !entry.is_undone() {
            return Err(TextHistoryPlanError::NotUndone);
        }
        Ok(TextHistoryAction {
            entry_id,
            buffer_id: entry.buffer_id,
            operation: self.rebased_operation(index, SelectiveReplayDirection::Redo)?,
        })
    }

    pub(crate) fn mark_undone(&mut self, entry_id: u64) {
        if let Some(entry) = self.entries.iter_mut().find(|entry| entry.id == entry_id) {
            entry.undone_at = Some(Instant::now());
        }
    }

    pub(crate) fn mark_redone(&mut self, entry_id: u64) {
        if let Some(entry) = self.entries.iter_mut().find(|entry| entry.id == entry_id) {
            entry.undone_at = None;
            entry.updated_at = Instant::now();
        }
    }

    pub(crate) fn latest_summary(&self) -> Option<&str> {
        self.entries().last().map(|entry| entry.summary.as_str())
    }

    pub(crate) fn latest_edit_count(&self) -> Option<usize> {
        self.entries()
            .last()
            .map(|entry| entry.operation.edits.len())
    }

    pub(crate) fn latest_inserted_text(&self) -> Option<&str> {
        self.entries()
            .last()
            .and_then(|entry| entry.operation.edits.first())
            .map(|edit| edit.inserted_text.as_str())
    }

    fn entry_with_index(
        &self,
        entry_id: u64,
    ) -> Result<(usize, &TextHistoryEntry), TextHistoryPlanError> {
        self.entries
            .iter()
            .enumerate()
            .find(|(_, entry)| entry.id == entry_id)
            .ok_or(TextHistoryPlanError::MissingEntry)
    }

    fn rebased_operation(
        &self,
        index: usize,
        direction: SelectiveReplayDirection,
    ) -> Result<TextDocumentOperationRecord, TextHistoryPlanError> {
        let entry = &self.entries[index];
        let mut operation = entry.operation.clone();
        for edit in &mut operation.edits {
            edit.start_char =
                self.rebased_range_start(index, entry.buffer_id, direction.affected_range(edit))?;
        }
        Ok(operation)
    }

    fn rebased_range_start(
        &self,
        index: usize,
        buffer_id: BufferId,
        mut range: Range<usize>,
    ) -> Result<usize, TextHistoryPlanError> {
        for later_entry in self.entries[index + 1..]
            .iter()
            .filter(|entry| entry.buffer_id == buffer_id && !entry.is_undone())
        {
            for later_edit in edits_sorted_by_start(&later_entry.operation.edits) {
                range = transform_range(range, later_edit)?;
            }
        }
        Ok(range.start)
    }
}

impl TextHistoryEntry {
    pub(crate) fn is_undone(&self) -> bool {
        self.undone_at.is_some()
    }
}

#[derive(Clone, Copy)]
enum SelectiveReplayDirection {
    Undo,
    Redo,
}

impl SelectiveReplayDirection {
    fn affected_range(self, edit: &TextDocumentEditOperation) -> Range<usize> {
        let len = match self {
            Self::Undo => edit.inserted_text.chars().count(),
            Self::Redo => edit.deleted_text.chars().count(),
        };
        edit.start_char..edit.start_char + len
    }
}

fn edits_sorted_by_start(edits: &[TextDocumentEditOperation]) -> Vec<&TextDocumentEditOperation> {
    let mut edits = edits.iter().collect::<Vec<_>>();
    edits.sort_by_key(|edit| edit.start_char);
    edits
}

fn transform_range(
    range: Range<usize>,
    edit: &TextDocumentEditOperation,
) -> Result<Range<usize>, TextHistoryPlanError> {
    let edit_start = edit.start_char;
    let old_len = edit.deleted_text.chars().count();
    let new_len = edit.inserted_text.chars().count();
    let edit_end = edit_start + old_len;
    if range.start == range.end {
        return transform_empty_range(range.start, edit_start, edit_end, old_len, new_len)
            .map(|position| position..position);
    }
    if range.end <= edit_start {
        return Ok(range);
    }
    if range.start >= edit_end {
        return Ok(shift_range(range, new_len as isize - old_len as isize));
    }
    Err(TextHistoryPlanError::RebaseConflict)
}

fn transform_empty_range(
    position: usize,
    edit_start: usize,
    edit_end: usize,
    old_len: usize,
    new_len: usize,
) -> Result<usize, TextHistoryPlanError> {
    if position <= edit_start {
        return Ok(position);
    }
    if position >= edit_end {
        return Ok(shift_position(
            position,
            new_len as isize - old_len as isize,
        ));
    }
    Err(TextHistoryPlanError::RebaseConflict)
}

fn shift_range(range: Range<usize>, delta: isize) -> Range<usize> {
    shift_position(range.start, delta)..shift_position(range.end, delta)
}

fn shift_position(position: usize, delta: isize) -> usize {
    if delta.is_negative() {
        position.saturating_sub(delta.unsigned_abs())
    } else {
        position.saturating_add(delta as usize)
    }
}

fn coalescable_adjacent_insert<'a>(
    latest_edits: &'a mut [TextDocumentEditOperation],
    incoming_edits: &'a [TextDocumentEditOperation],
) -> Option<(
    &'a mut TextDocumentEditOperation,
    &'a TextDocumentEditOperation,
)> {
    let latest_edit = latest_edits.last_mut()?;
    let incoming_edit = incoming_edits.first()?;
    if incoming_edits.len() != 1
        || !latest_edit.deleted_text.is_empty()
        || !incoming_edit.deleted_text.is_empty()
        || latest_edit.inserted_text.is_empty()
        || incoming_edit.inserted_text.is_empty()
    {
        return None;
    }
    let latest_end = latest_edit.start_char + latest_edit.inserted_text.chars().count();
    (latest_end == incoming_edit.start_char).then_some((latest_edit, incoming_edit))
}

fn operation_summary(source: TextHistorySource, operation: &TextDocumentOperationRecord) -> String {
    match source {
        TextHistorySource::SearchReplace => search_replace_summary(operation),
        TextHistorySource::Editor => editor_summary(operation),
    }
}

fn search_replace_summary(operation: &TextDocumentOperationRecord) -> String {
    let edit_count = operation.edits.len();
    if edit_count == 1 {
        "Replace match".to_owned()
    } else {
        format!("Replace {edit_count} matches")
    }
}

fn editor_summary(operation: &TextDocumentOperationRecord) -> String {
    if operation.edits.len() != 1 {
        return format!("Edit {} ranges", operation.edits.len());
    }
    let edit = &operation.edits[0];
    match (edit.deleted_text.is_empty(), edit.inserted_text.is_empty()) {
        (true, false) => format!("Insert \"{}\"", preview_text(&edit.inserted_text)),
        (false, true) => format!("Delete \"{}\"", preview_text(&edit.deleted_text)),
        (false, false) => format!("Replace with \"{}\"", preview_text(&edit.inserted_text)),
        (true, true) => "Edit".to_owned(),
    }
}

fn preview_text(text: &str) -> String {
    let flattened = text.replace(['\r', '\n'], " ");
    let mut preview = flattened
        .chars()
        .take(TEXT_HISTORY_PREVIEW_MAX_CHARS)
        .collect::<String>();
    if flattened.chars().count() > TEXT_HISTORY_PREVIEW_MAX_CHARS {
        preview.push_str("...");
    }
    preview
}
