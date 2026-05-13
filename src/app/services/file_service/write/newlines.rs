use crate::app::domain::{LineEndingStyle, TextFormatMetadata};
use std::borrow::Cow;
use std::io;

pub(in crate::app::services::file_service) fn serialized_content<'a>(
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

pub(super) fn preserves_raw_line_endings(format: &TextFormatMetadata) -> bool {
    matches!(format.line_endings, LineEndingStyle::Mixed)
}

pub(super) struct NewlineSerializer {
    target: &'static str,
    pending_cr: bool,
}

impl NewlineSerializer {
    pub(super) fn new(style: LineEndingStyle) -> Self {
        Self {
            target: style.as_str(),
            pending_cr: false,
        }
    }

    pub(super) fn write_span(
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

    pub(super) fn finish(
        &mut self,
        write_span: &mut impl FnMut(&str) -> io::Result<()>,
    ) -> io::Result<()> {
        if std::mem::take(&mut self.pending_cr) {
            write_span(self.target)?;
        }
        Ok(())
    }
}
