//! Reference equality handles ordinary replay; explicit imports use bounded verified comparison.
use super::{ByteSpan, PieceTreeLite, io};
use std::ops::Range;
pub(crate) fn coalesce(spans: impl IntoIterator<Item = ByteSpan>) -> Vec<ByteSpan> {
    let mut result: Vec<ByteSpan> = Vec::new();
    for span in spans {
        if span.byte_len == 0 {
            continue;
        }
        if let Some(previous) = result.last_mut()
            && previous.buffer == span.buffer
            && previous.byte_end() == span.start_byte
        {
            previous.byte_len += span.byte_len;
            previous.char_len += span.char_len;
        } else {
            result.push(span);
        }
    }
    result
}
impl PieceTreeLite {
    pub(crate) fn matches_history_range(
        &self,
        range: Range<usize>,
        expected: &[ByteSpan],
    ) -> io::Result<bool> {
        if coalesce(self.reference_spans(range.clone())?) == coalesce(expected.iter().copied()) {
            return Ok(true);
        }
        let mut actual_hash = blake3::Hasher::new();
        for span in self.try_spans_for_range(range) {
            actual_hash.update(span?.text.as_bytes());
        }
        let mut expected_hash = blake3::Hasher::new();
        for span in expected {
            self.visit_history_span(*span, |text| {
                expected_hash.update(text.as_bytes());
                Ok(())
            })?;
        }
        Ok(actual_hash.finalize() == expected_hash.finalize())
    }
}
