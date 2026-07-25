use super::{FileContent, FileVisibleWindow, STAGED_METADATA_SAMPLE_BYTES};
use crate::app::domain::buffer::{BufferTextMetadata, detected_text_format_and_metadata};
use crate::app::domain::{EncodingSource, TextDocument};
use chardetng::{EncodingDetector, Iso2022JpDetection, Utf8Detection};
use encoding_rs::Encoding;
use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

const FILE_BACKED_UTF8_MIN_BYTES: u64 = 16 * 1024 * 1024;

pub(super) struct PrefixInspection {
    pub(super) encoding: &'static Encoding,
    pub(super) has_bom: bool,
    pub(super) encoding_source: EncodingSource,
}

struct LoadedDocument {
    document: TextDocument,
    sample: String,
    line_count: usize,
    has_decoding_warnings: bool,
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
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Binary files are not supported",
        ));
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
                std::str::from_utf8(&without_bom[..error.valid_up_to()])
                    .expect("UTF-8 valid prefix")
                    .to_owned(),
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
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Binary files are not supported",
        ));
    }
    Ok(())
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
        && File::open(path)?.metadata()?.len() >= FILE_BACKED_UTF8_MIN_BYTES
    {
        match read_utf8_document_file_backed(path, has_bom) {
            Ok(loaded) => return Ok(loaded),
            Err(error) if error.kind() == io::ErrorKind::InvalidData => {}
            Err(error) => return Err(error),
        }
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

fn read_utf8_document_file_backed(path: &Path, has_bom: bool) -> io::Result<LoadedDocument> {
    let file_offset = if has_bom { 3 } else { 0 };
    let (document, sample, line_count) =
        TextDocument::from_utf8_file(path, file_offset, STAGED_METADATA_SAMPLE_BYTES)?;
    Ok(LoadedDocument {
        document,
        sample,
        line_count,
        has_decoding_warnings: false,
    })
}

fn read_utf8_document_fast_path(path: &Path, has_bom: bool) -> io::Result<Option<LoadedDocument>> {
    const UTF8_BOM: &[u8] = b"\xEF\xBB\xBF";

    // Intentional capacity tradeoff: `read_to_string` can transiently keep more
    // memory live for huge UTF-8 files, but measured 2 GB opens are materially
    // faster than the lower-memory staged decode path. Do not slow this path
    // down just to reduce peak memory unless speed measurements stay healthy.
    let mut content = match std::fs::read_to_string(path) {
        Ok(content) => content,
        Err(error) if error.kind() == io::ErrorKind::InvalidData => return Ok(None),
        Err(error) => return Err(error),
    };
    if has_bom && content.as_bytes().starts_with(UTF8_BOM) {
        content.drain(..UTF8_BOM.len());
    }
    if content.as_bytes().contains(&0) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Binary files are not supported",
        ));
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
