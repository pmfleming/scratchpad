use crate::app::domain::{DocumentSnapshot, LineEndingStyle, TextFormatMetadata};
use std::borrow::Cow;
use std::io::{self, Write};

use super::resolve_encoding;

pub(super) fn encode_content(
    content: &str,
    encoding: &'static encoding_rs::Encoding,
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

pub(super) fn serialized_content<'a>(
    content: &'a str,
    format: &TextFormatMetadata,
) -> io::Result<Cow<'a, str>> {
    if preserves_raw_line_endings(format) {
        return Ok(Cow::Borrowed(content));
    }

    let mut serialized = String::with_capacity(content.len());
    let mut serializer = NewlineSerializer::new(format.preferred_line_ending_style());
    serializer.write_span(content, &mut |span| {
        serialized.push_str(span);
        Ok(())
    })?;
    serializer.finish(&mut |span| {
        serialized.push_str(span);
        Ok(())
    })?;
    Ok(Cow::Owned(serialized))
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
    encoding: &'static encoding_rs::Encoding,
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

pub(super) fn write_snapshot_to_writer(
    writer: &mut dyn Write,
    snapshot: &DocumentSnapshot,
    format: &TextFormatMetadata,
) -> io::Result<()> {
    let encoding = resolve_encoding(&format.encoding_name)?;
    if encoding == encoding_rs::UTF_16LE {
        return write_snapshot_utf16_to_writer(
            writer,
            snapshot,
            format,
            format.has_bom,
            Endianness::Little,
        );
    }
    if encoding == encoding_rs::UTF_16BE {
        return write_snapshot_utf16_to_writer(
            writer,
            snapshot,
            format,
            format.has_bom,
            Endianness::Big,
        );
    }
    if encoding == encoding_rs::UTF_8 {
        return write_snapshot_utf8_to_writer_with_format(writer, snapshot, format);
    }

    write_snapshot_encoded_to_writer(writer, snapshot, format, encoding)
}

pub(super) fn write_snapshot_utf8_to_writer(
    writer: &mut dyn Write,
    snapshot: &DocumentSnapshot,
    has_bom: bool,
) -> io::Result<()> {
    if has_bom {
        writer.write_all(&[0xEF, 0xBB, 0xBF])?;
    }

    write_snapshot_spans(snapshot, |span| writer.write_all(span.as_bytes()))
}

fn write_snapshot_utf8_to_writer_with_format(
    writer: &mut dyn Write,
    snapshot: &DocumentSnapshot,
    format: &TextFormatMetadata,
) -> io::Result<()> {
    if format.has_bom {
        writer.write_all(&[0xEF, 0xBB, 0xBF])?;
    }

    write_serialized_snapshot_spans(snapshot, format, |span| writer.write_all(span.as_bytes()))
}

fn write_snapshot_spans(
    snapshot: &DocumentSnapshot,
    mut write_span: impl FnMut(&str) -> io::Result<()>,
) -> io::Result<()> {
    let tree = snapshot.piece_tree();
    for span in tree.spans_for_range(0..tree.len_chars()) {
        write_span(span.text)?;
    }
    Ok(())
}

fn write_snapshot_utf16_to_writer(
    writer: &mut dyn Write,
    snapshot: &DocumentSnapshot,
    format: &TextFormatMetadata,
    has_bom: bool,
    endianness: Endianness,
) -> io::Result<()> {
    if has_bom {
        writer.write_all(match endianness {
            Endianness::Little => &[0xFF, 0xFE],
            Endianness::Big => &[0xFE, 0xFF],
        })?;
    }

    write_serialized_snapshot_spans(snapshot, format, |span| {
        for unit in span.encode_utf16() {
            let bytes = match endianness {
                Endianness::Little => unit.to_le_bytes(),
                Endianness::Big => unit.to_be_bytes(),
            };
            writer.write_all(&bytes)?;
        }
        Ok(())
    })
}

fn write_serialized_snapshot_spans(
    snapshot: &DocumentSnapshot,
    format: &TextFormatMetadata,
    mut write_span: impl FnMut(&str) -> io::Result<()>,
) -> io::Result<()> {
    if preserves_raw_line_endings(format) {
        return write_snapshot_spans(snapshot, write_span);
    }

    let mut serializer = NewlineSerializer::new(format.preferred_line_ending_style());
    let tree = snapshot.piece_tree();
    for span in tree.spans_for_range(0..tree.len_chars()) {
        serializer.write_span(span.text, &mut write_span)?;
    }
    serializer.finish(&mut write_span)
}

fn preserves_raw_line_endings(format: &TextFormatMetadata) -> bool {
    matches!(format.line_endings, LineEndingStyle::Mixed)
}

struct NewlineSerializer {
    target: &'static str,
    pending_cr: bool,
}

impl NewlineSerializer {
    fn new(style: LineEndingStyle) -> Self {
        Self {
            target: style.as_str(),
            pending_cr: false,
        }
    }

    fn write_span(
        &mut self,
        text: &str,
        write_span: &mut impl FnMut(&str) -> io::Result<()>,
    ) -> io::Result<()> {
        let mut segment_start = 0usize;
        for (index, ch) in text.char_indices() {
            if self.pending_cr {
                self.pending_cr = false;
                if ch == '\n' {
                    write_span(self.target)?;
                    segment_start = index + ch.len_utf8();
                    continue;
                }
                write_span(self.target)?;
            }

            match ch {
                '\r' => {
                    if segment_start < index {
                        write_span(&text[segment_start..index])?;
                    }
                    self.pending_cr = true;
                    segment_start = index + ch.len_utf8();
                }
                '\n' => {
                    if segment_start < index {
                        write_span(&text[segment_start..index])?;
                    }
                    write_span(self.target)?;
                    segment_start = index + ch.len_utf8();
                }
                _ => {}
            }
        }

        if segment_start < text.len() {
            write_span(&text[segment_start..])?;
        }
        Ok(())
    }

    fn finish(&mut self, write_span: &mut impl FnMut(&str) -> io::Result<()>) -> io::Result<()> {
        if std::mem::take(&mut self.pending_cr) {
            write_span(self.target)?;
        }
        Ok(())
    }
}

fn write_snapshot_encoded_to_writer(
    writer: &mut dyn Write,
    snapshot: &DocumentSnapshot,
    format: &TextFormatMetadata,
    encoding: &'static encoding_rs::Encoding,
) -> io::Result<()> {
    let mut encoder = encoding.new_encoder();
    let mut dst = [0u8; 8192];

    write_serialized_snapshot_spans(snapshot, format, |span| {
        let mut src = span;
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
        Ok(())
    })?;

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
