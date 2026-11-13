//! Append imported backing references without copying or re-reading their bodies.
use super::{
    AddTextChunk, AddTextStorage, ChunkText, OriginalTextStorage, Piece, PieceBuffer,
    PieceTreeStorage, io,
};
use crate::app::services::chunk_store::Utf8Chunks;
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
        match &other.original {
            OriginalTextStorage::Owned(text) => {
                let mut start = original_base;
                for body in Utf8Chunks::from_slice(text.as_bytes()) {
                    let body = body?;
                    let len = body.len();
                    self.add
                        .adopt_chunk(start, ChunkText::Memory(Arc::new(body)))?;
                    start += len;
                }
            }
            OriginalTextStorage::Chunked(original) => {
                for chunk in &original.chunks {
                    self.add
                        .adopt_chunk(original_base + chunk.logical_start, chunk.text.clone())?;
                }
            }
        }
        for chunk in other.add.chunks.iter() {
            self.add
                .adopt_chunk(add_base + chunk.start_byte, chunk.text.clone())?;
        }
        Ok(pieces
            .iter()
            .map(|piece| {
                let base = match piece.buffer {
                    PieceBuffer::Original => original_base,
                    PieceBuffer::Add => add_base,
                };
                Piece {
                    buffer: PieceBuffer::Add,
                    start_byte: piece.start_byte + base,
                    ..*piece
                }
            })
            .collect())
    }
}

impl AddTextStorage {
    /// Appends a chunk from another tree at `start`, re-owning stored handles
    /// and sealing in-memory bodies when this storage has a store.
    fn adopt_chunk(&mut self, start: usize, text: ChunkText) -> io::Result<()> {
        let text = match text {
            ChunkText::Stored(chunk) => ChunkText::Stored(chunk.for_owner(self.owner)),
            ChunkText::Inline(body) | ChunkText::Memory(body) => match &self.store {
                Some(store) => {
                    let _ = store.wait_for_capacity();
                    ChunkText::Stored(store.seal_bulk(body, self.owner)?)
                }
                None => ChunkText::Memory(body),
            },
            ChunkText::Missing(len) => ChunkText::Missing(len),
        };
        self.chunks.push(AddTextChunk {
            start_byte: start,
            text,
        });
        Ok(())
    }
}
