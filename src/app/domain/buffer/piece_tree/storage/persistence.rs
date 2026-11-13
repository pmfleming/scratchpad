//! Storage addressing captured without flattening the visible document.
use super::{
    ADD_TAIL_BYTES, AddTextChunk, AddTextStorage, Arc, ByteSpan, ChunkText, ChunkedOriginal,
    OriginalChunk, OriginalTextStorage, PathBuf, PieceBuffer, PieceTreeStorage, Range,
    StoredOriginalBlock, io,
};
use crate::app::services::chunk_store::{Chunk, ChunkStoreHandle, Utf8Chunks};

#[derive(Clone)]
pub(crate) struct StorageBlock {
    pub buffer: PieceBuffer,
    pub start: usize,
    pub chunk: Arc<Chunk>,
}
pub(crate) struct StorageCapture {
    pub blocks: Vec<StorageBlock>,
    pub tail: Option<(usize, Arc<String>)>,
}
impl PieceTreeStorage {
    pub(crate) fn copy_to_store(
        &self,
        copy: &mut crate::app::services::chunk_store::Rebind,
    ) -> io::Result<Self> {
        let mut storage = self.clone();
        let mut convert = |text: &mut ChunkText| -> io::Result<()> {
            match text {
                ChunkText::Stored(chunk) => *chunk = copy.copy(chunk)?,
                ChunkText::Memory(body) => {
                    copy.store.wait_for_capacity()?;
                    *text = ChunkText::Stored(copy.store.seal(body.clone(), self.add.owner));
                }
                ChunkText::Inline(_) | ChunkText::Missing(_) => {}
            }
            Ok(())
        };
        if let OriginalTextStorage::Chunked(original) = &mut storage.original {
            for chunk in &mut Arc::make_mut(original).chunks {
                convert(&mut chunk.text)?;
            }
        }
        for index in 0..storage.add.chunks.len() {
            convert(&mut storage.add.chunks[index].text)?;
        }
        storage.add.store = Some(copy.store.clone());
        storage.durable_original = Arc::default();
        Ok(storage)
    }

    pub(crate) fn span_char_len(&self, span: ByteSpan) -> io::Result<u64> {
        let mut chars = 0;
        self.visit_span_blocks(span, |block| {
            chars += block.char_len(span.buffer)?;
            Ok(())
        })?;
        Ok(chars)
    }
    /// Worker-only: owned originals are chunked once for durability. Mutable add
    /// tails remain tails; captures never seal them.
    pub(crate) fn capture_storage(&self, store: &ChunkStoreHandle) -> io::Result<StorageCapture> {
        let epoch = store.epoch().id;
        if let Some(bound) = self.store()
            && bound.epoch().id != epoch
        {
            return Err(io::Error::other("capture belongs to another store epoch"));
        }
        let mut blocks = Vec::new();
        for (start, chunk) in self.durable_original_blocks(store, epoch)? {
            blocks.push(StorageBlock {
                buffer: PieceBuffer::Original,
                start,
                chunk,
            });
        }
        let mut tail = None;
        for chunk in self.add.chunks.iter() {
            match &chunk.text {
                ChunkText::Missing(_) => {}
                ChunkText::Inline(text) => tail = Some((chunk.start_byte, text.clone())),
                text => blocks.push(StorageBlock {
                    buffer: PieceBuffer::Add,
                    start: chunk.start_byte,
                    chunk: self.durable_chunk(store, text)?,
                }),
            }
        }
        Ok(StorageCapture { blocks, tail })
    }

    /// Original text sealed into the epoch of `store`, chunked once per epoch
    /// and then reused from the durable-original cache.
    fn durable_original_blocks(
        &self,
        store: &ChunkStoreHandle,
        epoch: u64,
    ) -> io::Result<Vec<StoredOriginalBlock>> {
        let known = self
            .durable_original
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&epoch)
            .cloned();
        if let Some(known) = known {
            return Ok(known);
        }
        let chunks = match &self.original {
            OriginalTextStorage::Owned(text) => self.seal_reachable_original(text, store)?,
            OriginalTextStorage::Chunked(original) => {
                let mut chunks = Vec::new();
                for chunk in &original.chunks {
                    if !matches!(chunk.text, ChunkText::Missing(_)) {
                        chunks.push((chunk.logical_start, self.durable_chunk(store, &chunk.text)?));
                    }
                }
                chunks
            }
        };
        self.durable_original
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(epoch, chunks.clone());
        Ok(chunks)
    }

    /// Seals the UTF-8 chunks of an owned original that live pieces can still reach.
    fn seal_reachable_original(
        &self,
        text: &str,
        store: &ChunkStoreHandle,
    ) -> io::Result<Vec<StoredOriginalBlock>> {
        let mut chunks = Vec::new();
        let mut start = 0;
        for body in Utf8Chunks::from_slice(text.as_bytes()) {
            let body = body?;
            let end = start + body.len();
            let reachable = self
                .original_reachable
                .as_ref()
                .is_none_or(|ranges| ranges.iter().any(|r| r.start < end && r.end > start));
            if reachable {
                chunks.push((start, store.seal(Arc::new(body), self.add.owner)));
            }
            start = end;
        }
        Ok(chunks)
    }

    /// The stored handle for `text`, sealing an in-memory body into `store`.
    fn durable_chunk(&self, store: &ChunkStoreHandle, text: &ChunkText) -> io::Result<Arc<Chunk>> {
        Ok(match text {
            ChunkText::Stored(chunk) => chunk.clone(),
            text => store.seal(text.try_load_for_scan(false)?, self.add.owner),
        })
    }

    pub(crate) fn restore_storage(
        store: ChunkStoreHandle,
        blocks: Vec<StorageBlock>,
        tail: Option<(usize, String)>,
        original_length: usize,
        add_length: usize,
    ) -> io::Result<Self> {
        let epoch = store.epoch().id;
        let mut original = Vec::new();
        let mut add = AddTextStorage {
            store: Some(store),
            ..AddTextStorage::default()
        };
        for block in blocks {
            let metadata = block
                .chunk
                .reference()
                .filter(|reference| reference.epoch.id == epoch)
                .ok_or_else(|| io::Error::other("uncommitted or foreign stored block"))?
                .metadata;
            match block.buffer {
                PieceBuffer::Original => {
                    pad_original_to(&mut original, block.start);
                    if block.start != original_end(&original) {
                        return Err(io::Error::other("noncontiguous original mapping"));
                    }
                    original.push(OriginalChunk {
                        logical_start: block.start,
                        byte_len: metadata.byte_len,
                        char_len: usize::try_from(metadata.char_len).map_err(io::Error::other)?,
                        text: ChunkText::Stored(block.chunk),
                    });
                }
                PieceBuffer::Add => {
                    add.pad_to(block.start);
                    if block.start != add.len() {
                        return Err(io::Error::other("noncontiguous add mapping"));
                    }
                    add.chunks.push(AddTextChunk {
                        start_byte: block.start,
                        text: ChunkText::Stored(block.chunk),
                    });
                }
            }
        }
        add.restore_tail(tail, add_length)?;
        if original_end(&original) > original_length || add.len() > add_length {
            return Err(io::Error::other("stored mappings exceed address space"));
        }
        pad_original_to(&mut original, original_length);
        Ok(Self {
            original: OriginalTextStorage::Chunked(Arc::new(ChunkedOriginal {
                path: PathBuf::new(),
                chunks: original,
                byte_len: original_length,
            })),
            add,
            fault: Arc::default(),
            original_reachable: None,
            durable_original: Arc::default(),
        })
    }

    pub(crate) fn retain_reachable(&mut self, spans: &[ByteSpan]) {
        let add_ranges = merged_ranges(spans, PieceBuffer::Add);
        for index in 0..self.add.chunks.len() {
            let chunk = &self.add.chunks[index];
            if !matches!(chunk.text, ChunkText::Inline(_))
                && !overlaps_any(&add_ranges, chunk.start_byte, chunk.text.len())
            {
                let len = chunk.text.len();
                self.add.chunks[index].text = ChunkText::Missing(len);
            }
        }
        match &mut self.original {
            OriginalTextStorage::Owned(_) => {
                self.original_reachable = Some(Arc::new(
                    spans
                        .iter()
                        .filter(|s| s.buffer == PieceBuffer::Original)
                        .map(|s| s.start_byte as usize..s.byte_end() as usize)
                        .collect(),
                ));
            }
            OriginalTextStorage::Chunked(original) => {
                let original_ranges = merged_ranges(spans, PieceBuffer::Original);
                for chunk in &mut Arc::make_mut(original).chunks {
                    if !overlaps_any(&original_ranges, chunk.logical_start, chunk.byte_len) {
                        chunk.text = ChunkText::Missing(chunk.byte_len);
                    }
                }
            }
        }
        self.durable_original = Arc::default();
    }

    pub(crate) fn address_lengths(&self) -> (usize, usize) {
        (self.original_len(), self.add.len())
    }
    pub(crate) fn contains_span(&self, span: ByteSpan) -> bool {
        let Some(end) = span.start_byte.checked_add(span.byte_len) else {
            return false;
        };
        let limit = match span.buffer {
            PieceBuffer::Original => self.original_len(),
            PieceBuffer::Add => self.add.len(),
        } as u64;
        if end > limit || span.char_len > span.byte_len || (span.byte_len > 0 && span.char_len == 0)
        {
            return false;
        }
        if span.byte_len == 0 {
            return span.char_len == 0;
        }
        // Both bounds fit `usize`: `end` is within the storage length checked above.
        let (start, end) = (span.start_byte as usize, end as usize);
        let missing = |text: &ChunkText| matches!(text, ChunkText::Missing(_));
        match (&self.original, span.buffer) {
            (OriginalTextStorage::Owned(_), PieceBuffer::Original) => true,
            (OriginalTextStorage::Chunked(original), PieceBuffer::Original) => !original
                .chunks_overlapping(start, end)
                .any(|chunk| missing(&chunk.text)),
            (_, PieceBuffer::Add) => !self
                .add
                .chunks_overlapping(start, end)
                .any(|chunk| missing(&chunk.text)),
        }
    }
}

/// The `buffer` side of `spans` as sorted, non-overlapping byte ranges.
fn merged_ranges(spans: &[ByteSpan], buffer: PieceBuffer) -> Vec<Range<u64>> {
    let mut ranges: Vec<Range<u64>> = spans
        .iter()
        .filter(|span| span.buffer == buffer)
        .map(|span| span.start_byte..span.byte_end())
        .collect();
    ranges.sort_unstable_by_key(|range| range.start);
    let mut merged: Vec<Range<u64>> = Vec::new();
    for range in ranges {
        if let Some(last) = merged.last_mut()
            && range.start <= last.end
        {
            last.end = last.end.max(range.end);
        } else {
            merged.push(range);
        }
    }
    merged
}

/// Whether `start..start + len` intersects any of the sorted, merged `ranges`.
fn overlaps_any(ranges: &[Range<u64>], start: usize, len: usize) -> bool {
    let index = ranges.partition_point(|range| range.end <= start as u64);
    ranges
        .get(index)
        .is_some_and(|range| range.start < (start + len) as u64)
}

/// Logical end of a contiguous original mapping.
fn original_end(chunks: &[OriginalChunk]) -> usize {
    chunks
        .last()
        .map_or(0, |chunk| chunk.logical_start + chunk.byte_len)
}

/// Covers the gap before `offset` with a released placeholder chunk.
fn pad_original_to(chunks: &mut Vec<OriginalChunk>, offset: usize) {
    let end = original_end(chunks);
    if offset > end {
        chunks.push(OriginalChunk {
            logical_start: end,
            text: ChunkText::Missing(offset - end),
            byte_len: offset - end,
            char_len: 0,
        });
    }
}

impl AddTextStorage {
    /// Covers the gap before `offset` with a released placeholder chunk.
    fn pad_to(&mut self, offset: usize) {
        let end = self.len();
        if offset > end {
            self.chunks.push(AddTextChunk {
                start_byte: end,
                text: ChunkText::Missing(offset - end),
            });
        }
    }

    fn restore_tail(&mut self, tail: Option<(usize, String)>, length: usize) -> io::Result<()> {
        if let Some((start_byte, text)) = tail {
            self.pad_to(start_byte);
            if start_byte != self.len() || text.len() > ADD_TAIL_BYTES {
                return Err(io::Error::other("invalid stored tail"));
            }
            self.chunks.push(AddTextChunk {
                start_byte,
                text: ChunkText::Inline(Arc::new(text)),
            });
        }
        if self.len() > length {
            return Err(io::Error::other("stored tail exceeds address space"));
        }
        self.pad_to(length);
        Ok(())
    }
}

impl PieceTreeStorage {
    pub(crate) fn visit_span(
        &self,
        span: ByteSpan,
        mut visit: impl FnMut(&str) -> io::Result<()>,
    ) -> io::Result<()> {
        self.visit_span_blocks(span, |block| {
            let text = block.read()?;
            let mut remaining = text.as_ref();
            while !remaining.is_empty() {
                let cut = remaining.floor_char_boundary(remaining.len().min(ADD_TAIL_BYTES));
                visit(&remaining[..cut])?;
                remaining = &remaining[cut..];
            }
            Ok(())
        })
    }
}
