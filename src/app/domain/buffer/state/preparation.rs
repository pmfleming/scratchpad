//! Bulk text is normalized, edited, measured, and sealed on the path lane.
use super::{BufferState, TextDocument, TextFormatMetadata};
use crate::app::domain::buffer::{
    BufferTextMetadata, HistoryToken, PieceSource, normalize_inserted_text_line_endings,
    try_analyze_piece_tree_text,
};
use crate::app::domain::{CharCursor, CursorRange};
use std::{
    io,
    ops::Range,
    sync::{Arc, Mutex},
};
pub(crate) const BULK_TEXT_BYTES: usize = 256 * 1024;
#[derive(Clone)]
pub(crate) struct Preparation(Arc<Mutex<Option<PrepareTextRequest>>>);
type EditTransform = Box<
    dyn FnOnce() -> io::Result<(
            TextDocument,
            TextFormatMetadata,
            CursorRange,
            Vec<(Range<usize>, usize)>,
        )> + Send,
>;
pub(crate) struct PrepareTextRequest {
    transform: Option<(bool, bool, EditTransform)>,
    copy_metadata: Option<BufferTextMetadata>,
    base: HistoryToken,
    document: TextDocument,
    format: TextFormatMetadata,
    replacements: Vec<(Range<usize>, String)>,
    history_target: Option<u64>,
    event: super::TextHistoryEvent,
    previous: CursorRange,
    next: Option<CursorRange>,
    source: PieceSource,
    origin_view: Option<u64>,
}
pub(crate) struct PreparedText {
    pub changes_document: bool,
    base: HistoryToken,
    pub document: TextDocument,
    pub format: TextFormatMetadata,
    pub metadata: BufferTextMetadata,
    pub selection: CursorRange,
    pub origin_view: Option<u64>,
    pub previous: CursorRange,
    changes: Vec<(Range<usize>, usize)>,
    event: super::TextHistoryEvent,
}
impl PrepareTextRequest {
    pub(crate) fn prepare_with_store(
        mut self,
        store: Option<crate::app::services::chunk_store::ChunkStoreHandle>,
    ) -> io::Result<PreparedText> {
        if let Some(store) = store {
            if self.adds_text() {
                store.check_bulk_capacity(
                    self.replacements.iter().map(|(_, text)| text.len()).sum(),
                )?;
            }
            self.document.set_bulk_intake(true);
            self.document.bind_store(store)?;
        }
        self.prepare()
    }
    fn adds_text(&self) -> bool {
        self.replacements.iter().any(|(_, text)| !text.is_empty())
            || self.transform.as_ref().is_some_and(|(_, adds, _)| *adds)
    }
    pub(crate) fn prepare(mut self) -> io::Result<PreparedText> {
        let adds_text = self.adds_text();
        let mut changes_document = true;
        let mut transform_changes = None;
        for (_, text) in &mut self.replacements {
            let normalized = normalize_inserted_text_line_endings(
                text,
                self.format.preferred_line_ending_style(),
            );
            if matches!(normalized, std::borrow::Cow::Owned(_)) {
                *text = normalized.into_owned();
            }
        }
        let next = if let Some((changes, _, transform)) = self.transform.take() {
            changes_document = changes;
            let (document, format, selection, updates) = std::panic::catch_unwind(
                std::panic::AssertUnwindSafe(transform),
            )
            .map_err(|_| io::Error::other("Text became unreadable while preparing the edit"))??;
            self.document = document;
            self.format = format;
            transform_changes = Some(updates);
            selection
        } else if let Some(target) = self.history_target {
            self.document.navigate_history(target).map_err(|cause| {
                io::Error::other(format!("Could not prepare history: {cause:?}"))
            })?
        } else {
            let next = self.next.unwrap_or_else(|| {
                let (range, text) = &self.replacements[0];
                CursorRange::one(CharCursor::new(range.start + text.chars().count()))
            });
            self.document.set_bulk_intake(true);
            self.document
                .replace_char_ranges_with_source(
                    &self.replacements,
                    self.previous,
                    next,
                    self.source,
                )
                .map_err(|cause| {
                    if cause == crate::app::domain::buffer::TextReplacementError::BulkRefused {
                        return io::Error::other("Memory limit reached. Large paste or replacement refused; current text was kept.");
                    }
                    io::Error::other(
                        self.document
                            .piece_tree()
                            .intake_error()
                            .unwrap_or_else(|| format!("Could not prepare text: {cause:?}")),
                    )
                })?;
            next
        };
        self.document.set_bulk_intake(false);
        if let Some(cause) = self.document.piece_tree().intake_error() {
            return Err(io::Error::other(cause));
        }
        if adds_text && let Some(store) = self.document.piece_tree().store() {
            store.check_bulk_capacity(0)?;
        }
        if self.document.piece_tree().read_fault().is_some() {
            return Err(io::Error::other(
                "Text became unreadable during preparation",
            ));
        }
        self.document.piece_tree_mut().clear_anchors();
        let metadata = if changes_document {
            try_analyze_piece_tree_text(self.document.piece_tree(), &mut self.format)?.metadata
        } else {
            self.copy_metadata.take().expect("copy metadata captured")
        };
        Ok(PreparedText {
            base: self.base,
            changes_document,
            event: self.event,
            document: self.document,
            format: self.format,
            metadata,
            selection: next,
            origin_view: self.origin_view,
            previous: self.previous,
            changes: transform_changes.unwrap_or_else(|| {
                self.replacements
                    .iter()
                    .map(|(range, text)| (range.clone(), text.chars().count()))
                    .collect()
            }),
        })
    }
}
impl BufferState {
    pub(crate) fn queue_edit_transform(
        &mut self,
        previous: CursorRange,
        origin_view: u64,
        changes_document: bool,
        adds_text: bool,
        transform: impl FnOnce() -> io::Result<(
            TextDocument,
            TextFormatMetadata,
            CursorRange,
            Vec<(Range<usize>, usize)>,
        )> + Send
        + 'static,
    ) -> bool {
        if self.is_preparing_text() || self.is_loading_preview {
            return false;
        }
        let metadata = BufferTextMetadata {
            line_count: self.line_count,
            artifact_summary: self.artifact_summary.clone(),
            preferred_line_ending: self.format.preferred_line_ending,
            has_non_compliant_characters: self.has_non_compliant_characters,
        };
        let request = PrepareTextRequest {
            base: self.document.history_token(),
            document: self.document.clone(),
            format: self.format.clone(),
            transform: Some((changes_document, adds_text, Box::new(transform))),
            copy_metadata: Some(metadata),
            replacements: Vec::new(),
            history_target: None,
            event: super::TextHistoryEvent::Edit,
            previous,
            next: Some(previous),
            source: PieceSource::Edit,
            origin_view: Some(origin_view),
        };
        self.preparation = Some(Preparation(Arc::new(Mutex::new(Some(request)))));
        true
    }

    pub(crate) fn queue_history_preparation(
        &mut self,
        target: u64,
        previous: CursorRange,
        origin_view: Option<u64>,
        event: super::TextHistoryEvent,
    ) -> bool {
        if self.is_preparing_text() || self.is_loading_preview {
            return false;
        }
        let request = PrepareTextRequest {
            transform: None,
            copy_metadata: None,
            base: self.document.history_token(),
            document: self.document.clone(),
            format: self.format.clone(),
            replacements: Vec::new(),
            history_target: Some(target),
            event,
            previous,
            next: None,
            source: PieceSource::Edit,
            origin_view,
        };
        self.preparation = Some(Preparation(Arc::new(Mutex::new(Some(request)))));
        true
    }

    pub(crate) fn prepare_loaded_replacement(
        mut self,
        loaded: Self,
        source: Option<PieceSource>,
    ) -> io::Result<Self> {
        let base = self
            .prepared_history_base
            .take()
            .unwrap_or_else(|| self.document.history_token());
        self.document.set_bulk_intake(true);
        let replaced = match source {
            Some(source) => self.replace_from_loaded_buffer_source(loaded, source),
            None => self.replace_from_loaded_buffer(loaded),
        };
        if !replaced {
            return Err(io::Error::other(
                "Could not prepare loaded text; current text was kept",
            ));
        }
        self.document.set_bulk_intake(false);
        self.prepared_history_base = Some(base);
        Ok(self)
    }
    #[must_use]
    pub fn is_preparing_text(&self) -> bool {
        self.preparation.is_some()
    }
    pub(crate) fn queue_text_preparation(
        &mut self,
        replacements: Vec<(Range<usize>, String)>,
        previous: CursorRange,
        next: Option<CursorRange>,
        source: PieceSource,
        origin_view: Option<u64>,
    ) -> bool {
        if let Some(store) = self.document.piece_tree().store()
            && let Err(cause) =
                store.check_bulk_capacity(replacements.iter().map(|(_, text)| text.len()).sum())
        {
            self.storage_refusal = Some(cause.to_string());
            return false;
        }
        if self.is_loading_preview
            || self.is_preparing_text()
            || replacements.is_empty()
            || self.document.piece_tree().read_fault().is_some()
            || self
                .document
                .validate_char_replacements(&replacements)
                .is_err()
        {
            return false;
        }
        let request = PrepareTextRequest {
            transform: None,
            copy_metadata: None,
            base: self.document.history_token(),
            document: self.document.clone(),
            format: self.format.clone(),
            replacements,
            history_target: None,
            event: super::TextHistoryEvent::Edit,
            previous,
            next,
            source,
            origin_view,
        };
        self.preparation = Some(Preparation(Arc::new(Mutex::new(Some(request)))));
        true
    }
    pub(crate) fn take_text_preparation(&mut self) -> Option<PrepareTextRequest> {
        self.preparation
            .as_ref()?
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
    }
    pub(crate) fn retry_text_preparation(&mut self, request: PrepareTextRequest) {
        if let Some(preparation) = &self.preparation {
            *preparation
                .0
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(request);
        }
    }
    pub(crate) fn cancel_text_preparation(&mut self) {
        self.preparation = None;
    }
    pub(crate) fn install_prepared_text(
        &mut self,
        prepared: PreparedText,
    ) -> Result<CursorRange, Box<PreparedText>> {
        self.preparation = None;
        if self.document.history_token() != prepared.base {
            return Err(Box::new(prepared));
        }
        let selection = prepared.selection;
        let event = prepared.event;
        self.document = prepared.document;
        self.format = prepared.format;
        self.apply_text_metadata_refresh(
            self.document_revision(),
            prepared.metadata.line_count,
            prepared.metadata.artifact_summary,
            self.format.clone(),
        );
        self.active_selection = None;
        self.is_dirty = true;
        self.note_text_history_event(event);
        Ok(selection)
    }
}

impl PreparedText {
    pub(crate) fn transformed_selection(
        &self,
        view: u64,
        selection: Option<CursorRange>,
    ) -> CursorRange {
        let mut selection = selection.unwrap_or(self.previous);
        if self.origin_view == Some(view) && selection == self.previous {
            return self.selection;
        }
        for (range, inserted) in &self.changes {
            for cursor in [&mut selection.primary, &mut selection.secondary] {
                if cursor.index >= range.end {
                    cursor.index = cursor
                        .index
                        .saturating_sub(range.len())
                        .saturating_add(*inserted);
                } else if cursor.index >= range.start {
                    cursor.index = range.start + inserted;
                }
            }
        }
        let end = self.document.piece_tree().len_chars();
        selection.primary.index = selection.primary.index.min(end);
        selection.secondary.index = selection.secondary.index.min(end);
        selection
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::services::session_store::SessionStore;
    #[test]
    fn prepared_unicode_is_atomic_undoable_bound_and_scan_preserves_the_viewport_cache() {
        let root = tempfile::tempdir().unwrap();
        let store = SessionStore::new(root.path().into());
        let handle = store.chunk_store_handle().unwrap();
        let mut buffer = BufferState::new("paste".into(), "base".into(), None);
        buffer.document.bind_store(handle.clone()).unwrap();
        let snapshot = buffer.document.snapshot();
        let text = "λ🦀\n".repeat(400_000);
        let previous = CursorRange::one(CharCursor::new(0));
        assert!(buffer.queue_text_preparation(
            vec![(0..0, text.clone())],
            previous,
            None,
            PieceSource::Paste,
            None
        ));
        assert_eq!(buffer.text(), "base");
        assert!(buffer.is_preparing_text());
        let prepared = buffer
            .take_text_preparation()
            .unwrap()
            .prepare_with_store(Some(handle.clone()))
            .unwrap();
        let selection = prepared.selection;
        assert!(buffer.install_prepared_text(prepared).is_ok());
        let expected = normalize_inserted_text_line_endings(
            &text,
            buffer.format.preferred_line_ending_style(),
        )
        .into_owned();
        assert_eq!(selection.primary.index, expected.chars().count());
        assert!(!buffer.is_preparing_text());
        assert_eq!(buffer.text(), format!("{expected}base"));
        assert_eq!(snapshot.extract_text(), "base");
        store.flush_text_chunks().unwrap();
        handle.set_limits(0, 0);
        handle.set_limits(4 * 1024 * 1024, 1024 * 1024);
        let _pin = buffer.document.piece_tree().pin_range(0..1).unwrap();
        let cached = handle.cached_bytes();
        assert!(cached > 0);
        buffer.document.try_content_digest().unwrap();
        assert_eq!(
            handle.cached_bytes(),
            cached,
            "scan must not populate or evict viewport bodies"
        );
        let mut restored = buffer.document.clone();
        restored.undo_last_operation().unwrap();
        assert_eq!(restored.extract_text(), "base");
        restored.redo_last_operation().unwrap();
        assert_eq!(restored.extract_text(), format!("{expected}base"));
    }
    #[test]
    fn unbound_sealed_text_and_divergent_clones_remain_readable() {
        let mut document = TextDocument::new(String::new());
        let text = "λ".repeat(300_000);
        document.insert_direct(0, &text);
        let original = document.snapshot();
        let mut clone = document.clone();
        document.insert_direct(0, "first");
        clone.insert_direct(0, "second");
        assert_eq!(original.extract_text(), text);
        assert_eq!(document.extract_text(), format!("first{text}"));
        assert_eq!(clone.extract_text(), format!("second{text}"));
    }
}

#[cfg(test)]
mod history_preparation_tests {
    use super::*;
    use crate::app::services::session_store::SessionStore;
    #[test]
    fn prepared_history_is_atomic_and_reuses_existing_text_without_new_body_writes() {
        let root = tempfile::tempdir().unwrap();
        let session = SessionStore::new(root.path().to_owned());
        let store = session.chunk_store_handle().unwrap();
        let mut buffer = BufferState::new("history".into(), "base".into(), None);
        buffer.document_mut().bind_store(store.clone()).unwrap();
        let initial = buffer.document().history_now();
        let text = "λ\n".repeat(200_000);
        buffer.queue_text_preparation(
            vec![(0..0, text.clone())],
            CursorRange::default(),
            None,
            PieceSource::Paste,
            None,
        );
        let prepared = buffer
            .take_text_preparation()
            .unwrap()
            .prepare_with_store(Some(store.clone()))
            .unwrap();
        assert!(buffer.install_prepared_text(prepared).is_ok());
        let pasted = buffer.document().history_now();
        session.flush_text_chunks().unwrap();
        let hashed = store.hashed_bytes();
        assert!(buffer.document().history_needs_preparation(initial));
        buffer.queue_history_preparation(
            initial,
            CursorRange::default(),
            None,
            super::super::TextHistoryEvent::Replay,
        );
        assert_eq!(buffer.document().piece_tree().len_chars(), 400_004);
        let prepared = buffer
            .take_text_preparation()
            .unwrap()
            .prepare_with_store(Some(store.clone()))
            .unwrap();
        assert!(buffer.install_prepared_text(prepared).is_ok());
        assert_eq!(buffer.text(), "base");
        buffer.queue_history_preparation(
            pasted,
            CursorRange::default(),
            None,
            super::super::TextHistoryEvent::Navigate,
        );
        let prepared = buffer
            .take_text_preparation()
            .unwrap()
            .prepare_with_store(Some(store.clone()))
            .unwrap();
        assert!(buffer.install_prepared_text(prepared).is_ok());
        assert_eq!(buffer.text(), format!("{text}base"));
        session.flush_text_chunks().unwrap();
        assert_eq!(store.hashed_bytes(), hashed);
    }
}
