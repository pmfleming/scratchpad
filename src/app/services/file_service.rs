use crate::app::domain::{
    DiskFileState, EncodingSource, TextArtifactSummary, TextFormatMetadata,
};
use chardetng::EncodingDetector;
use encoding_rs::Encoding;
use encoding_rs_io::DecodeReaderBytesBuilder;
use std::fs::File;
use std::io::{self, Read};
use std::path::Path;
use std::time::UNIX_EPOCH;

pub struct FileService;

#[derive(Debug)]
pub struct FileContent {
    pub content: String,
    pub format: TextFormatMetadata,
    pub artifact_summary: TextArtifactSummary,
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

        let (encoding, has_bom, encoding_source) = if let Some((enc, _)) = Encoding::for_bom(prefix) {
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
        let format = TextFormatMetadata::detected(
            &content,
            encoding.name().to_string(),
            has_bom,
            encoding_source,
            has_decoding_warnings,
        );

        Ok(FileContent {
            artifact_summary: TextArtifactSummary::from_text_with_line_endings(
                &content,
                format.line_endings,
            ),
            content,
            format,
        })
    }

    pub fn write_file_with_format(
        path: &Path,
        content: &str,
        format: &TextFormatMetadata,
    ) -> io::Result<()> {
        let encoding = Encoding::for_label(format.encoding_name.as_bytes())
            .unwrap_or(encoding_rs::UTF_8);
        let bytes = encode_content(content, encoding, format.has_bom);
        std::fs::write(path, bytes)
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
}

fn encode_content(content: &str, encoding: &'static Encoding, has_bom: bool) -> Vec<u8> {
    if encoding == encoding_rs::UTF_16LE {
        return encode_utf16(content, has_bom, Endianness::Little);
    }

    if encoding == encoding_rs::UTF_16BE {
        return encode_utf16(content, has_bom, Endianness::Big);
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

fn encode_non_utf16(content: &str, encoding: &'static Encoding, has_bom: bool) -> Vec<u8> {
    let (bytes, _, _) = encoding.encode(content);
    let bytes = bytes.into_owned();

    if has_bom && encoding == encoding_rs::UTF_8 {
        prepend_bom(bytes, &[0xEF, 0xBB, 0xBF])
    } else {
        bytes
    }
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
