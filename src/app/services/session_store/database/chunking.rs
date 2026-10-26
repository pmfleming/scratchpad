//! Feed FastCDC the byte stream, not piece or character boundaries. UTF-8 may
//! cross a chunk boundary; decoding happens only after complete reconstruction.
use crate::app::domain::DocumentSnapshot;
use crate::app::domain::buffer::{PieceTreeSlice, PieceTreeText};
use std::io::{self, Read};

pub(super) const MAX_BYTES: usize = 256 * 1024;
pub(super) fn chunks(snapshot: &DocumentSnapshot) -> fastcdc::v2020::StreamCDC<SnapshotReader<'_>> {
    fastcdc::v2020::StreamCDC::new(
        SnapshotReader {
            spans: snapshot.spans_for_range(0..snapshot.len_chars()),
            current: None,
            offset: 0,
        },
        16 * 1024,
        64 * 1024,
        MAX_BYTES,
    )
}

pub(super) struct SnapshotReader<'a> {
    spans: PieceTreeSlice<'a>,
    current: Option<PieceTreeText<'a>>,
    offset: usize,
}
impl Read for SnapshotReader<'_> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if out.is_empty() {
            return Ok(0);
        }
        loop {
            if let Some(text) = &self.current {
                let remaining = &text.as_bytes()[self.offset..];
                let len = remaining.len().min(out.len());
                if len > 0 {
                    out[..len].copy_from_slice(&remaining[..len]);
                    self.offset += len;
                    return Ok(len);
                }
            }
            self.current = self.spans.next().map(|span| span.text);
            self.offset = 0;
            if self.current.is_none() {
                return Ok(0);
            }
        }
    }
}
