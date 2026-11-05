use super::{
    ByteSpan, Piece, PieceBuffer, PieceTreeAnchorState, PieceTreeLite, PieceTreeRuntime,
    PieceTreeStorage, StorageBlock, StorageCapture, build_root_from_pieces, io,
};
use crate::app::services::chunk_store::ChunkStoreHandle;

impl PieceTreeLite {
    pub(crate) fn visit_history_span(
        &self,
        span: ByteSpan,
        visit: impl FnMut(&str) -> io::Result<()>,
    ) -> io::Result<()> {
        self.storage.visit_span(span, visit)
    }

    pub(crate) fn copy_to_store(
        &self,
        copy: &mut crate::app::services::chunk_store::Rebind,
    ) -> io::Result<Self> {
        let mut tree = self.clone_without_anchors();
        tree.storage = self.storage.copy_to_store(copy)?;
        Ok(tree)
    }

    pub(crate) fn reference_spans(
        &self,
        range: std::ops::Range<usize>,
    ) -> io::Result<Vec<ByteSpan>> {
        let mut position = 0;
        let mut spans = Vec::new();
        for piece in self
            .root
            .nodes
            .iter()
            .flat_map(|n| &n.leaves)
            .flat_map(|l| &l.pieces)
        {
            let end = position + piece.char_len;
            let lo = range.start.max(position);
            let hi = range.end.min(end);
            if lo < hi {
                let (start, bytes) = if lo == position && hi == end {
                    (piece.start_byte, piece.byte_len)
                } else {
                    let text = self.storage.try_text_for_span(
                        ByteSpan {
                            buffer: piece.buffer,
                            start_byte: piece.start_byte as u64,
                            byte_len: piece.byte_len as u64,
                            char_len: piece.char_len as u64,
                        },
                        false,
                    )?;
                    let boundary = |n| text.char_indices().nth(n).map_or(text.len(), |(b, _)| b);
                    let a = boundary(lo - position);
                    let b = boundary(hi - position);
                    (piece.start_byte + a, b - a)
                };
                spans.push(ByteSpan {
                    buffer: piece.buffer,
                    start_byte: start as u64,
                    byte_len: bytes as u64,
                    char_len: (hi - lo) as u64,
                });
            }
            position = end;
            if position >= range.end {
                break;
            }
        }
        Ok(spans)
    }

    pub(crate) fn display_line_count(&self) -> usize {
        if self.len_chars() == 0 {
            return 0;
        }
        let ends = self
            .root
            .nodes
            .iter()
            .rev()
            .flat_map(|n| n.leaves.iter().rev())
            .flat_map(|l| l.pieces.iter().rev())
            .next()
            .is_some_and(|p| p.ends_with_lf);
        self.metrics().newlines + usize::from(!ends)
    }
    pub(crate) fn span_char_len(&self, span: ByteSpan) -> io::Result<u64> {
        self.storage.span_char_len(span)
    }
    pub(crate) fn capture_storage(&self, store: &ChunkStoreHandle) -> io::Result<StorageCapture> {
        self.storage.capture_storage(store)
    }
    pub(crate) fn encode_pieces(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        let (original, add) = self.storage.address_lengths();
        for value in [1, original as u64, add as u64] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        for piece in self
            .root
            .nodes
            .iter()
            .flat_map(|n| &n.leaves)
            .flat_map(|l| &l.pieces)
        {
            for value in [
                u64::from(piece.buffer == PieceBuffer::Add),
                piece.start_byte as u64,
                piece.byte_len as u64,
                piece.char_len as u64,
                piece.newline_count as u64,
                u64::from(piece.is_ascii),
                u64::from(piece.ends_with_lf),
            ] {
                bytes.extend_from_slice(&value.to_le_bytes());
            }
        }
        bytes
    }
    pub(crate) fn restore_references(
        store: ChunkStoreHandle,
        blocks: Vec<StorageBlock>,
        tail: Option<(usize, String)>,
        bytes: &[u8],
    ) -> io::Result<Self> {
        if bytes.len() < 24 || !(bytes.len() - 24).is_multiple_of(56) {
            return Err(io::Error::other("invalid piece metadata length"));
        }
        let value = |offset| {
            usize::try_from(u64::from_le_bytes(
                bytes[offset..offset + 8]
                    .try_into()
                    .expect("eight byte header"),
            ))
            .map_err(io::Error::other)
        };
        if value(0)? != 1 {
            return Err(io::Error::other("invalid piece metadata version"));
        }
        let storage =
            PieceTreeStorage::restore_storage(store, blocks, tail, value(8)?, value(16)?)?;
        let bytes = &bytes[24..];
        let mut pieces = Vec::with_capacity(bytes.len() / 56);
        for record in bytes.chunks_exact(56) {
            let fields = record
                .chunks_exact(8)
                .map(|b| {
                    usize::try_from(u64::from_le_bytes(b.try_into().expect("eight byte field")))
                        .map_err(io::Error::other)
                })
                .collect::<io::Result<Vec<_>>>()?;
            let buffer = match fields[0] {
                0 => PieceBuffer::Original,
                1 => PieceBuffer::Add,
                _ => return Err(io::Error::other("invalid piece buffer")),
            };
            let piece = Piece {
                buffer,
                start_byte: fields[1],
                byte_len: fields[2],
                char_len: fields[3],
                newline_count: fields[4],
                is_ascii: fields[5] == 1,
                ends_with_lf: fields[6] == 1,
            };
            if fields[6] > 1
                || fields[5] > 1
                || piece.byte_len == 0
                || piece.char_len == 0
                || piece.char_len > piece.byte_len
                || piece.newline_count > piece.char_len
                || (piece.is_ascii && piece.char_len != piece.byte_len)
                || !storage.contains_span(ByteSpan {
                    buffer,
                    start_byte: piece.start_byte as u64,
                    byte_len: piece.byte_len as u64,
                    char_len: piece.char_len as u64,
                })
            {
                return Err(io::Error::other("invalid piece metrics or storage range"));
            }
            pieces.push(piece);
        }
        let mut tree = Self {
            storage,
            root: build_root_from_pieces(pieces),
            runtime: PieceTreeRuntime::default(),
            anchor_state: PieceTreeAnchorState::default(),
        };
        tree.assign_missing_leaf_ids();
        Ok(tree)
    }
    pub(crate) fn retain_reachable_storage(&mut self, spans: &[ByteSpan]) {
        self.storage.retain_reachable(spans);
    }
    pub(crate) fn contains_history_span(&self, span: ByteSpan) -> bool {
        self.storage.contains_span(span)
    }
}
