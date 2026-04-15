use super::{
    BufferTextMetadata, EncodingSource, TextArtifactSummary, TextDocument, TextFormatMetadata,
    TextReplacementError, TextReplacements, buffer_text_metadata,
};
use eframe::egui;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static NEXT_BUFFER_ID: AtomicU64 = AtomicU64::new(1);
static NEXT_TEMP_BUFFER_ID: AtomicU64 = AtomicU64::new(1);

pub type BufferId = u64;

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
    pub format: TextFormatMetadata,
    pub disk_state: Option<DiskFileState>,
    pub freshness: BufferFreshness,
}

struct BufferBuildState {
    name: String,
    path: Option<PathBuf>,
    is_dirty: bool,
    temp_id: String,
    format: TextFormatMetadata,
    disk_state: Option<DiskFileState>,
    freshness: BufferFreshness,
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
    pub format: TextFormatMetadata,
    pub disk_state: Option<DiskFileState>,
    pub freshness: BufferFreshness,
}

impl BufferState {
    pub fn new(name: String, content: String, path: Option<PathBuf>) -> Self {
        let format = TextFormatMetadata::utf8_for_new_file(&content);
        Self::with_format(name, content, path, format)
    }

    pub fn with_encoding(
        name: String,
        content: String,
        path: Option<PathBuf>,
        encoding: String,
        has_bom: bool,
    ) -> Self {
        let format = TextFormatMetadata::detected(
            &content,
            encoding,
            has_bom,
            EncodingSource::Heuristic,
            false,
        );
        Self::with_format(name, content, path, format)
    }

    pub fn with_format(
        name: String,
        content: String,
        path: Option<PathBuf>,
        mut format: TextFormatMetadata,
    ) -> Self {
        let text_metadata = buffer_text_metadata(&content, &mut format);
        Self::build(
            next_buffer_id(),
            content,
            text_metadata,
            BufferBuildState {
                name,
                path,
                is_dirty: false,
                temp_id: next_temp_id(),
                format,
                disk_state: None,
                freshness: BufferFreshness::InSync,
            },
        )
    }

    pub fn restored(restored: RestoredBufferState) -> Self {
        register_existing_buffer_id(restored.id);
        let mut format = restored.format;
        let text_metadata = buffer_text_metadata(&restored.content, &mut format);
        Self::build(
            restored.id,
            restored.content,
            text_metadata,
            BufferBuildState {
                name: restored.name,
                path: restored.path,
                is_dirty: restored.is_dirty,
                temp_id: restored.temp_id,
                format,
                disk_state: restored.disk_state,
                freshness: restored.freshness,
            },
        )
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
        self.replace_document_text(text, None);
    }

    pub fn replace_text_with_format(&mut self, text: String, format: TextFormatMetadata) {
        self.replace_document_text(text, Some(format));
    }

    pub(crate) fn replace_char_ranges_with_undo(
        &mut self,
        replacements: TextReplacements<'_>,
        previous_selection: egui::text::CCursorRange,
        next_selection: egui::text::CCursorRange,
    ) -> Result<(), TextReplacementError> {
        self.document.replace_char_ranges_with_undo(
            replacements,
            previous_selection,
            next_selection,
        )?;
        self.refresh_text_metadata();
        Ok(())
    }

    pub fn refresh_text_metadata(&mut self) {
        let text_metadata = buffer_text_metadata(self.document.as_str(), &mut self.format);
        self.apply_text_metadata(text_metadata);
    }

    pub fn sync_to_disk_state(&mut self, disk_state: Option<DiskFileState>) {
        self.set_disk_state(disk_state, BufferFreshness::InSync);
    }

    pub fn mark_stale_on_disk(&mut self, disk_state: Option<DiskFileState>) {
        self.set_disk_state(disk_state, BufferFreshness::StaleOnDisk);
    }

    pub fn mark_conflict_on_disk(&mut self, disk_state: Option<DiskFileState>) {
        self.set_disk_state(disk_state, BufferFreshness::ConflictOnDisk);
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

    fn build(
        id: BufferId,
        content: String,
        text_metadata: BufferTextMetadata,
        state: BufferBuildState,
    ) -> Self {
        Self {
            id,
            name: state.name,
            document: TextDocument::with_preferred_line_ending(
                content,
                text_metadata.preferred_line_ending,
            ),
            path: state.path,
            is_dirty: state.is_dirty,
            is_settings_file: false,
            temp_id: state.temp_id,
            line_count: text_metadata.line_count,
            artifact_summary: text_metadata.artifact_summary,
            format: state.format,
            disk_state: state.disk_state,
            freshness: state.freshness,
        }
    }

    fn replace_document_text(&mut self, text: String, format: Option<TextFormatMetadata>) {
        self.document.replace_text(text);
        if let Some(format) = format {
            self.format = format;
        }
        self.refresh_text_metadata();
    }

    fn set_disk_state(&mut self, disk_state: Option<DiskFileState>, freshness: BufferFreshness) {
        self.disk_state = disk_state;
        self.freshness = freshness;
    }

    fn apply_text_metadata(&mut self, text_metadata: BufferTextMetadata) {
        self.line_count = text_metadata.line_count;
        self.artifact_summary = text_metadata.artifact_summary;
        self.document
            .set_preferred_line_ending(text_metadata.preferred_line_ending);
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