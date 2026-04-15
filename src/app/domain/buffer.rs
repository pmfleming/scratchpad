use eframe::egui::{self, TextBuffer};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
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
    preferred_line_ending: LineEndingStyle,
}

impl TextDocument {
    pub fn new(text: String) -> Self {
        Self::with_preferred_line_ending(text, platform_default_line_ending())
    }

    pub fn with_preferred_line_ending(
        text: String,
        preferred_line_ending: LineEndingStyle,
    ) -> Self {
        Self {
            text,
            undoer: new_text_document_undoer(),
            preferred_line_ending,
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

    pub fn set_preferred_line_ending(&mut self, preferred_line_ending: LineEndingStyle) {
        self.preferred_line_ending = preferred_line_ending;
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
        let normalized_text = normalize_editor_inserted_text(text, self.preferred_line_ending);
        self.text.insert_str(byte_idx, normalized_text.as_ref());
        normalized_text.chars().count()
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

fn normalize_editor_inserted_text(
    text: &str,
    preferred_line_ending: LineEndingStyle,
) -> Cow<'_, str> {
    match text {
        "\r" | "\r\n" | "\n" => Cow::Borrowed(preferred_line_ending.as_str()),
        _ => Cow::Borrowed(text),
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LineEndingStyle {
    #[default]
    None,
    Lf,
    Crlf,
    Cr,
    Mixed,
}

impl LineEndingStyle {
    pub fn label(self) -> &'static str {
        match self {
            Self::None => "None",
            Self::Lf => "LF",
            Self::Crlf => "CRLF",
            Self::Cr => "CR",
            Self::Mixed => "Mixed",
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::None | Self::Lf | Self::Mixed => "\n",
            Self::Crlf => "\r\n",
            Self::Cr => "\r",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LineEndingCounts {
    pub lf: usize,
    pub crlf: usize,
    pub cr: usize,
}

impl LineEndingCounts {
    fn dominant_style(self) -> Option<LineEndingStyle> {
        let mut entries = [
            (self.crlf, LineEndingStyle::Crlf),
            (self.lf, LineEndingStyle::Lf),
            (self.cr, LineEndingStyle::Cr),
        ];
        entries.sort_by(|left, right| right.0.cmp(&left.0));
        (entries[0].0 > 0).then_some(entries[0].1)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EncodingSource {
    Bom,
    #[default]
    Heuristic,
    ExplicitUserChoice,
    DefaultForNewFile,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextFormatMetadata {
    pub encoding_name: String,
    pub has_bom: bool,
    pub line_endings: LineEndingStyle,
    pub line_ending_counts: LineEndingCounts,
    pub encoding_source: EncodingSource,
    pub is_ascii_subset: bool,
    pub has_decoding_warnings: bool,
}

impl TextFormatMetadata {
    pub fn utf8_for_new_file(text: &str) -> Self {
        let mut format = Self {
            encoding_name: "UTF-8".to_owned(),
            has_bom: false,
            line_endings: platform_default_line_ending(),
            line_ending_counts: LineEndingCounts::default(),
            encoding_source: EncodingSource::DefaultForNewFile,
            is_ascii_subset: text.is_ascii(),
            has_decoding_warnings: false,
        };
        format.refresh_from_text(text);
        format
    }

    pub fn detected(
        text: &str,
        encoding_name: String,
        has_bom: bool,
        encoding_source: EncodingSource,
        has_decoding_warnings: bool,
    ) -> Self {
        let mut format = Self {
            encoding_name,
            has_bom,
            line_endings: LineEndingStyle::None,
            line_ending_counts: LineEndingCounts::default(),
            encoding_source,
            is_ascii_subset: text.is_ascii(),
            has_decoding_warnings,
        };
        format.refresh_from_text(text);
        format
    }

    pub fn refresh_from_text(&mut self, text: &str) {
        let (line_ending_counts, line_endings) = analyze_line_endings(text);
        self.line_ending_counts = line_ending_counts;
        self.line_endings = line_endings;
        self.is_ascii_subset = text.is_ascii();
    }

    pub fn encoding_label(&self) -> String {
        let base = match self.encoding_name.as_str() {
            "windows-1252" => "Windows-1252 (ANSI)".to_owned(),
            "UTF-8" => "UTF-8".to_owned(),
            "UTF-16LE" => "UTF-16LE".to_owned(),
            "UTF-16BE" => "UTF-16BE".to_owned(),
            other => other.to_owned(),
        };

        if self.has_bom {
            format!("{base} BOM")
        } else {
            base
        }
    }

    pub fn encoding_tooltip(&self) -> String {
        let source = match self.encoding_source {
            EncodingSource::Bom => "Detected from BOM",
            EncodingSource::Heuristic => "Detected heuristically",
            EncodingSource::ExplicitUserChoice => "Selected explicitly",
            EncodingSource::DefaultForNewFile => "Default for new files",
        };
        let ascii = if self.is_ascii_subset {
            "; ASCII-only content"
        } else {
            ""
        };
        format!("{}{}", source, ascii)
    }

    pub fn line_endings_label(&self) -> &'static str {
        self.line_endings.label()
    }

    pub fn format_warning_text(&self) -> Option<String> {
        let mut warnings = Vec::new();
        if self.line_endings == LineEndingStyle::Mixed {
            warnings.push("Mixed line endings detected".to_owned());
        }
        if self.has_decoding_warnings {
            warnings.push("Decoding substitutions present".to_owned());
        }

        if warnings.is_empty() {
            None
        } else {
            Some(warnings.join("; "))
        }
    }

    pub fn preferred_line_ending_style(&self) -> LineEndingStyle {
        match self.line_endings {
            LineEndingStyle::Lf | LineEndingStyle::Crlf | LineEndingStyle::Cr => self.line_endings,
            LineEndingStyle::Mixed => self
                .line_ending_counts
                .dominant_style()
                .unwrap_or_else(platform_default_line_ending),
            LineEndingStyle::None => platform_default_line_ending(),
        }
    }
}

pub fn platform_default_line_ending() -> LineEndingStyle {
    if cfg!(windows) {
        LineEndingStyle::Crlf
    } else {
        LineEndingStyle::Lf
    }
}

pub fn analyze_line_endings(text: &str) -> (LineEndingCounts, LineEndingStyle) {
    let mut counts = LineEndingCounts::default();
    let mut chars = text.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    counts.crlf += 1;
                    chars.next();
                } else {
                    counts.cr += 1;
                }
            }
            '\n' => counts.lf += 1,
            _ => {}
        }
    }

    let nonzero = [counts.lf > 0, counts.crlf > 0, counts.cr > 0]
        .into_iter()
        .filter(|present| *present)
        .count();
    let style = match nonzero {
        0 => LineEndingStyle::None,
        1 if counts.crlf > 0 => LineEndingStyle::Crlf,
        1 if counts.lf > 0 => LineEndingStyle::Lf,
        1 => LineEndingStyle::Cr,
        _ => LineEndingStyle::Mixed,
    };

    (counts, style)
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
        let (_, line_endings) = analyze_line_endings(text);
        Self::from_text_with_line_endings(text, line_endings)
    }

    pub fn from_text_with_line_endings(text: &str, line_endings: LineEndingStyle) -> Self {
        let mut summary = Self::default();
        let mut chars = text.chars().peekable();
        let structural_carriage_returns = line_endings == LineEndingStyle::Cr;

        while let Some(ch) = chars.next() {
            match ch {
                '\u{1B}' => {
                    summary.has_ansi_sequences = true;
                }
                '\r' => {
                    if chars.peek() == Some(&'\n') {
                        chars.next();
                    } else if !structural_carriage_returns {
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

        Some(format!("Control characters detected: {}", parts.join(", ")))
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
    pub format: TextFormatMetadata,
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
    pub format: TextFormatMetadata,
    pub disk_state: Option<DiskFileState>,
    pub freshness: BufferFreshness,
}

impl BufferState {
    pub fn new(name: String, content: String, path: Option<PathBuf>) -> Self {
        Self::with_format(
            name,
            content,
            path,
            TextFormatMetadata::utf8_for_new_file(""),
        )
    }

    pub fn with_encoding(
        name: String,
        content: String,
        path: Option<PathBuf>,
        encoding: String,
        has_bom: bool,
    ) -> Self {
        Self::with_format(
            name,
            content.clone(),
            path,
            TextFormatMetadata::detected(
                &content,
                encoding,
                has_bom,
                EncodingSource::Heuristic,
                false,
            ),
        )
    }

    pub fn with_format(
        name: String,
        content: String,
        path: Option<PathBuf>,
        mut format: TextFormatMetadata,
    ) -> Self {
        let line_count = display_line_count(&content);
        format.refresh_from_text(&content);
        let artifact_summary =
            TextArtifactSummary::from_text_with_line_endings(&content, format.line_endings);
        Self {
            id: next_buffer_id(),
            name,
            document: TextDocument::with_preferred_line_ending(
                content,
                format.preferred_line_ending_style(),
            ),
            path,
            is_dirty: false,
            is_settings_file: false,
            temp_id: next_temp_id(),
            line_count,
            artifact_summary,
            format,
            disk_state: None,
            freshness: BufferFreshness::InSync,
        }
    }

    pub fn restored(restored: RestoredBufferState) -> Self {
        register_existing_buffer_id(restored.id);
        let line_count = display_line_count(&restored.content);
        let mut format = restored.format;
        format.refresh_from_text(&restored.content);
        let artifact_summary = TextArtifactSummary::from_text_with_line_endings(
            &restored.content,
            format.line_endings,
        );
        Self {
            id: restored.id,
            name: restored.name,
            document: TextDocument::with_preferred_line_ending(
                restored.content,
                format.preferred_line_ending_style(),
            ),
            path: restored.path,
            is_dirty: restored.is_dirty,
            is_settings_file: false,
            temp_id: restored.temp_id,
            line_count,
            artifact_summary,
            format,
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

    pub fn replace_text_with_format(&mut self, text: String, mut format: TextFormatMetadata) {
        self.document.replace_text(text);
        let current_text = self.text().to_owned();
        format.refresh_from_text(&current_text);
        self.format = format;
        self.refresh_text_metadata();
    }

    pub fn refresh_text_metadata(&mut self) {
        let current_text = self.text().to_owned();
        self.line_count = display_line_count(&current_text);
        self.format.refresh_from_text(&current_text);
        self.document
            .set_preferred_line_ending(self.format.preferred_line_ending_style());
        self.artifact_summary = TextArtifactSummary::from_text_with_line_endings(
            &current_text,
            self.format.line_endings,
        );
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
