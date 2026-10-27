//! FastCDC with unchanged-range reuse. Bytes are read only until a new CDC
//! boundary synchronizes with a previously committed, unchanged chunk boundary.
use crate::app::domain::{
    DocumentSnapshot,
    buffer::{ByteSlice, PieceTreeText},
};
use std::{
    cell::Cell,
    collections::HashMap,
    io::{self, Read},
    ops::Range,
    rc::Rc,
};

pub(super) const MAX_BYTES: usize = 256 * 1024;

#[derive(Clone, Copy, Debug)]
pub(super) struct ChunkMeta {
    pub len: usize,
    pub digest: blake3::Hash,
}
#[derive(Clone)]
pub(super) struct ChunkCheckpoint {
    pub token: u64,
    pub parts: Vec<ChunkMeta>,
}
pub(super) struct Chunk {
    pub meta: ChunkMeta,
    pub data: Option<Vec<u8>>,
}

pub(super) fn manifest_hasher() -> blake3::Hasher {
    blake3::Hasher::new_derive_key("Scratchpad recovery chunk manifest v1")
}
pub(super) fn hash_part(hasher: &mut blake3::Hasher, part: ChunkMeta) {
    hasher.update(&(part.len as u64).to_le_bytes());
    hasher.update(part.digest.as_bytes());
}

pub(super) struct IncrementalChunks<'a> {
    snapshot: &'a DocumentSnapshot,
    reusable: HashMap<usize, ChunkMeta>,
    position: usize,
    stream: Option<fastcdc::v2020::StreamCDC<SnapshotReader<'a>>>,
    read_bytes: Rc<Cell<u64>>,
}
impl<'a> IncrementalChunks<'a> {
    pub fn new(snapshot: &'a DocumentSnapshot, previous: Option<&ChunkCheckpoint>) -> Self {
        let mut reusable = HashMap::new();
        if !snapshot.piece_tree().is_file_backed()
            && let Some(previous) = previous
            && let Some(edits) = snapshot.piece_tree().save_changes_since(previous.token)
        {
            let mut position = 0;
            let mut ranges: Vec<_> = previous
                .parts
                .iter()
                .enumerate()
                .map(|(index, part)| {
                    let start = position;
                    position += part.len;
                    (start..position, *part, index + 1 == previous.parts.len())
                })
                .collect();
            for edit in edits {
                ranges.retain_mut(|(range, _, _)| {
                    if range.end <= edit.start {
                        return true;
                    }
                    if range.start >= edit.start + edit.removed {
                        range.start = range.start - edit.removed + edit.inserted;
                        range.end = range.end - edit.removed + edit.inserted;
                        return true;
                    }
                    false
                });
            }
            // The former EOF chunk may have ended before a natural CDC boundary.
            // It cannot be reused as an interior chunk after an append.
            reusable.extend(
                ranges
                    .into_iter()
                    .filter(|(range, _, terminal)| !terminal || range.end == snapshot.len_bytes())
                    .map(|(range, part, _)| (range.start, part)),
            );
        }
        Self {
            snapshot,
            reusable,
            position: 0,
            stream: None,
            read_bytes: Rc::new(Cell::new(0)),
        }
    }
    pub fn bytes_read(&self) -> u64 {
        self.read_bytes.get()
    }
}
impl Iterator for IncrementalChunks<'_> {
    type Item = io::Result<Chunk>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.position >= self.snapshot.len_bytes() {
            return None;
        }
        if let Some(meta) = self.reusable.get(&self.position).copied() {
            self.position += meta.len;
            self.stream = None;
            return Some(Ok(Chunk { meta, data: None }));
        }
        let stream = self.stream.get_or_insert_with(|| {
            fastcdc::v2020::StreamCDC::new(
                SnapshotReader {
                    spans: self
                        .snapshot
                        .piece_tree()
                        .byte_spans(self.position..self.snapshot.len_bytes()),
                    current: None,
                    read_bytes: self.read_bytes.clone(),
                },
                16 * 1024,
                64 * 1024,
                MAX_BYTES,
            )
        });
        match stream.next() {
            Some(Ok(chunk)) => {
                self.position += chunk.data.len();
                let meta = ChunkMeta {
                    len: chunk.data.len(),
                    digest: blake3::hash(&chunk.data),
                };
                Some(Ok(Chunk {
                    meta,
                    data: Some(chunk.data),
                }))
            }
            Some(Err(cause)) => Some(Err(io::Error::other(cause))),
            None => Some(Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "snapshot stream ended early",
            ))),
        }
    }
}

struct SnapshotReader<'a> {
    spans: ByteSlice<'a>,
    current: Option<(PieceTreeText<'a>, Range<usize>)>,
    read_bytes: Rc<Cell<u64>>,
}
impl Read for SnapshotReader<'_> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if out.is_empty() {
            return Ok(0);
        }
        loop {
            if let Some((text, range)) = &mut self.current {
                let len = range.len().min(out.len());
                if len > 0 {
                    out[..len].copy_from_slice(&text.as_bytes()[range.start..range.start + len]);
                    range.start += len;
                    self.read_bytes.set(self.read_bytes.get() + len as u64);
                    return Ok(len);
                }
            }
            self.current = self.spans.next();
            if self.current.is_none() {
                return Ok(0);
            }
        }
    }
}
