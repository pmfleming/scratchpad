use eframe::egui::{self, TextBuffer};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static NEXT_BUFFER_ID: AtomicU64 = AtomicU64::new(1);
static NEXT_TEMP_BUFFER_ID: AtomicU64 = AtomicU64::new(1);

pub type BufferId = u64;
pub type TextDocumentUndoState = (egui::text::CCursorRange, String);
pub type TextDocumentUndoer = egui::util::undoer::Undoer<TextDocumentUndoState>;
pub const TEXT_DOCUMENT_MAX_UNDOS: usize = 100;

#[derive(Clone)]
pub struct TextDocument {
    text: String,
    undoer: TextDocumentUndoer,
}

impl TextDocument {
    pub fn new(text: String) -> Self {
        Self {
            text,
            undoer: new_text_document_undoer(),
        }
    }

    pub fn as_str(&self) -> &str {
        &self.text
    }

    pub fn undoer(&self) -> TextDocumentUndoer {
        self.undoer.clone()
    }

    pub fn set_undoer(&mut self, undoer: TextDocumentUndoer) {
        self.undoer = undoer;
    }

    pub fn clear_undoer(&mut self) {
        self.undoer = new_text_document_undoer();
    }

    pub fn replace_text(&mut self, text: String) {
        self.text = text;
        self.clear_undoer();
    }
}

fn new_text_document_undoer() -> TextDocumentUndoer {
    TextDocumentUndoer::with_settings(egui::util::undoer::Settings {
        max_undos: TEXT_DOCUMENT_MAX_UNDOS,
        ..Default::default()
    })
}

impl TextBuffer for TextDocument {
    fn is_mutable(&self) -> bool {
        true
    }

    fn as_str(&self) -> &str {
        self.as_str()
    }

    fn insert_text(&mut self, text: &str, char_index: usize) -> usize {
        let byte_idx = self.byte_index_from_char_index(char_index);
        self.text.insert_str(byte_idx, text);
        text.chars().count()
    }

    fn delete_char_range(&mut self, char_range: std::ops::Range<usize>) {
        assert!(
            char_range.start <= char_range.end,
            "start must be <= end, but got {char_range:?}"
        );

        let byte_start = self.byte_index_from_char_index(char_range.start);
        let byte_end = self.byte_index_from_char_index(char_range.end);
        self.text.drain(byte_start..byte_end);
    }

    fn type_id(&self) -> std::any::TypeId {
        std::any::TypeId::of::<Self>()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TextArtifactSummary {
    pub has_ansi_sequences: bool,
    pub has_carriage_returns: bool,
    pub has_backspaces: bool,
    pub other_control_count: usize,
}

impl TextArtifactSummary {
    pub fn from_text(text: &str) -> Self {
        let mut summary = Self::default();
        let mut chars = text.chars().peekable();

        while let Some(ch) = chars.next() {
            match ch {
                '\u{1B}' => {
                    summary.has_ansi_sequences = true;
                }
                '\r' => {
                    if chars.peek() != Some(&'\n') {
                        summary.has_carriage_returns = true;
                    }
                }
                '\u{0008}' => {
                    summary.has_backspaces = true;
                }
                '\n' | '\t' => {}
                _ if ch.is_control() => {
                    summary.other_control_count += 1;
                }
                _ => {}
            }
        }

        summary
    }

    pub fn has_control_chars(&self) -> bool {
        self.has_ansi_sequences
            || self.has_carriage_returns
            || self.has_backspaces
            || self.other_control_count > 0
    }

    pub fn status_text(&self) -> Option<String> {
        if !self.has_control_chars() {
            return None;
        }

        let mut parts = Vec::new();

        if self.has_ansi_sequences {
            parts.push("ANSI");
        }
        if self.has_carriage_returns {
            parts.push("CR");
        }
        if self.has_backspaces {
            parts.push("BS");
        }
        if self.other_control_count > 0 {
            parts.push("CTL");
        }

        Some(format!("Control characters present: {}", parts.join(", ")))
    }
}

pub fn display_line_count(text: &str) -> usize {
    if text.is_empty() {
        return 1;
    }

    let mut count = 1;
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '\n' => {
                count += 1;
            }
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    // CRLF: skip the \n, it's already counted as part of the pair
                    chars.next();
                }
                count += 1;
            }
            _ => {}
        }
    }
    count
}

#[derive(Clone)]
pub struct RenderedLayout {
    pub galley: Arc<eframe::egui::Galley>,
    pub row_line_numbers: Vec<Option<usize>>,
}

impl RenderedLayout {
    pub fn from_galley(galley: Arc<eframe::egui::Galley>) -> Self {
        let row_line_numbers = row_line_numbers_for_galley(&galley);
        Self {
            galley,
            row_line_numbers,
        }
    }

    pub fn visual_row_count(&self) -> usize {
        self.row_line_numbers.len().max(1)
    }
}

fn row_line_numbers_for_galley(galley: &eframe::egui::Galley) -> Vec<Option<usize>> {
    let mut current_line = 1usize;
    let mut starts_new_line = true;
    let mut row_line_numbers = Vec::with_capacity(galley.rows.len());

    for row in &galley.rows {
        row_line_numbers.push(starts_new_line.then_some(current_line));
        starts_new_line = row.ends_with_newline;
        if row.ends_with_newline {
            current_line += 1;
        }
    }

    row_line_numbers
}

#[derive(Clone)]
pub struct BufferState {
    pub id: BufferId,
    pub name: String,
    document: TextDocument,
    pub path: Option<PathBuf>,
    pub is_dirty: bool,
    pub is_settings_file: bool,
    pub temp_id: String,
    pub line_count: usize,
    pub artifact_summary: TextArtifactSummary,
    pub encoding: String,
    pub has_bom: bool,
    pub disk_state: Option<DiskFileState>,
    pub freshness: BufferFreshness,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DiskFileState {
    pub modified_millis: Option<u64>,
    pub len: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BufferFreshness {
    #[default]
    InSync,
    StaleOnDisk,
    ConflictOnDisk,
    MissingOnDisk,
}

pub struct RestoredBufferState {
    pub id: BufferId,
    pub name: String,
    pub content: String,
    pub path: Option<PathBuf>,
    pub is_dirty: bool,
    pub temp_id: String,
    pub encoding: String,
    pub has_bom: bool,
    pub disk_state: Option<DiskFileState>,
    pub freshness: BufferFreshness,
}

impl BufferState {
    pub fn new(name: String, content: String, path: Option<PathBuf>) -> Self {
        Self::with_encoding(name, content, path, "UTF-8".to_string(), false)
    }

    pub fn with_encoding(
        name: String,
        content: String,
        path: Option<PathBuf>,
        encoding: String,
        has_bom: bool,
    ) -> Self {
        let line_count = display_line_count(&content);
        let artifact_summary = TextArtifactSummary::from_text(&content);
        Self {
            id: next_buffer_id(),
            name,
            document: TextDocument::new(content),
            path,
            is_dirty: false,
            is_settings_file: false,
            temp_id: next_temp_id(),
            line_count,
            artifact_summary,
            encoding,
            has_bom,
            disk_state: None,
            freshness: BufferFreshness::InSync,
        }
    }

    pub fn restored(restored: RestoredBufferState) -> Self {
        register_existing_buffer_id(restored.id);
        let line_count = display_line_count(&restored.content);
        let artifact_summary = TextArtifactSummary::from_text(&restored.content);
        Self {
            id: restored.id,
            name: restored.name,
            document: TextDocument::new(restored.content),
            path: restored.path,
            is_dirty: restored.is_dirty,
            is_settings_file: false,
            temp_id: restored.temp_id,
            line_count,
            artifact_summary,
            encoding: restored.encoding,
            has_bom: restored.has_bom,
            disk_state: restored.disk_state,
            freshness: restored.freshness,
        }
    }

    pub fn document(&self) -> &TextDocument {
        &self.document
    }

    pub fn document_mut(&mut self) -> &mut TextDocument {
        &mut self.document
    }

    pub fn text(&self) -> &str {
        self.document.as_str()
    }

    pub fn replace_text(&mut self, text: String) {
        self.document.replace_text(text);
        self.refresh_text_metadata();
    }

    pub fn refresh_text_metadata(&mut self) {
        self.line_count = display_line_count(self.text());
        self.artifact_summary = TextArtifactSummary::from_text(self.text());
    }

    pub fn sync_to_disk_state(&mut self, disk_state: Option<DiskFileState>) {
        self.disk_state = disk_state;
        self.freshness = BufferFreshness::InSync;
    }

    pub fn mark_stale_on_disk(&mut self, disk_state: Option<DiskFileState>) {
        self.disk_state = disk_state;
        self.freshness = BufferFreshness::StaleOnDisk;
    }

    pub fn mark_conflict_on_disk(&mut self, disk_state: Option<DiskFileState>) {
        self.disk_state = disk_state;
        self.freshness = BufferFreshness::ConflictOnDisk;
    }

    pub fn mark_missing_on_disk(&mut self) {
        self.freshness = BufferFreshness::MissingOnDisk;
    }

    pub fn disk_status_label(&self) -> Option<&'static str> {
        match self.freshness {
            BufferFreshness::InSync => None,
            BufferFreshness::StaleOnDisk => Some("On disk changed"),
            BufferFreshness::ConflictOnDisk => Some("Disk conflict"),
            BufferFreshness::MissingOnDisk => Some("File missing"),
        }
    }

    pub fn disk_status_message(&self) -> Option<String> {
        let path_label = self
            .path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| self.name.clone());

        match self.freshness {
            BufferFreshness::InSync => None,
            BufferFreshness::StaleOnDisk => Some(format!("{path_label} changed on disk.")),
            BufferFreshness::ConflictOnDisk => Some(format!(
                "{path_label} changed on disk. Your tab has unsaved edits."
            )),
            BufferFreshness::MissingOnDisk => Some(format!("{path_label} is missing on disk.")),
        }
    }

    pub fn display_name(&self) -> String {
        let marker = if self.is_dirty { "*" } else { "" };
        format!("{}{}", marker, self.name)
    }

    pub fn overflow_context_label(&self) -> Option<String> {
        self.path.as_ref().map(|path| path.display().to_string())
    }
}

fn next_buffer_id() -> BufferId {
    NEXT_BUFFER_ID.fetch_add(1, Ordering::Relaxed)
}

fn register_existing_buffer_id(id: BufferId) {
    let next_id = id.saturating_add(1);
    let mut current = NEXT_BUFFER_ID.load(Ordering::Relaxed);

    while current < next_id {
        match NEXT_BUFFER_ID.compare_exchange(
            current,
            next_id,
            Ordering::Relaxed,
            Ordering::Relaxed,
        ) {
            Ok(_) => break,
            Err(observed) => current = observed,
        }
    }
}

fn next_temp_id() -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let sequence = NEXT_TEMP_BUFFER_ID.fetch_add(1, Ordering::Relaxed);
    format!("buffer-{timestamp}-{sequence}")
}
