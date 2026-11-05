use super::{
    ByteSpan, Piece, PieceTreeInternalNode, PieceTreeLeaf, PieceTreeLite, PieceTreeSpan,
    PieceTreeText, byte_range_for_char_range,
};
use std::{io, ops::Range};

mod previews;
pub(super) use previews::{
    previews_for_matches_in_contiguous_text, previews_for_matches_in_piece_spans,
};

// Both byte and character spans walk the same pieces, without buffering text.
pub(super) struct Pieces<'a> {
    nodes: std::slice::Iter<'a, PieceTreeInternalNode>,
    leaves: std::slice::Iter<'a, PieceTreeLeaf>,
    pieces: std::slice::Iter<'a, Piece>,
}

impl<'a> Pieces<'a> {
    pub(super) fn new(tree: &'a PieceTreeLite, node: usize, leaf: usize) -> Self {
        let mut nodes = tree.root.nodes[node..].iter();
        let leaves = nodes.next().map_or(&[][..], |node| &node.leaves[leaf..]);
        Self {
            nodes,
            leaves: leaves.iter(),
            pieces: [].iter(),
        }
    }
}

impl<'a> Iterator for Pieces<'a> {
    type Item = &'a Piece;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(piece) = self.pieces.next() {
                return Some(piece);
            }
            if let Some(leaf) = self.leaves.next() {
                self.pieces = leaf.pieces.iter();
            } else {
                self.leaves = self.nodes.next()?.leaves.iter();
            }
        }
    }
}

pub struct PieceTreeSlice<'a> {
    tree: &'a PieceTreeLite,
    range_chars: Range<usize>,
    pieces: Pieces<'a>,
    current_char: usize,
}

impl PieceTreeLite {
    pub fn pin_lines(&self, lines: Range<usize>) -> io::Result<super::TextPin> {
        if self.len_chars() == 0 {
            return Ok(super::TextPin::default());
        }
        let first = lines.start.min(self.metrics().newlines);
        let last = lines.end.min(self.metrics().newlines);
        let address = self.find_leaf_for_line_index(first);
        let mut line = address.leaf_start_newline;
        let mut pin = super::TextPin::default();
        for piece in Pieces::new(self, address.node_index, address.leaf_index) {
            if line > last {
                break;
            }
            let end = line + piece.newline_count;
            // Newline-free pieces are skipped by line lookup using metrics.
            if piece.newline_count > 0 && end >= first {
                pin.extend(self.storage.pin_span(
                    ByteSpan {
                        buffer: piece.buffer,
                        start_byte: piece.start_byte as u64,
                        byte_len: piece.byte_len as u64,
                        char_len: piece.char_len as u64,
                    },
                    true,
                )?);
            }
            line = end;
        }
        Ok(pin)
    }
    pub fn try_line_info(&self, line: usize) -> io::Result<super::PieceTreeLineInfo> {
        let _pin = self.pin_lines(line..line)?;
        Ok(self.line_info(line))
    }
    pub fn try_line_index_at_offset(&self, offset: usize) -> io::Result<usize> {
        let offset = offset.min(self.len_chars());
        let _pin = self.pin_range(offset..offset)?;
        Ok(self.line_index_at_offset(offset))
    }
    #[must_use]
    pub fn spans_for_line(&self, target_line: usize) -> PieceTreeSlice<'_> {
        let line = self.line_info(target_line);
        self.spans_for_range(line.start_char..line.start_char + line.char_len)
    }

    #[must_use]
    pub fn spans_for_range(&self, range_chars: Range<usize>) -> PieceTreeSlice<'_> {
        let range_chars = self.normalize_char_range(range_chars);
        let address = self.find_leaf_for_char_offset(range_chars.start);
        PieceTreeSlice {
            tree: self,
            range_chars,
            pieces: Pieces::new(self, address.node_index, address.leaf_index),
            current_char: address.leaf_start_char,
        }
    }
}

impl<'a> PieceTreeSlice<'a> {
    fn next_with(
        &mut self,
        load: impl Fn(&'a PieceTreeLite, &'a Piece) -> io::Result<PieceTreeText<'a>>,
    ) -> Option<io::Result<PieceTreeSpan<'a>>> {
        while !self.range_chars.is_empty() && self.current_char < self.range_chars.end {
            let piece = self.pieces.next()?;
            let piece_start_char = self.current_char;
            let piece_end_char = piece_start_char + piece.char_len;
            self.current_char = piece_end_char;

            if piece_end_char <= self.range_chars.start {
                continue;
            }

            let local_start = self.range_chars.start.saturating_sub(piece_start_char);
            let local_end = (self.range_chars.end.min(piece_end_char)) - piece_start_char;
            let text = match load(self.tree, piece) {
                Ok(text) => text,
                Err(error) => return Some(Err(error)),
            };

            if local_start == 0 && local_end == piece.char_len {
                return Some(Ok(PieceTreeSpan {
                    text,
                    char_start: piece_start_char,
                    char_len: piece.char_len,
                    byte_span: ByteSpan {
                        buffer: piece.buffer,
                        start_byte: piece.start_byte as u64,
                        byte_len: piece.byte_len as u64,
                        char_len: piece.char_len as u64,
                    },
                }));
            }

            let byte_range = if piece.is_ascii {
                local_start..local_end
            } else {
                byte_range_for_char_range(&text, local_start, local_end)
            };
            let start_byte = piece.start_byte + byte_range.start;
            let byte_len = byte_range.len();
            return Some(Ok(PieceTreeSpan {
                text: text.slice(byte_range),
                char_start: piece_start_char + local_start,
                char_len: local_end - local_start,
                byte_span: ByteSpan {
                    buffer: piece.buffer,
                    start_byte: start_byte as u64,
                    byte_len: byte_len as u64,
                    char_len: (local_end - local_start) as u64,
                },
            }));
        }

        None
    }
}

impl<'a> Iterator for PieceTreeSlice<'a> {
    type Item = PieceTreeSpan<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        self.next_with(|tree, piece| Ok(tree.piece_text(piece)))
            .map(|result| result.expect("infallible pinned loader"))
    }
}

/// Loads and verifies at most one piece at a time. Background scans do not
/// populate the foreground cache. A failure ends the scan without emitting text.
pub struct TryPieceTreeSlice<'a> {
    inner: PieceTreeSlice<'a>,
    failed: bool,
    retain: bool,
}
impl<'a> Iterator for TryPieceTreeSlice<'a> {
    type Item = io::Result<PieceTreeSpan<'a>>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.failed {
            return None;
        }
        let retain = self.retain;
        let result = self.inner.next_with(|tree, piece| {
            tree.storage.try_text_for_span(
                ByteSpan {
                    buffer: piece.buffer,
                    start_byte: piece.start_byte as u64,
                    byte_len: piece.byte_len as u64,
                    char_len: piece.char_len as u64,
                },
                retain,
            )
        });
        if matches!(result, Some(Err(_))) {
            self.failed = true;
        }
        result
    }
}
impl PieceTreeLite {
    pub fn try_spans_for_range(&self, range: Range<usize>) -> TryPieceTreeSlice<'_> {
        TryPieceTreeSlice {
            inner: self.spans_for_range(range),
            failed: false,
            retain: false,
        }
    }

    pub fn pin_range(&self, range: Range<usize>) -> io::Result<super::TextPin> {
        if range.start > range.end || range.end > self.len_chars() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "pin range outside document",
            ));
        }
        let address = self.find_leaf_for_char_offset(range.start);
        let mut position = address.leaf_start_char;
        let mut pin = super::TextPin::default();
        for piece in Pieces::new(self, address.node_index, address.leaf_index) {
            let end = position + piece.char_len;
            if position > range.end {
                break;
            }
            if end >= range.start {
                pin.extend(self.storage.pin_span(
                    ByteSpan {
                        buffer: piece.buffer,
                        start_byte: piece.start_byte as u64,
                        byte_len: piece.byte_len as u64,
                        char_len: piece.char_len as u64,
                    },
                    true,
                )?);
            }
            position = end;
        }
        Ok(pin)
    }

    pub(crate) fn prefetch_range(&self, range: Range<usize>) {
        if range.start > range.end || range.end > self.len_chars() {
            return;
        }
        let address = self.find_leaf_for_char_offset(range.start);
        let mut position = address.leaf_start_char;
        for piece in Pieces::new(self, address.node_index, address.leaf_index) {
            if position > range.end {
                break;
            }
            if position + piece.char_len >= range.start {
                self.storage.prefetch_span(ByteSpan {
                    buffer: piece.buffer,
                    start_byte: piece.start_byte as u64,
                    byte_len: piece.byte_len as u64,
                    char_len: piece.char_len as u64,
                });
            }
            position += piece.char_len;
        }
    }

    pub fn read_fault(&self) -> Option<String> {
        self.storage.read_fault()
    }
    pub(crate) fn pin_history_spans(
        &self,
        spans: impl IntoIterator<Item = ByteSpan>,
    ) -> io::Result<super::TextPin> {
        let mut pin = super::TextPin::default();
        for span in spans {
            pin.extend(self.storage.pin_span(span, true)?);
        }
        Ok(pin)
    }
}
