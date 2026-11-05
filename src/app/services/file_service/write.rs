mod newlines;

use crate::app::domain::{DocumentSnapshot, TextFormatMetadata};
use encoding_rs::CoderResult;
use std::io::{self, Write};
use std::ops::Deref;

use super::resolve_encoding;
use newlines::{NewlineSerializer, preserves_raw_line_endings};

pub(super) fn write_content_to_writer(
    writer: &mut dyn Write,
    content: &str,
    format: &TextFormatMetadata,
) -> io::Result<()> {
    write_with_format(writer, std::iter::once(Ok(content)), format)
}

pub(super) fn write_snapshot_to_writer(
    writer: &mut dyn Write,
    snapshot: &DocumentSnapshot,
    format: &TextFormatMetadata,
) -> io::Result<()> {
    let tree = snapshot.piece_tree();
    write_with_format(
        writer,
        tree.try_spans_for_range(0..tree.len_chars())
            .map(|span| span.map(|span| span.text)),
        format,
    )
}

pub(super) fn write_snapshot_utf8_to_writer(
    writer: &mut dyn Write,
    snapshot: &DocumentSnapshot,
) -> io::Result<()> {
    let tree = snapshot.piece_tree();
    for span in tree.try_spans_for_range(0..tree.len_chars()) {
        writer.write_all(span?.text.as_bytes())?;
    }
    Ok(())
}

// Accept borrowed strings and piece-tree text without flattening the snapshot.
fn write_with_format<S: Deref<Target = str>>(
    writer: &mut dyn Write,
    spans: impl IntoIterator<Item = io::Result<S>>,
    format: &TextFormatMetadata,
) -> io::Result<()> {
    let encoding = resolve_encoding(&format.encoding_name)?;
    if format.has_bom {
        let bom: &[u8] = if encoding == encoding_rs::UTF_8 {
            &[0xEF, 0xBB, 0xBF]
        } else if encoding == encoding_rs::UTF_16LE {
            &[0xFF, 0xFE]
        } else if encoding == encoding_rs::UTF_16BE {
            &[0xFE, 0xFF]
        } else {
            &[]
        };
        writer.write_all(bom)?;
    }
    if encoding == encoding_rs::UTF_8 {
        return write_serialized_spans(spans, format, |span| writer.write_all(span.as_bytes()));
    }
    if encoding == encoding_rs::UTF_16LE || encoding == encoding_rs::UTF_16BE {
        let to_bytes: fn(u16) -> [u8; 2] = if encoding == encoding_rs::UTF_16LE {
            u16::to_le_bytes
        } else {
            u16::to_be_bytes
        };
        return write_serialized_spans(spans, format, |span| {
            for unit in span.encode_utf16() {
                writer.write_all(&to_bytes(unit))?;
            }
            Ok(())
        });
    }
    let mut encoder = encoding.new_encoder();
    let mut dst = [0u8; 8192];
    write_serialized_spans(spans, format, |span| {
        encode_span(writer, &mut encoder, &mut dst, span, false)
    })?;
    // Stateful encodings must emit their closing shift sequence even when the
    // final content span is empty or ends in a normalized newline.
    encode_span(writer, &mut encoder, &mut dst, "", true)
}

fn write_serialized_spans<S: Deref<Target = str>>(
    spans: impl IntoIterator<Item = io::Result<S>>,
    format: &TextFormatMetadata,
    mut write_span: impl FnMut(&str) -> io::Result<()>,
) -> io::Result<()> {
    let mut serializer = NewlineSerializer::new(format.preferred_line_ending_style());
    for span in spans {
        let span = span?;
        if preserves_raw_line_endings(format) {
            write_span(&span)?;
        } else {
            serializer.write_span(&span, &mut write_span)?;
        }
    }
    serializer.finish(&mut write_span)
}

fn encode_span(
    writer: &mut dyn Write,
    encoder: &mut encoding_rs::Encoder,
    dst: &mut [u8],
    mut source: &str,
    last: bool,
) -> io::Result<()> {
    loop {
        let (result, read, written, had_errors) = encoder.encode_from_utf8(source, dst, last);
        if had_errors {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "Text contains characters not representable in {}",
                    encoder.encoding().name()
                ),
            ));
        }
        writer.write_all(&dst[..written])?;
        source = &source[read..];
        if result == CoderResult::InputEmpty {
            return Ok(());
        }
    }
}
