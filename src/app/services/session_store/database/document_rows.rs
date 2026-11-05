//! Typed document metadata. No text bodies or serialized document payloads.
use super::{Database, error, invalid, rows::MetadataRow};
use crate::app::domain::buffer::BufferTextMetadata;
use crate::app::domain::{LineEndingCounts, TextFormatMetadata};
use crate::app::services::session_store::model::SessionBuffer;
use std::io;

pub(super) const SCHEMA: &str = "CREATE TABLE documents(
id TEXT PRIMARY KEY,
buffer_id INTEGER NOT NULL CHECK(buffer_id>0),
name TEXT NOT NULL,
path TEXT,
path_key TEXT,
search_text TEXT NOT NULL,
is_dirty INTEGER NOT NULL CHECK(is_dirty IN (0,1)),
text_is_empty INTEGER NOT NULL CHECK(text_is_empty IN (0,1)),
has_recorded_edits INTEGER NOT NULL CHECK(has_recorded_edits IN (0,1)),
is_settings_file INTEGER NOT NULL CHECK(is_settings_file IN (0,1)),
show_control_chars INTEGER NOT NULL CHECK(show_control_chars IN (0,1)),
right_to_left INTEGER NOT NULL CHECK(right_to_left IN (0,1)),
encoding_name TEXT NOT NULL,
has_bom INTEGER NOT NULL CHECK(has_bom IN (0,1)),
encoding_source TEXT NOT NULL,
line_endings TEXT NOT NULL,
preferred_line_ending TEXT NOT NULL,
lf_count INTEGER NOT NULL CHECK(lf_count>=0),
crlf_count INTEGER NOT NULL CHECK(crlf_count>=0),
cr_count INTEGER NOT NULL CHECK(cr_count>=0),
is_ascii_subset INTEGER NOT NULL CHECK(is_ascii_subset IN (0,1)),
has_decoding_warnings INTEGER NOT NULL CHECK(has_decoding_warnings IN (0,1)),
line_count INTEGER NOT NULL CHECK(line_count>=0),
disk_modified_millis INTEGER,
disk_len INTEGER,
disk_content_digest TEXT,
observed_digest TEXT,
observed_missing INTEGER NOT NULL CHECK(observed_missing IN (0,1)),artifact_flags INTEGER NOT NULL CHECK(artifact_flags BETWEEN 0 AND 31),other_control_count INTEGER NOT NULL CHECK(other_control_count>=0),ansi_count INTEGER,backspace_count INTEGER,unicode_format_count INTEGER,other_control_exact INTEGER) STRICT;
CREATE INDEX documents_path ON documents(path_key);
CREATE INDEX documents_buffer ON documents(buffer_id);";
pub(super) const COLUMNS: &str = "id,buffer_id,name,path,path_key,search_text,is_dirty,text_is_empty,has_recorded_edits,is_settings_file,show_control_chars,right_to_left,encoding_name,has_bom,encoding_source,line_endings,preferred_line_ending,lf_count,crlf_count,cr_count,is_ascii_subset,has_decoding_warnings,line_count,disk_modified_millis,disk_len,disk_content_digest,observed_digest,observed_missing,artifact_flags,other_control_count,ansi_count,backspace_count,unicode_format_count,other_control_exact";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct DocumentRow {
    pub id: String,
    pub buffer_id: u64,
    pub name: String,
    pub path: Option<String>,
    pub path_key: Option<String>,
    pub search_text: String,
    pub is_dirty: bool,
    pub text_is_empty: bool,
    pub has_recorded_edits: bool,
    pub is_settings_file: bool,
    pub show_control_chars: bool,
    pub right_to_left: bool,
    pub encoding_name: String,
    pub has_bom: bool,
    pub encoding_source: String,
    pub line_endings: String,
    pub preferred_line_ending: String,
    pub lf_count: u64,
    pub crlf_count: u64,
    pub cr_count: u64,
    pub is_ascii_subset: bool,
    pub has_decoding_warnings: bool,
    pub line_count: u64,
    pub disk_modified_millis: Option<u64>,
    pub disk_len: Option<u64>,
    pub disk_content_digest: Option<String>,
    pub observed_digest: Option<String>,
    pub observed_missing: bool,
    pub analysis: BufferTextMetadata,
}
impl DocumentRow {
    pub fn capture(b: &SessionBuffer) -> io::Result<Self> {
        Ok(Self {
            id: b.temp_id.clone(),
            buffer_id: b.id,
            name: b.name.clone(),
            path: b
                .path
                .as_ref()
                .map(|p| {
                    p.to_str()
                        .map(str::to_owned)
                        .ok_or_else(|| invalid("document path is not UTF-8"))
                })
                .transpose()?,
            path_key: b
                .path
                .as_deref()
                .map(crate::app::CanonicalPathKey::from_path)
                .map(|p| p.as_str().to_owned()),
            search_text: format!(
                "{}\n{}",
                b.name,
                b.path
                    .as_deref()
                    .map(|p| p
                        .to_str()
                        .ok_or_else(|| invalid("document path is not UTF-8")))
                    .transpose()?
                    .unwrap_or_default()
            )
            .to_lowercase(),
            is_dirty: b.is_dirty,
            text_is_empty: b.text_is_empty,
            has_recorded_edits: b.has_recorded_edits,
            is_settings_file: b.is_settings_file,
            show_control_chars: b.show_control_chars,
            right_to_left: b.right_to_left_reading_order,
            encoding_name: b.format.encoding_name.clone(),
            has_bom: b.format.has_bom,
            encoding_source: enum_name(b.format.encoding_source)?,
            line_endings: enum_name(b.format.line_endings)?,
            preferred_line_ending: enum_name(b.format.preferred_line_ending)?,
            lf_count: b.format.line_ending_counts.lf as u64,
            crlf_count: b.format.line_ending_counts.crlf as u64,
            cr_count: b.format.line_ending_counts.cr as u64,
            is_ascii_subset: b.format.is_ascii_subset,
            has_decoding_warnings: b.format.has_decoding_warnings,
            line_count: b.line_count as u64,
            disk_modified_millis: b.disk_modified_millis,
            disk_len: b.disk_len,
            disk_content_digest: b.disk_content_digest.clone(),
            observed_digest: b.observed_disk_digest.clone(),
            observed_missing: b.observed_disk_missing,
            analysis: b
                .text_metadata
                .clone()
                .unwrap_or_else(|| BufferTextMetadata {
                    line_count: b.line_count,
                    artifact_summary: crate::app::domain::TextArtifactSummary::default(),
                    preferred_line_ending: b.format.preferred_line_ending,
                    has_non_compliant_characters: false,
                }),
        })
    }
    pub fn read(row: &turso::Row) -> io::Result<Self> {
        let checked = MetadataRow(row);
        Ok(Self {
            id: row.get(0).map_err(error)?,
            buffer_id: checked.unsigned(1)?,
            name: row.get(2).map_err(error)?,
            path: row.get(3).map_err(error)?,
            path_key: row.get(4).map_err(error)?,
            search_text: row.get(5).map_err(error)?,
            is_dirty: checked.boolean(6)?,
            text_is_empty: checked.boolean(7)?,
            has_recorded_edits: checked.boolean(8)?,
            is_settings_file: checked.boolean(9)?,
            show_control_chars: checked.boolean(10)?,
            right_to_left: checked.boolean(11)?,
            encoding_name: row.get(12).map_err(error)?,
            has_bom: checked.boolean(13)?,
            encoding_source: row.get(14).map_err(error)?,
            line_endings: row.get(15).map_err(error)?,
            preferred_line_ending: row.get(16).map_err(error)?,
            lf_count: checked.unsigned(17)?,
            crlf_count: checked.unsigned(18)?,
            cr_count: checked.unsigned(19)?,
            is_ascii_subset: checked.boolean(20)?,
            has_decoding_warnings: checked.boolean(21)?,
            line_count: checked.unsigned(22)?,
            disk_modified_millis: checked.optional_unsigned(23)?,
            disk_len: checked.optional_unsigned(24)?,
            disk_content_digest: row.get(25).map_err(error)?,
            observed_digest: row.get(26).map_err(error)?,
            observed_missing: checked.boolean(27)?,
            analysis: read_analysis(row)?,
        })
    }
    pub fn into_buffer(self) -> io::Result<SessionBuffer> {
        Ok(SessionBuffer {
            id: self.buffer_id,
            temp_id: self.id,
            name: self.name,
            path: self.path.map(Into::into),
            is_dirty: self.is_dirty,
            text_is_empty: self.text_is_empty,
            has_recorded_edits: self.has_recorded_edits,
            is_settings_file: self.is_settings_file,
            show_control_chars: self.show_control_chars,
            right_to_left_reading_order: self.right_to_left,
            line_count: usize::try_from(self.line_count).map_err(invalid)?,
            text_metadata: Some(self.analysis),
            format: TextFormatMetadata {
                encoding_name: self.encoding_name,
                has_bom: self.has_bom,
                encoding_source: parse_enum(self.encoding_source)?,
                line_endings: parse_enum(self.line_endings)?,
                preferred_line_ending: parse_enum(self.preferred_line_ending)?,
                line_ending_counts: LineEndingCounts {
                    lf: usize::try_from(self.lf_count).map_err(invalid)?,
                    crlf: usize::try_from(self.crlf_count).map_err(invalid)?,
                    cr: usize::try_from(self.cr_count).map_err(invalid)?,
                },
                is_ascii_subset: self.is_ascii_subset,
                has_decoding_warnings: self.has_decoding_warnings,
            },
            disk_modified_millis: self.disk_modified_millis,
            disk_len: self.disk_len,
            disk_content_digest: self.disk_content_digest,
            observed_disk_digest: self.observed_digest,
            observed_disk_missing: self.observed_missing,
            pending_disk_observation: false,
            text_history: Vec::new(),
            history_document: None,
            history_capture: None,
        })
    }
}

impl Database {
    pub(super) async fn write_document_row(
        &self,
        row: &DocumentRow,
        existing: bool,
    ) -> io::Result<()> {
        let sql = if existing {
            "UPDATE documents SET buffer_id=?2,name=?3,path=?4,path_key=?5,search_text=?6,is_dirty=?7,text_is_empty=?8,has_recorded_edits=?9,is_settings_file=?10,show_control_chars=?11,right_to_left=?12,encoding_name=?13,has_bom=?14,encoding_source=?15,line_endings=?16,preferred_line_ending=?17,lf_count=?18,crlf_count=?19,cr_count=?20,is_ascii_subset=?21,has_decoding_warnings=?22,line_count=?23,disk_modified_millis=?24,disk_len=?25,disk_content_digest=?26,observed_digest=?27,observed_missing=?28,artifact_flags=?29,other_control_count=?30,ansi_count=?31,backspace_count=?32,unicode_format_count=?33,other_control_exact=?34 WHERE id=?1"
        } else {
            "INSERT INTO documents(id,buffer_id,name,path,path_key,search_text,is_dirty,text_is_empty,has_recorded_edits,is_settings_file,show_control_chars,right_to_left,encoding_name,has_bom,encoding_source,line_endings,preferred_line_ending,lf_count,crlf_count,cr_count,is_ascii_subset,has_decoding_warnings,line_count,disk_modified_millis,disk_len,disk_content_digest,observed_digest,observed_missing,artifact_flags,other_control_count,ansi_count,backspace_count,unicode_format_count,other_control_exact) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)"
        };
        self.execute_cached(
            sql,
            turso::params![
                row.id.as_str(),
                signed(row.buffer_id)?,
                row.name.as_str(),
                row.path.clone(),
                row.path_key.clone(),
                row.search_text.as_str(),
                i64::from(row.is_dirty),
                i64::from(row.text_is_empty),
                i64::from(row.has_recorded_edits),
                i64::from(row.is_settings_file),
                i64::from(row.show_control_chars),
                i64::from(row.right_to_left),
                row.encoding_name.as_str(),
                i64::from(row.has_bom),
                row.encoding_source.as_str(),
                row.line_endings.as_str(),
                row.preferred_line_ending.as_str(),
                signed(row.lf_count)?,
                signed(row.crlf_count)?,
                signed(row.cr_count)?,
                i64::from(row.is_ascii_subset),
                i64::from(row.has_decoding_warnings),
                signed(row.line_count)?,
                row.disk_modified_millis.map(signed).transpose()?,
                row.disk_len.map(signed).transpose()?,
                row.disk_content_digest.clone(),
                row.observed_digest.clone(),
                i64::from(row.observed_missing),
                artifact_flags(&row.analysis),
                signed(row.analysis.artifact_summary.other_control_count as u64)?,
                row.analysis
                    .artifact_summary
                    .ansi_sequence_count
                    .map(|n| signed(n as u64))
                    .transpose()?,
                row.analysis
                    .artifact_summary
                    .backspace_count
                    .map(|n| signed(n as u64))
                    .transpose()?,
                row.analysis
                    .artifact_summary
                    .unicode_format_control_count
                    .map(|n| signed(n as u64))
                    .transpose()?,
                row.analysis
                    .artifact_summary
                    .other_control_count_exact
                    .map(|n| signed(n as u64))
                    .transpose()?,
            ],
        )
        .await
    }
    pub(super) async fn read_document_row(&self, id: &str) -> io::Result<DocumentRow> {
        self.query_collect(
            &format!("SELECT {COLUMNS} FROM documents WHERE id=?"),
            [id],
            DocumentRow::read,
        )
        .await?
        .pop()
        .ok_or_else(|| invalid("workspace references a missing document"))
    }
}
pub(super) use super::rows::{boolean, signed, unsigned};
pub(super) fn enum_name(value: impl serde::Serialize) -> io::Result<String> {
    match serde_json::to_value(value).map_err(invalid)? {
        serde_json::Value::String(name) => Ok(name),
        _ => Err(invalid("metadata enum must have a string name")),
    }
}
pub(super) fn parse_enum<T: serde::de::DeserializeOwned>(name: String) -> io::Result<T> {
    serde_json::from_value(serde_json::Value::String(name)).map_err(invalid)
}

fn artifact_flags(metadata: &BufferTextMetadata) -> i64 {
    let a = &metadata.artifact_summary;
    i64::from(a.has_ansi_sequences)
        | (i64::from(a.has_carriage_returns) << 1)
        | (i64::from(a.has_backspaces) << 2)
        | (i64::from(a.has_unicode_format_controls) << 3)
        | (i64::from(metadata.has_non_compliant_characters) << 4)
}
fn read_analysis(row: &turso::Row) -> io::Result<BufferTextMetadata> {
    let flags = row.get::<i64>(28).map_err(error)?;
    if !(0..32).contains(&flags) {
        return Err(invalid("invalid artifact flags"));
    }
    let checked = MetadataRow(row);
    Ok(BufferTextMetadata {
        line_count: checked.index(22)?,
        preferred_line_ending: parse_enum(row.get(16).map_err(error)?)?,
        has_non_compliant_characters: flags & 16 != 0,
        artifact_summary: crate::app::domain::TextArtifactSummary {
            has_ansi_sequences: flags & 1 != 0,
            has_carriage_returns: flags & 2 != 0,
            has_backspaces: flags & 4 != 0,
            has_unicode_format_controls: flags & 8 != 0,
            other_control_count: checked.index(29)?,
            ansi_sequence_count: checked.optional_index(30)?,
            backspace_count: checked.optional_index(31)?,
            unicode_format_control_count: checked.optional_index(32)?,
            other_control_count_exact: checked.optional_index(33)?,
        },
    })
}
