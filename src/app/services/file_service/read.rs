use super::{FileContent, FileVisibleWindow};
use crate::app::domain::buffer::{StreamedTextInspection, StreamingTextInspector};
use crate::app::domain::{EncodingSource, TextDocument};
use crate::app::services::chunk_store::ImportContext;
use chardetng::{EncodingDetector, Iso2022JpDetection, Utf8Detection};
use encoding_rs::Encoding;
use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

const CHUNKED_UTF8_MIN_BYTES: u64 = 16 * 1024 * 1024;

pub(super) struct PrefixInspection {
    pub(super) encoding: &'static Encoding,
    pub(super) has_bom: bool,
    pub(super) encoding_source: EncodingSource,
}

struct LoadedDocument {
    document: TextDocument,
    inspection: StreamedTextInspection,
    has_decoding_warnings: bool,
}
struct StagedDocument {
    content: String,
    inspector: StreamingTextInspector,
    has_decoding_warnings: bool,
}
impl StagedDocument {
    fn with_capacity(capacity: usize) -> Self {
        Self {
            content: String::with_capacity(capacity),
            inspector: StreamingTextInspector::new(),
            has_decoding_warnings: false,
        }
    }
    fn append(&mut self, text: &str, had_errors: bool) -> io::Result<()> {
        self.has_decoding_warnings |= had_errors;
        if text.contains('\0') {
            return Err(binary_file_error());
        }
        self.inspector.observe(text);
        self.content.push_str(text);
        Ok(())
    }
    fn finish(self) -> LoadedDocument {
        LoadedDocument {
            document: TextDocument::new(self.content),
            inspection: self.inspector.finish(),
            has_decoding_warnings: self.has_decoding_warnings,
        }
    }
}

pub(super) fn inspect_file_prefix(path: &Path) -> io::Result<PrefixInspection> {
    let mut file = File::open(path)?;
    let mut prefix = [0_u8; 4096];
    let prefix_len = file.read(&mut prefix)?;
    let prefix = &prefix[..prefix_len];

    let (encoding, has_bom, encoding_source) =
        if let Some((encoding, _)) = Encoding::for_bom(prefix) {
            (encoding, true, EncodingSource::Bom)
        } else {
            let mut detector = EncodingDetector::new(Iso2022JpDetection::Allow);
            detector.feed(prefix, prefix_len < 4096);
            (
                detector.guess(None, Utf8Detection::Allow),
                false,
                EncodingSource::Heuristic,
            )
        };

    ensure_text_prefix(prefix, has_bom)?;
    Ok(PrefixInspection {
        encoding,
        has_bom,
        encoding_source,
    })
}

pub(super) fn read_file_content(
    path: &Path,
    encoding: &'static Encoding,
    has_bom: bool,
    encoding_source: EncodingSource,
    context: &ImportContext,
) -> io::Result<FileContent> {
    let loaded = read_document_with_encoding(path, encoding, has_bom, context)?;
    let (format, text_metadata, digest) = loaded.inspection.into_metadata(
        encoding.name().to_owned(),
        has_bom,
        encoding_source,
        loaded.has_decoding_warnings,
    );
    let mut document = loaded.document;
    document.set_preferred_line_ending(text_metadata.preferred_line_ending);
    let metadata_exact = !loaded.has_decoding_warnings
        || encoding == encoding_rs::UTF_8
        || encoding == encoding_rs::UTF_16LE
        || encoding == encoding_rs::UTF_16BE;
    Ok(FileContent {
        document,
        artifact_summary: text_metadata.artifact_summary.clone(),
        format,
        text_metadata,
        decoded_digest: Some(digest),
        metadata_exact,
    })
}

pub(super) fn read_first_visible_window(
    path: &Path,
    encoding: &'static Encoding,
    has_bom: bool,
    max_bytes: usize,
) -> io::Result<FileVisibleWindow> {
    const UTF8_BOM: &[u8] = b"\xEF\xBB\xBF";

    let file_size_bytes = File::open(path)?.metadata()?.len();
    let file_size_for_platform = usize::try_from(file_size_bytes).unwrap_or(usize::MAX);
    let read_limit = max_bytes.max(1).min(file_size_for_platform);
    let mut bytes = Vec::with_capacity(read_limit);
    File::open(path)?
        .take(read_limit as u64)
        .read_to_end(&mut bytes)?;
    let loaded_bytes = bytes.len();
    let complete = loaded_bytes as u64 >= file_size_bytes;
    if bytes.contains(&0) {
        return Err(binary_file_error());
    }
    let without_bom = if has_bom && bytes.starts_with(UTF8_BOM) {
        &bytes[UTF8_BOM.len()..]
    } else {
        &bytes
    };

    let (text, has_decoding_warnings) = if encoding == encoding_rs::UTF_8 {
        match std::str::from_utf8(without_bom) {
            Ok(text) => (text.to_owned(), false),
            Err(error) if error.error_len().is_none() && !complete => (
                String::from_utf8_lossy(&without_bom[..error.valid_up_to()]).into_owned(),
                false,
            ),
            Err(_) => {
                let (decoded, had_errors) = encoding.decode_without_bom_handling(without_bom);
                (decoded.into_owned(), had_errors)
            }
        }
    } else {
        let (decoded, had_errors) = encoding.decode_without_bom_handling(without_bom);
        (decoded.into_owned(), had_errors)
    };

    Ok(FileVisibleWindow {
        text,
        file_size_bytes,
        loaded_bytes,
        encoding_name: encoding.name().to_owned(),
        has_bom,
        has_decoding_warnings,
        complete,
    })
}

fn ensure_text_prefix(prefix: &[u8], has_bom: bool) -> io::Result<()> {
    if is_probably_binary(prefix, has_bom) {
        return Err(binary_file_error());
    }
    Ok(())
}

fn binary_file_error() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "Binary files are not supported")
}

fn read_document_with_encoding(
    path: &Path,
    encoding: &'static Encoding,
    has_bom: bool,
    context: &ImportContext,
) -> io::Result<LoadedDocument> {
    const RAW_READ_BYTES: usize = 16 * 1024;
    const DECODED_CHUNK_BYTES: usize = 32 * 1024;

    if encoding == encoding_rs::UTF_8
        && File::open(path)?.metadata()?.len() >= CHUNKED_UTF8_MIN_BYTES
    {
        let (document, inspection) =
            TextDocument::import_utf8_file(path, if has_bom { 3 } else { 0 }, context)?;
        return Ok(LoadedDocument {
            document,
            inspection,
            has_decoding_warnings: false,
        });
    }

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
    let mut document = StagedDocument::with_capacity(decoded_capacity.min(16 * 1024 * 1024));
    let mut raw = [0u8; RAW_READ_BYTES];
    let mut pending = Vec::new();
    let mut decoded = [0u8; DECODED_CHUNK_BYTES];

    loop {
        context.check()?;
        let read = file.read(&mut raw)?;
        let eof = read == 0;
        pending.extend_from_slice(&raw[..read]);
        let consumed = decode_pending(&mut decoder, &pending, &mut decoded, eof, &mut document)?;
        pending.drain(..consumed);
        if document.content.len() >= CHUNKED_UTF8_MIN_BYTES as usize
            && let Some(store) = &context.store
        {
            store.check_bulk_capacity(document.content.len())?;
        }
        if eof {
            return Ok(document.finish());
        }
    }
}

fn decode_pending(
    decoder: &mut encoding_rs::Decoder,
    pending: &[u8],
    decoded: &mut [u8],
    eof: bool,
    document: &mut StagedDocument,
) -> io::Result<usize> {
    let mut consumed = 0;
    loop {
        let (result, read, written, had_errors) =
            decoder.decode_to_utf8(&pending[consumed..], decoded, eof);
        let text = std::str::from_utf8(&decoded[..written]).map_err(|error| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Decoded UTF-8 error: {error}"),
            )
        })?;
        document.append(text, had_errors)?;
        consumed += read;
        if result == encoding_rs::CoderResult::InputEmpty {
            return Ok(consumed);
        }
    }
}

fn read_utf8_document_fast_path(path: &Path, has_bom: bool) -> io::Result<Option<LoadedDocument>> {
    const UTF8_BOM: &[u8] = b"\xEF\xBB\xBF";

    // The small-file owned path avoids streaming overhead below the import threshold.
    let mut content = match std::fs::read_to_string(path) {
        Ok(content) => content,
        Err(error) if error.kind() == io::ErrorKind::InvalidData => return Ok(None),
        Err(error) => return Err(error),
    };
    if has_bom && content.as_bytes().starts_with(UTF8_BOM) {
        content.drain(..UTF8_BOM.len());
    }
    if content.as_bytes().contains(&0) {
        return Err(binary_file_error());
    }

    let mut inspector = StreamingTextInspector::new();
    inspector.observe(&content);
    Ok(Some(LoadedDocument {
        document: TextDocument::new(content),
        inspection: inspector.finish(),
        has_decoding_warnings: false,
    }))
}

fn is_probably_binary(prefix: &[u8], has_bom: bool) -> bool {
    if has_bom || prefix.is_empty() {
        return false;
    }

    prefix.contains(&0)
}
