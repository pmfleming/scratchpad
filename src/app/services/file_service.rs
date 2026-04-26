use crate::app::domain::buffer::{BufferTextMetadata, detected_text_format_and_metadata};
use crate::app::domain::{
    BufferState, DiskFileState, DocumentSnapshot, EncodingSource, TextArtifactSummary,
    TextFormatMetadata,
};
use crate::app::services::store_io::write_atomic_with;
use chardetng::EncodingDetector;
use encoding_rs::Encoding;
use encoding_rs_io::DecodeReaderBytesBuilder;
use std::fs::File;
use std::io::{self, Read, Write};
use std::path::Path;
use std::time::UNIX_EPOCH;

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

const LARGE_FILE_STAGED_METADATA_BYTES: usize = 5 * 1024 * 1024;
const STAGED_METADATA_SAMPLE_BYTES: usize = 64 * 1024;

pub struct FileService;

#[derive(Debug)]
pub struct FileContent {
    pub content: String,
    pub format: TextFormatMetadata,
    pub artifact_summary: TextArtifactSummary,
    pub(crate) text_metadata: BufferTextMetadata,
    pub(crate) metadata_complete: bool,
}

impl FileService {
    pub fn read_disk_state(path: &Path) -> io::Result<DiskFileState> {
        let metadata = std::fs::metadata(path)?;
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
        let mut file = File::open(path)?;
        let mut prefix = [0_u8; 4096];
        let prefix_len = file.read(&mut prefix)?;
        let prefix = &prefix[..prefix_len];

        let (encoding, has_bom, encoding_source) = if let Some((enc, _)) = Encoding::for_bom(prefix)
        {
            (enc, true, EncodingSource::Bom)
        } else {
            let mut detector = EncodingDetector::new();
            detector.feed(prefix, prefix_len < 4096);
            (detector.guess(None, true), false, EncodingSource::Heuristic)
        };

        if is_probably_binary(prefix, has_bom) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Binary files are not supported",
            ));
        }

        let file = File::open(path)?;
        let mut decoder = DecodeReaderBytesBuilder::new()
            .encoding(Some(encoding))
            .build(file);
        let mut content = String::new();
        decoder.read_to_string(&mut content)?;

        if content.contains('\0') {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Binary files are not supported",
            ));
        }

        let has_decoding_warnings = content.contains('\u{FFFD}');
        Ok(build_file_content(
            content,
            encoding.name().to_string(),
            has_bom,
            encoding_source,
            has_decoding_warnings,
        ))
    }

    pub fn read_file_with_encoding(path: &Path, encoding_name: &str) -> io::Result<FileContent> {
        let mut file = File::open(path)?;
        let mut prefix = [0_u8; 4096];
        let prefix_len = file.read(&mut prefix)?;
        let prefix = &prefix[..prefix_len];
        let encoding = resolve_encoding(encoding_name)?;
        let has_bom = Encoding::for_bom(prefix).is_some();

        if is_probably_binary(prefix, has_bom) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Binary files are not supported",
            ));
        }

        let content = read_text_with_encoding(path, encoding)?;
        if content.contains('\0') {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Binary files are not supported",
            ));
        }

        let has_decoding_warnings = content.contains('\u{FFFD}');
        Ok(build_file_content(
            content,
            encoding.name().to_string(),
            has_bom,
            EncodingSource::ExplicitUserChoice,
            has_decoding_warnings,
        ))
    }

    pub fn canonical_encoding_name(encoding_name: &str) -> io::Result<String> {
        Ok(resolve_encoding(encoding_name)?.name().to_string())
    }

    pub fn build_buffer_from_file_content(
        path: &Path,
        file_content: FileContent,
        disk_state: Option<DiskFileState>,
    ) -> BufferState {
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string());
        let FileContent {
            content,
            format,
            text_metadata,
            metadata_complete,
            ..
        } = file_content;
        let mut buffer = if metadata_complete {
            BufferState::with_text_metadata(
                name,
                content,
                Some(path.to_path_buf()),
                format,
                text_metadata,
            )
        } else {
            BufferState::with_text_metadata_refresh_state(
                name,
                content,
                Some(path.to_path_buf()),
                format,
                text_metadata,
                true,
            )
        };
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
        let bytes = encode_content(content, encoding, format.has_bom)?;
        std::fs::write(path, bytes)
    }

    pub fn write_snapshot_with_format(
        path: &Path,
        snapshot: &DocumentSnapshot,
        format: &TextFormatMetadata,
    ) -> io::Result<()> {
        write_atomic_with(path, |file| {
            write_snapshot_to_writer(file, snapshot, format)
        })
    }

    pub fn write_snapshot_utf8(path: &Path, snapshot: &DocumentSnapshot) -> io::Result<()> {
        write_atomic_with(path, |file| {
            write_snapshot_utf8_to_writer(file, snapshot, false)
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
        std::fs::rename(from, to)
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

fn read_text_with_encoding(path: &Path, encoding: &'static Encoding) -> io::Result<String> {
    let file = File::open(path)?;
    let mut decoder = DecodeReaderBytesBuilder::new()
        .encoding(Some(encoding))
        .build(file);
    let mut content = String::new();
    decoder.read_to_string(&mut content)?;
    Ok(content)
}

fn build_file_content(
    content: String,
    encoding_name: String,
    has_bom: bool,
    encoding_source: EncodingSource,
    has_decoding_warnings: bool,
) -> FileContent {
    if content.len() < LARGE_FILE_STAGED_METADATA_BYTES {
        let (format, text_metadata) = detected_text_format_and_metadata(
            &content,
            encoding_name,
            has_bom,
            encoding_source,
            has_decoding_warnings,
        );
        let artifact_summary = text_metadata.artifact_summary.clone();
        return FileContent {
            content,
            format,
            artifact_summary,
            text_metadata,
            metadata_complete: true,
        };
    }

    let sample = staged_metadata_sample(&content);
    let (mut format, sample_metadata) = detected_text_format_and_metadata(
        sample,
        encoding_name,
        has_bom,
        encoding_source,
        has_decoding_warnings,
    );
    format.is_ascii_subset = false;
    let text_metadata = BufferTextMetadata {
        line_count: staged_display_line_count(&content),
        artifact_summary: sample_metadata.artifact_summary.clone(),
        preferred_line_ending: format.preferred_line_ending_style(),
        has_non_compliant_characters: false,
    };
    let artifact_summary = text_metadata.artifact_summary.clone();
    FileContent {
        content,
        format,
        artifact_summary,
        text_metadata,
        metadata_complete: false,
    }
}

fn staged_metadata_sample(content: &str) -> &str {
    let mut end = STAGED_METADATA_SAMPLE_BYTES.min(content.len());
    while end > 0 && !content.is_char_boundary(end) {
        end -= 1;
    }
    &content[..end]
}

fn staged_display_line_count(content: &str) -> usize {
    let bytes = content.as_bytes();
    let mut lines = 1usize;
    let mut index = 0usize;

    while index < bytes.len() {
        match bytes[index] {
            b'\r' => {
                lines += 1;
                index += if bytes.get(index + 1) == Some(&b'\n') {
                    2
                } else {
                    1
                };
            }
            b'\n' => {
                lines += 1;
                index += 1;
            }
            _ => index += 1,
        }
    }

    lines
}

fn encode_content(
    content: &str,
    encoding: &'static Encoding,
    has_bom: bool,
) -> io::Result<Vec<u8>> {
    if encoding == encoding_rs::UTF_16LE {
        return Ok(encode_utf16(content, has_bom, Endianness::Little));
    }

    if encoding == encoding_rs::UTF_16BE {
        return Ok(encode_utf16(content, has_bom, Endianness::Big));
    }

    encode_non_utf16(content, encoding, has_bom)
}

enum Endianness {
    Little,
    Big,
}

fn encode_utf16(content: &str, has_bom: bool, endianness: Endianness) -> Vec<u8> {
    let utf16: Vec<u16> = content.encode_utf16().collect();
    let mut bytes = Vec::with_capacity((utf16.len() * 2) + if has_bom { 2 } else { 0 });

    if has_bom {
        bytes.extend_from_slice(match endianness {
            Endianness::Little => &[0xFF, 0xFE],
            Endianness::Big => &[0xFE, 0xFF],
        });
    }

    for unit in utf16 {
        let encoded_unit = match endianness {
            Endianness::Little => unit.to_le_bytes(),
            Endianness::Big => unit.to_be_bytes(),
        };
        bytes.extend_from_slice(&encoded_unit);
    }

    bytes
}

fn encode_non_utf16(
    content: &str,
    encoding: &'static Encoding,
    has_bom: bool,
) -> io::Result<Vec<u8>> {
    let (bytes, _, had_replacements) = encoding.encode(content);
    if had_replacements {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "Text contains characters not representable in {}",
                encoding.name()
            ),
        ));
    }
    let bytes = bytes.into_owned();

    if has_bom && encoding == encoding_rs::UTF_8 {
        Ok(prepend_bom(bytes, &[0xEF, 0xBB, 0xBF]))
    } else {
        Ok(bytes)
    }
}

fn write_snapshot_to_writer(
    writer: &mut dyn Write,
    snapshot: &DocumentSnapshot,
    format: &TextFormatMetadata,
) -> io::Result<()> {
    let encoding = resolve_encoding(&format.encoding_name)?;
    if encoding == encoding_rs::UTF_16LE {
        return write_snapshot_utf16_to_writer(
            writer,
            snapshot,
            format.has_bom,
            Endianness::Little,
        );
    }
    if encoding == encoding_rs::UTF_16BE {
        return write_snapshot_utf16_to_writer(writer, snapshot, format.has_bom, Endianness::Big);
    }
    if encoding == encoding_rs::UTF_8 {
        return write_snapshot_utf8_to_writer(writer, snapshot, format.has_bom);
    }

    write_snapshot_encoded_to_writer(writer, snapshot, encoding)
}

fn write_snapshot_utf8_to_writer(
    writer: &mut dyn Write,
    snapshot: &DocumentSnapshot,
    has_bom: bool,
) -> io::Result<()> {
    if has_bom {
        writer.write_all(&[0xEF, 0xBB, 0xBF])?;
    }

    let tree = snapshot.piece_tree();
    for span in tree.spans_for_range(0..tree.len_chars()) {
        writer.write_all(span.text.as_bytes())?;
    }
    Ok(())
}

fn write_snapshot_utf16_to_writer(
    writer: &mut dyn Write,
    snapshot: &DocumentSnapshot,
    has_bom: bool,
    endianness: Endianness,
) -> io::Result<()> {
    if has_bom {
        writer.write_all(match endianness {
            Endianness::Little => &[0xFF, 0xFE],
            Endianness::Big => &[0xFE, 0xFF],
        })?;
    }

    let tree = snapshot.piece_tree();
    for span in tree.spans_for_range(0..tree.len_chars()) {
        for unit in span.text.encode_utf16() {
            let bytes = match endianness {
                Endianness::Little => unit.to_le_bytes(),
                Endianness::Big => unit.to_be_bytes(),
            };
            writer.write_all(&bytes)?;
        }
    }

    Ok(())
}

fn write_snapshot_encoded_to_writer(
    writer: &mut dyn Write,
    snapshot: &DocumentSnapshot,
    encoding: &'static Encoding,
) -> io::Result<()> {
    let mut encoder = encoding.new_encoder();
    let mut dst = [0u8; 8192];
    let tree = snapshot.piece_tree();

    for span in tree.spans_for_range(0..tree.len_chars()) {
        let mut src = span.text;
        while !src.is_empty() {
            let (result, read, written, had_errors) =
                encoder.encode_from_utf8(src, &mut dst, false);
            if had_errors {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "Text contains characters not representable in {}",
                        encoding.name()
                    ),
                ));
            }
            writer.write_all(&dst[..written])?;
            src = &src[read..];
            if result == encoding_rs::CoderResult::InputEmpty {
                break;
            }
        }
    }

    loop {
        let (result, _read, written, had_errors) = encoder.encode_from_utf8("", &mut dst, true);
        if had_errors {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "Text contains characters not representable in {}",
                    encoding.name()
                ),
            ));
        }
        writer.write_all(&dst[..written])?;
        if result == encoding_rs::CoderResult::InputEmpty {
            break;
        }
    }

    Ok(())
}

fn prepend_bom(mut bytes: Vec<u8>, bom: &[u8]) -> Vec<u8> {
    let mut with_bom = Vec::with_capacity(bytes.len() + bom.len());
    with_bom.extend_from_slice(bom);
    with_bom.append(&mut bytes);
    with_bom
}

fn is_probably_binary(prefix: &[u8], has_bom: bool) -> bool {
    if has_bom || prefix.is_empty() {
        return false;
    }

    prefix.contains(&0)
}

#[cfg(test)]
mod tests {
    use super::{FileService, staged_display_line_count};
    use crate::app::domain::{
        EncodingSource, TextDocument, TextFormatMetadata, display_line_count,
    };

    #[test]
    fn writing_snapshot_uses_captured_revision_text() {
        let tempdir = tempfile::tempdir().expect("create tempdir");
        let path = tempdir.path().join("snapshot.txt");
        let mut document = TextDocument::new("before".to_owned());
        let snapshot = document.snapshot();

        document.insert_direct(6, " after");

        FileService::write_snapshot_with_format(
            &path,
            &snapshot,
            &crate::app::domain::TextFormatMetadata::utf8_for_new_file("before"),
        )
        .expect("write snapshot");

        assert_eq!(
            std::fs::read_to_string(path).expect("read written file"),
            "before"
        );
    }

    #[test]
    fn writing_fragmented_snapshot_with_utf16_preserves_content() {
        let tempdir = tempfile::tempdir().expect("create tempdir");
        let path = tempdir.path().join("snapshot-utf16.txt");
        let mut document = TextDocument::new("hello world".to_owned());
        document.insert_direct(5, " wide");
        let snapshot = document.snapshot();
        let format = TextFormatMetadata::detected(
            "hello wide world",
            "UTF-16LE".to_owned(),
            true,
            EncodingSource::ExplicitUserChoice,
            false,
        );

        FileService::write_snapshot_with_format(&path, &snapshot, &format)
            .expect("write fragmented snapshot");

        let reloaded = FileService::read_file(&path).expect("read UTF-16 snapshot");
        assert_eq!(reloaded.content, "hello wide world");
    }

    #[test]
    fn large_file_read_stages_metadata_refresh() {
        let tempdir = tempfile::tempdir().expect("create tempdir");
        let path = tempdir.path().join("large.txt");
        let content = "alpha\n".repeat((super::LARGE_FILE_STAGED_METADATA_BYTES / 6) + 1);
        std::fs::write(&path, &content).expect("write large file");

        let reloaded = FileService::read_file(&path).expect("read large file");

        assert!(!reloaded.metadata_complete);
        assert_eq!(
            reloaded.text_metadata.line_count,
            content.matches('\n').count() + 1
        );
    }

    #[test]
    fn staged_line_count_matches_display_line_count_for_cr_and_mixed_endings() {
        let content = "alpha\rbravo\r\ncharlie\ndelta\r";

        assert_eq!(
            staged_display_line_count(content),
            display_line_count(content)
        );
    }
}
