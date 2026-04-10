use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static NEXT_BUFFER_ID: AtomicU64 = AtomicU64::new(1);
static NEXT_TEMP_BUFFER_ID: AtomicU64 = AtomicU64::new(1);

pub type BufferId = u64;

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

pub struct BufferState {
    pub id: BufferId,
    pub name: String,
    pub content: String,
    pub path: Option<PathBuf>,
    pub is_dirty: bool,
    pub is_settings_file: bool,
    pub temp_id: String,
    pub line_count: usize,
    pub artifact_summary: TextArtifactSummary,
    pub encoding: String,
    pub has_bom: bool,
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
            content,
            path,
            is_dirty: false,
            is_settings_file: false,
            temp_id: next_temp_id(),
            line_count,
            artifact_summary,
            encoding,
            has_bom,
        }
    }

    pub fn restored(restored: RestoredBufferState) -> Self {
        register_existing_buffer_id(restored.id);
        let line_count = display_line_count(&restored.content);
        let artifact_summary = TextArtifactSummary::from_text(&restored.content);
        Self {
            id: restored.id,
            name: restored.name,
            content: restored.content,
            path: restored.path,
            is_dirty: restored.is_dirty,
            is_settings_file: false,
            temp_id: restored.temp_id,
            line_count,
            artifact_summary,
            encoding: restored.encoding,
            has_bom: restored.has_bom,
        }
    }

    pub fn refresh_text_metadata(&mut self) {
        self.line_count = display_line_count(&self.content);
        self.artifact_summary = TextArtifactSummary::from_text(&self.content);
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
