use crate::app::diagnostics;
use crate::app::domain::buffer::{BufferTextMetadata, detected_text_format_and_metadata};
use crate::app::domain::{
    BufferState, DiskFileState, DocumentSnapshot, EncodingSource, TextArtifactSummary,
    TextDocument, TextFormatMetadata,
};
use crate::app::services::store_io::write_atomic_with;
use chardetng::EncodingDetector;
use encoding_rs::Encoding;
use std::fs::File;
use std::io::{self, Read};
use std::path::Path;
use std::time::UNIX_EPOCH;

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
        let prefix = inspect_file_prefix(path).inspect_err(|error| {
            diagnostics::record_io_error(
                "read_file",
                Some(path),
                "file_service::read_file",
                &error,
            );
        })?;
        read_file_content(
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

    pub fn read_file_with_encoding(path: &Path, encoding_name: &str) -> io::Result<FileContent> {
        let prefix = inspect_file_prefix(path).inspect_err(|error| {
            diagnostics::record_io_error_with_details(
                "read_file_with_encoding",
                Some(path),
                "file_service::read_file_with_encoding",
                &error,
                [("encoding", encoding_name.to_owned())],
            );
        })?;
        read_file_content(
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
        std::fs::write(path, bytes).inspect_err(|error| {
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
        write_atomic_with(path, |file| {
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
        write_atomic_with(path, |file| {
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

fn resolve_encoding(encoding_name: &str) -> io::Result<&'static Encoding> {
    Encoding::for_label(encoding_name.as_bytes()).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("Unsupported text encoding: {encoding_name}"),
        )
    })
}

struct LoadedDocument {
    document: TextDocument,
    sample: String,
    line_count: usize,
    has_decoding_warnings: bool,
}

struct PrefixInspection {
    encoding: &'static Encoding,
    has_bom: bool,
    encoding_source: EncodingSource,
}

fn inspect_file_prefix(path: &Path) -> io::Result<PrefixInspection> {
    let mut file = File::open(path)?;
    let mut prefix = [0_u8; 4096];
    let prefix_len = file.read(&mut prefix)?;
    let prefix = &prefix[..prefix_len];

    let (encoding, has_bom, encoding_source) =
        if let Some((encoding, _)) = Encoding::for_bom(prefix) {
            (encoding, true, EncodingSource::Bom)
        } else {
            let mut detector = EncodingDetector::new();
            detector.feed(prefix, prefix_len < prefix.len());
            (detector.guess(None, true), false, EncodingSource::Heuristic)
        };

    ensure_text_prefix(prefix, has_bom)?;
    Ok(PrefixInspection {
        encoding,
        has_bom,
        encoding_source,
    })
}

fn ensure_text_prefix(prefix: &[u8], has_bom: bool) -> io::Result<()> {
    if is_probably_binary(prefix, has_bom) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Binary files are not supported",
        ));
    }
    Ok(())
}

fn read_file_content(
    path: &Path,
    encoding: &'static Encoding,
    has_bom: bool,
    encoding_source: EncodingSource,
) -> io::Result<FileContent> {
    let loaded = read_document_with_encoding(path, encoding, has_bom)?;
    Ok(build_file_content(
        loaded.document,
        loaded.sample,
        loaded.line_count,
        loaded.has_decoding_warnings,
        encoding.name().to_string(),
        has_bom,
        encoding_source,
    ))
}

fn build_file_content(
    document: TextDocument,
    sample: String,
    line_count: usize,
    has_decoding_warnings: bool,
    encoding_name: String,
    has_bom: bool,
    encoding_source: EncodingSource,
) -> FileContent {
    let (mut format, sample_metadata) = detected_text_format_and_metadata(
        &sample,
        encoding_name,
        has_bom,
        encoding_source,
        has_decoding_warnings,
    );
    format.is_ascii_subset = false;
    let text_metadata = BufferTextMetadata {
        line_count,
        artifact_summary: sample_metadata.artifact_summary.clone(),
        preferred_line_ending: format.preferred_line_ending_style(),
        has_non_compliant_characters: false,
    };
    let artifact_summary = text_metadata.artifact_summary.clone();
    FileContent {
        document,
        format,
        artifact_summary,
        text_metadata,
    }
}

fn read_document_with_encoding(
    path: &Path,
    encoding: &'static Encoding,
    has_bom: bool,
) -> io::Result<LoadedDocument> {
    const RAW_READ_BYTES: usize = 16 * 1024;
    const DECODED_CHUNK_BYTES: usize = 32 * 1024;

    if encoding == encoding_rs::UTF_8
        && let Some(loaded) = read_utf8_document_fast_path(path, has_bom)?
    {
        return Ok(loaded);
    }

    let mut file = File::open(path)?;
    let decoded_capacity = file
        .metadata()
        .ok()
        .and_then(|metadata| usize::try_from(metadata.len()).ok())
        .unwrap_or(0);
    let mut decoder = if has_bom {
        encoding.new_decoder_with_bom_removal()
    } else {
        encoding.new_decoder_without_bom_handling()
    };
    let mut content = String::with_capacity(decoded_capacity);
    let mut sample = String::new();
    let mut line_count = 1usize;
    let mut line_count_pending_cr = false;
    let mut has_decoding_warnings = false;
    let mut raw = [0u8; RAW_READ_BYTES];
    let mut pending = Vec::new();
    let mut decoded = [0u8; DECODED_CHUNK_BYTES];

    loop {
        let read = file.read(&mut raw)?;
        let eof = read == 0;
        if read > 0 {
            pending.extend_from_slice(&raw[..read]);
        }

        let mut consumed = 0usize;
        loop {
            let input = &pending[consumed..];
            let (result, read, written, had_errors) =
                decoder.decode_to_utf8(input, &mut decoded, eof);
            has_decoding_warnings |= had_errors;
            let text = std::str::from_utf8(&decoded[..written]).map_err(|error| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("Decoded UTF-8 error: {error}"),
                )
            })?;
            if text.contains('\0') {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Binary files are not supported",
                ));
            }
            if !text.is_empty() {
                content.push_str(text);
                append_staged_metadata_sample(&mut sample, text);
                line_count =
                    accumulate_staged_line_count(text, line_count, &mut line_count_pending_cr);
            }
            consumed += read;

            if result == encoding_rs::CoderResult::InputEmpty {
                break;
            }
        }

        if consumed > 0 {
            pending.drain(..consumed);
        }

        if eof {
            break;
        }
    }

    Ok(LoadedDocument {
        document: TextDocument::new(content),
        sample,
        line_count,
        has_decoding_warnings,
    })
}

fn read_utf8_document_fast_path(path: &Path, has_bom: bool) -> io::Result<Option<LoadedDocument>> {
    const UTF8_BOM: &[u8] = b"\xEF\xBB\xBF";

    // Keep this as a single full-buffer UTF-8 path until we have a peak-memory
    // probe that proves a better tradeoff. A direct read_to_string avoids the
    // byte Vec, but measured slower on 2GB opens; a chunk-fed piece builder was
    // slower still because it lost the established parallel piece build.
    let bytes = std::fs::read(path)?;
    let bom_len = if has_bom && bytes.starts_with(UTF8_BOM) {
        UTF8_BOM.len()
    } else {
        0
    };
    if bytes[bom_len..].contains(&0) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Binary files are not supported",
        ));
    }

    let Ok(mut content) = String::from_utf8(bytes) else {
        return Ok(None);
    };
    if bom_len > 0 {
        content.drain(..bom_len);
    }

    let mut sample = String::new();
    append_staged_metadata_sample(&mut sample, &content);
    let mut line_count_pending_cr = false;
    let line_count = accumulate_staged_line_count(&content, 1, &mut line_count_pending_cr);

    Ok(Some(LoadedDocument {
        document: TextDocument::new(content),
        sample,
        line_count,
        has_decoding_warnings: false,
    }))
}

fn append_staged_metadata_sample(sample: &mut String, chunk: &str) {
    if sample.len() >= STAGED_METADATA_SAMPLE_BYTES {
        return;
    }

    let remaining = STAGED_METADATA_SAMPLE_BYTES - sample.len();
    if chunk.len() <= remaining {
        sample.push_str(chunk);
        return;
    }

    let mut end = remaining;
    while end > 0 && !chunk.is_char_boundary(end) {
        end -= 1;
    }
    sample.push_str(&chunk[..end]);
}

fn accumulate_staged_line_count(
    chunk: &str,
    mut line_count: usize,
    pending_cr: &mut bool,
) -> usize {
    for byte in chunk.bytes() {
        if *pending_cr {
            *pending_cr = false;
            if byte == b'\n' {
                continue;
            }
        }

        match byte {
            b'\r' => {
                line_count += 1;
                *pending_cr = true;
            }
            b'\n' => {
                line_count += 1;
            }
            _ => {}
        }
    }

    line_count
}

fn is_probably_binary(prefix: &[u8], has_bom: bool) -> bool {
    if has_bom || prefix.is_empty() {
        return false;
    }

    prefix.contains(&0)
}
