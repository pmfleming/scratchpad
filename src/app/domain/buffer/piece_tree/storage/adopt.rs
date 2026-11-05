//! Append imported backing references without copying or re-reading their bodies.
use super::{
    AddTextChunk, ChunkText, OriginalTextStorage, Piece, PieceBuffer, PieceTreeStorage, io,
};
use std::sync::Arc;
impl PieceTreeStorage {
    pub(crate) fn adopt(&mut self, other: &Self, pieces: &[Piece]) -> io::Result<Vec<Piece>> {
        if let (Some(left), Some(right)) = (self.store(), other.store())
            && left.epoch().id != right.epoch().id
        {
            return Err(io::Error::other(
                "adoption requires an explicit copy between store epochs",
            ));
        }
        if self.add.store.is_none() {
            self.add.store.clone_from(&other.add.store);
        }
        if let Some(index) = self.add.chunks.len().checked_sub(1) {
            self.add.chunks[index]
                .text
                .seal(self.add.owner, self.add.store.as_ref());
        }
        let original_base = self.add.len();
        let add_base = original_base
            .checked_add(other.original_len())
            .ok_or_else(|| io::Error::other("adoption address overflow"))?;
        let mut append = |start: usize, mut text: ChunkText| -> io::Result<()> {
            text = match text {
                ChunkText::Stored(chunk) => ChunkText::Stored(chunk.for_owner(self.add.owner)),
                ChunkText::Inline(body) | ChunkText::Memory(body) => {
                    if let Some(store) = &self.add.store {
                        let _ = store.wait_for_capacity();
                        ChunkText::Stored(store.seal_bulk(body, self.add.owner)?)
                    } else {
                        ChunkText::Memory(body)
                    }
                }
                ChunkText::Missing(len) => ChunkText::Missing(len),
            };
            self.add.chunks.push(AddTextChunk {
                start_byte: start,
                text,
            });
            Ok(())
        };
        match &other.original {
            OriginalTextStorage::Owned(text) => {
                let mut start = 0;
                for body in crate::app::services::chunk_store::Utf8Chunks::new(text.as_bytes()) {
                    let body = body?;
                    let len = body.len();
                    append(original_base + start, ChunkText::Memory(Arc::new(body)))?;
                    start += len;
                }
            }
            OriginalTextStorage::Chunked(original) => {
                for chunk in &original.chunks {
                    append(original_base + chunk.logical_start, chunk.text.clone())?;
                }
            }
        }
        for chunk in other.add.chunks.iter() {
            append(add_base + chunk.start_byte, chunk.text.clone())?;
        }
        Ok(pieces
            .iter()
            .map(|piece| Piece {
                buffer: PieceBuffer::Add,
                start_byte: piece.start_byte
                    + if piece.buffer == PieceBuffer::Original {
                        original_base
                    } else {
                        add_base
                    },
                ..*piece
            })
            .collect())
    }
}
