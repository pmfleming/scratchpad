use crate::app::app_state::{AppSurface, ScratchpadApp};
use crate::app::domain::{BufferId, TabManager};
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
    before_text: String,
    last_text: String,
    last_edit_at: Instant,
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
        current_text: String,
    ) {
        self.pending_layout_transaction = None;
        let now = Instant::now();

        let can_extend_existing = self
            .pending_text_transaction
            .as_ref()
            .is_some_and(|pending| {
                pending.buffer_id == buffer_id
                    && now.duration_since(pending.last_edit_at) <= TEXT_EDIT_GROUP_IDLE
                    && text_preview_is_reasonable(&pending.before_text, &current_text)
            });

        if can_extend_existing {
            let pending = self
                .pending_text_transaction
                .as_mut()
                .expect("pending text transaction");
            let next_label = text_edit_preview_from_before(&pending.before_text, &current_text)
                .unwrap_or_else(|| "Edit".to_owned());
            pending.last_text = current_text;
            pending.last_edit_at = now;
            if let Some(entry) = self
                .transaction_log
                .last_entry_mut()
                .filter(|entry| entry.id == pending.entry_id)
            {
                entry.action_label = next_label;
            }
            return;
        }

        let before_text = snapshot_before
            .tab_manager
            .active_tab()
            .map(|tab| tab.active_buffer().text())
            .unwrap_or_default();
        let next_label = text_edit_preview_from_before(&before_text, &current_text)
            .unwrap_or_else(|| "Edit".to_owned());
        let entry_id = self.transaction_log.next_entry_id();
        self.transaction_log
            .push(next_label, vec![buffer_label], None, snapshot_before);
        self.pending_text_transaction = Some(PendingTextTransaction {
            entry_id,
            buffer_id,
            before_text,
            last_text: current_text,
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

fn text_edit_preview_from_before(before_text: &str, current_text: &str) -> Option<String> {
    let (_, inserted, _) = changed_span(before_text, current_text);
    inserted_text_preview(inserted)
}

fn changed_span<'a>(before: &'a str, after: &'a str) -> (&'a str, &'a str, &'a str) {
    let prefix_len = before
        .chars()
        .zip(after.chars())
        .take_while(|(left, right)| left == right)
        .count();

    let before_tail = before.chars().count().saturating_sub(prefix_len);
    let after_tail = after.chars().count().saturating_sub(prefix_len);
    let suffix_len = before
        .chars()
        .rev()
        .take(before_tail)
        .zip(after.chars().rev().take(after_tail))
        .take_while(|(left, right)| left == right)
        .count();

    let inserted_end = after.chars().count().saturating_sub(suffix_len);
    let inserted = slice_chars(after, prefix_len, inserted_end);
    let removed = slice_chars(
        before,
        prefix_len,
        before.chars().count().saturating_sub(suffix_len),
    );
    let unchanged_suffix = slice_chars(after, inserted_end, after.chars().count());
    (removed, inserted, unchanged_suffix)
}

fn slice_chars(text: &str, start: usize, end: usize) -> &str {
    let start_byte = if start == 0 {
        0
    } else {
        text.char_indices()
            .nth(start)
            .map(|(index, _)| index)
            .unwrap_or(text.len())
    };
    let end_byte = if end >= text.chars().count() {
        text.len()
    } else {
        text.char_indices()
            .nth(end)
            .map(|(index, _)| index)
            .unwrap_or(text.len())
    };
    &text[start_byte..end_byte]
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

fn text_preview_is_reasonable(before: &str, after: &str) -> bool {
    let (_, inserted, _) = changed_span(before, after);
    let line_count = inserted.lines().count().max(1);
    line_count <= TEXT_EDIT_MAX_PREVIEW_LINES
        && inserted.chars().count() <= TEXT_EDIT_MAX_PREVIEW_CHARS
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
    fn grouped_text_preview_uses_the_start_of_the_transaction() {
        assert_eq!(
            text_edit_preview_from_before("", "hello world"),
            Some("hello world".to_owned())
        );
        assert_eq!(
            text_edit_preview_from_before("hello", "hello world"),
            Some(" world".to_owned())
        );
    }
}
