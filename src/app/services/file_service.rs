use crate::app::diagnostics;
use crate::app::domain::buffer::BufferTextMetadata;
use crate::app::domain::{
    BufferState, DiskFileState, DocumentSnapshot, EncodingSource, TextArtifactSummary,
    TextDocument, TextFormatMetadata,
};
use encoding_rs::Encoding;
use std::io;
use std::path::Path;
use std::time::UNIX_EPOCH;

mod document_write;
mod read;
#[cfg(test)]
mod tests;
mod write;

#[derive(Clone, Copy)]
pub struct EncodingOption {
    pub canonical_name: &'static str,
    pub label: &'static str,
}

pub const COMMON_TEXT_ENCODINGS: &[EncodingOption] = &[
    EncodingOption {
        canonical_name: "UTF-8",
        label: "UTF-8",
    },
    EncodingOption {
        canonical_name: "UTF-16LE",
        label: "UTF-16LE",
    },
    EncodingOption {
        canonical_name: "UTF-16BE",
        label: "UTF-16BE",
    },
    EncodingOption {
        canonical_name: "windows-1252",
        label: "Windows-1252 (ANSI)",
    },
    EncodingOption {
        canonical_name: "windows-1251",
        label: "Windows-1251",
    },
    EncodingOption {
        canonical_name: "windows-1250",
        label: "Windows-1250",
    },
    EncodingOption {
        canonical_name: "Shift_JIS",
        label: "Shift_JIS",
    },
    EncodingOption {
        canonical_name: "EUC-JP",
        label: "EUC-JP",
    },
    EncodingOption {
        canonical_name: "GBK",
        label: "GBK",
    },
    EncodingOption {
        canonical_name: "Big5",
        label: "Big5",
    },
    EncodingOption {
        canonical_name: "EUC-KR",
        label: "EUC-KR",
    },
];

const STAGED_METADATA_SAMPLE_BYTES: usize = 64 * 1024;

pub struct FileService;

pub struct FileContent {
    pub document: TextDocument,
    pub format: TextFormatMetadata,
    pub artifact_summary: TextArtifactSummary,
    pub(crate) text_metadata: BufferTextMetadata,
}

/// A bounded, decoded prefix used to make the first editor viewport available
/// without ingesting the complete file into the piece tree.
pub struct FileVisibleWindow {
    pub text: String,
    pub file_size_bytes: u64,
    pub loaded_bytes: usize,
    pub encoding_name: String,
    pub has_bom: bool,
    pub has_decoding_warnings: bool,
    pub complete: bool,
}

impl FileService {
    pub fn read_disk_state(path: &Path) -> io::Result<DiskFileState> {
        let metadata = std::fs::metadata(path).inspect_err(|error| {
            if error.kind() == io::ErrorKind::NotFound {
                return;
            }
            diagnostics::record_io_error(
                "read_disk_state",
                Some(path),
                "file_service::read_disk_state",
                &error,
            );
        })?;
        let modified_millis = metadata
            .modified()
            .ok()
            .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
            .map(|duration| duration.as_millis() as u64);

        Ok(DiskFileState {
            modified_millis,
            len: metadata.len(),
        })
    }

    pub fn read_file(path: &Path) -> io::Result<FileContent> {
        let prefix = read::inspect_file_prefix(path).inspect_err(|error| {
            diagnostics::record_io_error(
                "read_file",
                Some(path),
                "file_service::read_file",
                &error,
            );
        })?;
        read::read_file_content(
            path,
            prefix.encoding,
            prefix.has_bom,
            prefix.encoding_source,
        )
        .inspect_err(|error| {
            diagnostics::record_io_error(
                "read_file",
                Some(path),
                "file_service::read_file",
                &error,
            );
        })
    }

    /// Decode at most `max_bytes` from the start of a file. This is the staged
    /// open boundary: callers can paint a first viewport while full hydration
    /// continues on a background lane.
    pub fn read_first_visible_window(
        path: &Path,
        max_bytes: usize,
    ) -> io::Result<FileVisibleWindow> {
        let prefix = read::inspect_file_prefix(path)?;
        read::read_first_visible_window(path, prefix.encoding, prefix.has_bom, max_bytes)
    }

    pub fn read_file_with_encoding(path: &Path, encoding_name: &str) -> io::Result<FileContent> {
        let prefix = read::inspect_file_prefix(path).inspect_err(|error| {
            diagnostics::record_io_error_with_details(
                "read_file_with_encoding",
                Some(path),
                "file_service::read_file_with_encoding",
                &error,
                [("encoding", encoding_name.to_owned())],
            );
        })?;
        read::read_file_content(
            path,
            resolve_encoding(encoding_name)?,
            prefix.has_bom,
            EncodingSource::ExplicitUserChoice,
        )
        .inspect_err(|error| {
            diagnostics::record_io_error_with_details(
                "read_file_with_encoding",
                Some(path),
                "file_service::read_file_with_encoding",
                &error,
                [("encoding", encoding_name.to_owned())],
            );
        })
    }

    pub fn canonical_encoding_name(encoding_name: &str) -> io::Result<String> {
        Ok(resolve_encoding(encoding_name)?.name().to_string())
    }

    /// Build the metadata-only representation used for large batches before
    /// inactive files are hydrated.
    #[must_use]
    pub fn build_cold_file_shell(path: &Path, disk_state: Option<DiskFileState>) -> BufferState {
        let name = path.file_name().map_or_else(
            || path.display().to_string(),
            |name| name.to_string_lossy().into_owned(),
        );
        let mut buffer = BufferState::new(name, String::new(), Some(path.to_path_buf()));
        buffer.sync_to_disk_state(disk_state);
        buffer
    }

    #[must_use]
    pub fn build_buffer_from_file_content(
        path: &Path,
        file_content: FileContent,
        disk_state: Option<DiskFileState>,
    ) -> BufferState {
        let name = path.file_name().map_or_else(
            || path.display().to_string(),
            |name| name.to_string_lossy().into_owned(),
        );
        let FileContent {
            document,
            format,
            text_metadata,
            ..
        } = file_content;
        let mut buffer = BufferState::with_document_text_metadata_refresh_state(
            name,
            document,
            Some(path.to_path_buf()),
            format,
            text_metadata,
            true,
        );
        buffer.sync_to_disk_state(disk_state);
        buffer
    }

    pub fn encoding_supports_bom(encoding_name: &str) -> io::Result<bool> {
        let encoding = resolve_encoding(encoding_name)?;
        Ok(encoding == encoding_rs::UTF_8
            || encoding == encoding_rs::UTF_16LE
            || encoding == encoding_rs::UTF_16BE)
    }

    pub fn write_file_with_format(
        path: &Path,
        content: &str,
        format: &TextFormatMetadata,
    ) -> io::Result<()> {
        let encoding = resolve_encoding(&format.encoding_name)?;
        let content = write::serialized_content(content, format)?;
        let bytes = write::encode_content(&content, encoding, format.has_bom)?;
        write_document_bytes(path, &bytes).inspect_err(|error| {
            diagnostics::record_io_error_with_details(
                "write_file_with_format",
                Some(path),
                "file_service::write_file_with_format",
                &error,
                [("encoding", format.encoding_name.clone())],
            );
        })
    }

    pub fn write_snapshot_with_format(
        path: &Path,
        snapshot: &DocumentSnapshot,
        format: &TextFormatMetadata,
    ) -> io::Result<()> {
        document_write::write_document_with(path, |file| {
            write::write_snapshot_to_writer(file, snapshot, format)
        })
        .inspect_err(|error| {
            diagnostics::record_io_error_with_details(
                "write_snapshot_with_format",
                Some(path),
                "file_service::write_snapshot_with_format",
                &error,
                [("encoding", format.encoding_name.clone())],
            );
        })
    }

    pub fn write_snapshot_utf8(path: &Path, snapshot: &DocumentSnapshot) -> io::Result<()> {
        document_write::write_document_with(path, |file| {
            write::write_snapshot_utf8_to_writer(file, snapshot, false)
        })
        .inspect_err(|error| {
            diagnostics::record_io_error(
                "write_snapshot_utf8",
                Some(path),
                "file_service::write_snapshot_utf8",
                &error,
            );
        })
    }

    pub fn write_file_with_bom(
        path: &Path,
        content: &str,
        encoding_name: &str,
        has_bom: bool,
    ) -> io::Result<()> {
        let format = TextFormatMetadata::detected(
            content,
            encoding_name.to_owned(),
            has_bom,
            EncodingSource::ExplicitUserChoice,
            false,
        );
        Self::write_file_with_format(path, content, &format)
    }

    pub fn rename_path(from: &Path, to: &Path) -> io::Result<()> {
        std::fs::rename(from, to).inspect_err(|error| {
            diagnostics::record_io_error_with_details(
                "rename_path",
                Some(from),
                "file_service::rename_path",
                &error,
                [("target_path", to.display().to_string())],
            );
        })
    }
}

#[cfg(target_os = "linux")]
fn write_document_bytes(path: &Path, bytes: &[u8]) -> io::Result<()> {
    use std::io::Write;

    document_write::write_document_with(path, |file| file.write_all(bytes))
}

#[cfg(not(target_os = "linux"))]
fn write_document_bytes(path: &Path, bytes: &[u8]) -> io::Result<()> {
    std::fs::write(path, bytes)
}

fn resolve_encoding(encoding_name: &str) -> io::Result<&'static Encoding> {
    Encoding::for_label(encoding_name.as_bytes()).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("Unsupported text encoding: {encoding_name}"),
        )
    })
}
