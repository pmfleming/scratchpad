use crate::app::app_state::{AppSurface, ScratchpadApp};
use crate::app::domain::buffer::TextDocumentOperationRecord;
use crate::app::domain::{BufferId, TabManager};
use std::ops::Range;
use std::time::{Duration, Instant};

const MAX_TRANSACTION_LOG_ENTRIES: usize = 200;
const LAYOUT_TRANSACTION_GROUP_IDLE: Duration = Duration::from_millis(250);
const TEXT_EDIT_GROUP_IDLE: Duration = Duration::from_millis(1200);
const TEXT_EDIT_MAX_PREVIEW_CHARS: usize = 160;
const TEXT_EDIT_MAX_PREVIEW_LINES: usize = 3;

#[derive(Clone)]
pub(crate) struct TransactionSnapshot {
    tab_manager: TabManager,
    active_surface: AppSurface,
    settings_tab_index: usize,
    pending_settings_toml_refresh: Option<BufferId>,
}

#[derive(Clone)]
pub(crate) struct TransactionLogEntry {
    pub(crate) id: u64,
    pub(crate) action_label: String,
    pub(crate) affected_items: Vec<String>,
    pub(crate) details: Option<String>,
    pub(crate) created_at: Instant,
    pub(crate) snapshot_before: TransactionSnapshot,
}

impl TransactionLogEntry {
    pub(crate) fn title(&self) -> String {
        let compact_label = self.action_label.replace('\n', " ");
        if self.affected_items.is_empty() {
            compact_label
        } else {
            format!("{}: {}", compact_label, self.affected_items.join(", "))
        }
    }
}

#[derive(Default)]
pub(crate) struct TransactionLog {
    next_id: u64,
    entries: Vec<TransactionLogEntry>,
}

pub(crate) struct PendingTextTransaction {
    entry_id: u64,
    buffer_id: BufferId,
    preview_state: Option<TextEditPreviewState>,
    last_edit_at: Instant,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TextEditPreviewState {
    text: String,
    span: Range<usize>,
    mergeable: bool,
}

pub(crate) struct PendingLayoutTransaction {
    entry_id: u64,
    last_edit_at: Instant,
}

impl TransactionLog {
    pub(crate) fn entries(&self) -> &[TransactionLogEntry] {
        &self.entries
    }

    pub(crate) fn push(
        &mut self,
        action_label: impl Into<String>,
        affected_items: Vec<String>,
        details: Option<String>,
        snapshot_before: TransactionSnapshot,
    ) {
        let entry = TransactionLogEntry {
            id: self.next_id,
            action_label: action_label.into(),
            affected_items,
            details,
            created_at: Instant::now(),
            snapshot_before,
        };
        self.next_id = self.next_id.saturating_add(1);
        self.entries.push(entry);
        if self.entries.len() > MAX_TRANSACTION_LOG_ENTRIES {
            let overflow = self.entries.len() - MAX_TRANSACTION_LOG_ENTRIES;
            self.entries.drain(0..overflow);
        }
    }

    pub(crate) fn undo_to_entry(&mut self, entry_id: u64) -> Option<TransactionLogEntry> {
        let index = self.entries.iter().position(|entry| entry.id == entry_id)?;
        let entry = self.entries[index].clone();
        self.entries.truncate(index);
        Some(entry)
    }

    pub(crate) fn last_entry_mut(&mut self) -> Option<&mut TransactionLogEntry> {
        self.entries.last_mut()
    }

    pub(crate) fn next_entry_id(&self) -> u64 {
        self.next_id
    }

    pub(crate) fn clear(&mut self) {
        self.entries.clear();
    }
}

impl ScratchpadApp {
    pub(crate) fn capture_transaction_snapshot(&self) -> TransactionSnapshot {
        TransactionSnapshot {
            tab_manager: self.tab_manager.clone(),
            active_surface: self.active_surface,
            settings_tab_index: self.settings_tab_index,
            pending_settings_toml_refresh: self.pending_settings_toml_refresh,
        }
    }

    pub(crate) fn has_coalescable_text_transaction(&self, buffer_id: BufferId) -> bool {
        self.pending_text_transaction
            .as_ref()
            .is_some_and(|pending| {
                pending.buffer_id == buffer_id
                    && Instant::now().duration_since(pending.last_edit_at) <= TEXT_EDIT_GROUP_IDLE
            })
    }

    pub(crate) fn record_transaction(
        &mut self,
        action_label: impl Into<String>,
        affected_items: Vec<String>,
        details: Option<String>,
        snapshot_before: TransactionSnapshot,
    ) {
        self.transaction_log
            .push(action_label, affected_items, details, snapshot_before);
        self.pending_layout_transaction = None;
        self.pending_text_transaction = None;
    }

    pub(crate) fn record_coalesced_layout_transaction(
        &mut self,
        action_label: &'static str,
        affected_items: Vec<String>,
        snapshot_before: Option<TransactionSnapshot>,
    ) {
        let now = Instant::now();
        if snapshot_before.is_none() {
            if let Some(pending) = self.pending_layout_transaction.as_mut() {
                pending.last_edit_at = now;
            }
            self.pending_text_transaction = None;
            return;
        }

        let entry_id = self.transaction_log.next_entry_id();
        self.transaction_log.push(
            action_label,
            affected_items,
            None,
            snapshot_before.expect("snapshot checked"),
        );
        self.pending_layout_transaction = Some(PendingLayoutTransaction {
            entry_id,
            last_edit_at: now,
        });
        self.pending_text_transaction = None;
    }

    pub(crate) fn capture_coalesced_layout_snapshot(
        &self,
        action_label: &'static str,
        affected_items: &[String],
    ) -> Option<TransactionSnapshot> {
        if self.can_extend_layout_transaction(action_label, affected_items) {
            None
        } else {
            Some(self.capture_transaction_snapshot())
        }
    }

    fn can_extend_layout_transaction(
        &self,
        action_label: &'static str,
        affected_items: &[String],
    ) -> bool {
        let now = Instant::now();
        self.pending_layout_transaction
            .as_ref()
            .is_some_and(|pending| {
                now.duration_since(pending.last_edit_at) <= LAYOUT_TRANSACTION_GROUP_IDLE
                    && self.transaction_log.entries().last().is_some_and(|entry| {
                        entry.id == pending.entry_id
                            && entry.action_label == action_label
                            && entry.affected_items == affected_items
                    })
            })
    }

    pub(crate) fn transaction_log_entries(&self) -> &[TransactionLogEntry] {
        self.transaction_log.entries()
    }

    pub fn open_transaction_log(&mut self) {
        self.transaction_log_open = true;
    }

    pub fn close_transaction_log(&mut self) {
        self.transaction_log_open = false;
    }

    pub fn transaction_log_open(&self) -> bool {
        self.transaction_log_open
    }

    pub fn undo_transaction_entry(&mut self, entry_id: u64) -> bool {
        let Some(entry) = self.transaction_log.undo_to_entry(entry_id) else {
            return false;
        };

        let title = entry.title();
        self.restore_transaction_snapshot(entry.snapshot_before);
        self.set_info_status(format!("Undid history entry: {}", title));
        let _ = self.persist_session_now();
        true
    }

    pub fn latest_transaction_entry_id(&self) -> Option<u64> {
        self.transaction_log.entries().last().map(|entry| entry.id)
    }

    pub fn transaction_log_len(&self) -> usize {
        self.transaction_log.entries().len()
    }

    pub fn clear_transaction_log(&mut self) {
        self.transaction_log.clear();
        self.pending_layout_transaction = None;
        self.pending_text_transaction = None;
    }

    pub(crate) fn active_buffer_transaction_label(&self) -> Option<String> {
        self.active_tab().map(|tab| {
            tab.active_buffer()
                .path
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| tab.active_buffer().name.clone())
        })
    }

    pub(crate) fn record_text_edit_transaction(
        &mut self,
        buffer_id: BufferId,
        buffer_label: String,
        snapshot_before: TransactionSnapshot,
        latest_edit: Option<TextDocumentOperationRecord>,
    ) {
        self.pending_layout_transaction = None;
        let now = Instant::now();
        let next_preview: Option<TextEditPreviewState> = latest_edit
            .as_ref()
            .and_then(TextEditPreviewState::from_operation);

        let can_extend_existing = self
            .pending_text_transaction
            .as_ref()
            .is_some_and(|pending| {
                pending.buffer_id == buffer_id
                    && now.duration_since(pending.last_edit_at) <= TEXT_EDIT_GROUP_IDLE
            });

        if can_extend_existing {
            let pending = self
                .pending_text_transaction
                .as_mut()
                .expect("pending text transaction");
            let previous_preview = pending.preview_state.take();
            let merged_preview: Option<TextEditPreviewState> =
                match (previous_preview, next_preview) {
                    (Some(existing), Some(next)) => existing.merge(next),
                    _ => None,
                };
            pending.preview_state = merged_preview;
            pending.last_edit_at = now;
            if let Some(entry) = self
                .transaction_log
                .last_entry_mut()
                .filter(|entry| entry.id == pending.entry_id)
            {
                entry.action_label = pending
                    .preview_state
                    .as_ref()
                    .and_then(TextEditPreviewState::label)
                    .unwrap_or_else(|| "Edit".to_owned());
            }
            return;
        }

        let next_label = next_preview
            .as_ref()
            .and_then(TextEditPreviewState::label)
            .unwrap_or_else(|| "Edit".to_owned());
        let entry_id = self.transaction_log.next_entry_id();
        self.transaction_log
            .push(next_label, vec![buffer_label], None, snapshot_before);
        self.pending_text_transaction = Some(PendingTextTransaction {
            entry_id,
            buffer_id,
            preview_state: next_preview,
            last_edit_at: now,
        });
    }

    fn restore_transaction_snapshot(&mut self, snapshot: TransactionSnapshot) {
        self.tab_manager = snapshot.tab_manager;
        self.active_surface = snapshot.active_surface;
        self.settings_tab_index = snapshot.settings_tab_index;
        self.pending_settings_toml_refresh = snapshot.pending_settings_toml_refresh;
        self.pending_editor_focus = self.active_tab().map(|tab| tab.active_view_id);
        self.tab_manager.pending_scroll_to_active = true;
        self.pending_layout_transaction = None;
        self.pending_text_transaction = None;
        self.mark_session_dirty();
    }
}

fn inserted_text_preview(inserted: &str) -> Option<String> {
    if inserted.trim().is_empty() {
        return None;
    }

    let lines = inserted
        .lines()
        .take(TEXT_EDIT_MAX_PREVIEW_LINES)
        .collect::<Vec<_>>();
    let mut preview = lines.join("\n");
    if preview.chars().count() > TEXT_EDIT_MAX_PREVIEW_CHARS {
        preview = preview
            .chars()
            .take(TEXT_EDIT_MAX_PREVIEW_CHARS)
            .collect::<String>();
        preview.push_str("...");
    } else if inserted.lines().count() > TEXT_EDIT_MAX_PREVIEW_LINES {
        preview.push_str("...");
    }

    Some(preview)
}

fn preview_text_is_reasonable(text: &str) -> bool {
    let line_count = text.lines().count().max(1);
    line_count <= TEXT_EDIT_MAX_PREVIEW_LINES && text.chars().count() <= TEXT_EDIT_MAX_PREVIEW_CHARS
}

impl TextEditPreviewState {
    fn from_operation(operation: &TextDocumentOperationRecord) -> Option<Self> {
        if operation.edits.len() != 1 {
            return None;
        }

        let edit = operation.edits.first()?;
        if edit.inserted_text.is_empty() || !preview_text_is_reasonable(&edit.inserted_text) {
            return None;
        }

        Some(Self {
            text: edit.inserted_text.clone(),
            span: edit.start_char..edit.start_char + edit.inserted_text.chars().count(),
            mergeable: edit.deleted_text.is_empty(),
        })
    }

    fn label(&self) -> Option<String> {
        inserted_text_preview(&self.text)
    }

    fn merge(mut self, next: Self) -> Option<Self> {
        if !self.mergeable || !next.mergeable {
            return None;
        }

        if self.span.end == next.span.start {
            self.text.push_str(&next.text);
            self.span.end = next.span.end;
        } else if next.span.end == self.span.start {
            self.text = format!("{}{}", next.text, self.text);
            self.span.start = next.span.start;
        } else {
            return None;
        }

        preview_text_is_reasonable(&self.text).then_some(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn transaction_entry_title_includes_affected_items() {
        let entry = TransactionLogEntry {
            id: 1,
            action_label: "hello".to_owned(),
            affected_items: vec!["notes.txt".to_owned()],
            details: None,
            created_at: Instant::now(),
            snapshot_before: ScratchpadApp::with_session_store(
                crate::app::services::session_store::SessionStore::new(
                    tempfile::tempdir().expect("create session dir").keep(),
                ),
            )
            .capture_transaction_snapshot(),
        };

        assert_eq!(entry.title(), "hello: notes.txt");
    }

    #[test]
    fn inserted_text_preview_shows_added_text() {
        assert_eq!(
            inserted_text_preview("hello world"),
            Some("hello world".to_owned())
        );
    }

    #[test]
    fn edit_preview_state_merges_adjacent_insertions() {
        let first = TextEditPreviewState {
            text: "hello".to_owned(),
            span: 0..5,
            mergeable: true,
        };
        let second = TextEditPreviewState {
            text: " world".to_owned(),
            span: 5..11,
            mergeable: true,
        };

        let merged = first
            .merge(second)
            .expect("adjacent insertions should merge");
        assert_eq!(merged.label(), Some("hello world".to_owned()));
    }
}
