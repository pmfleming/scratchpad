//! Storage addressing captured without flattening the visible document.
use super::{
    ADD_TAIL_BYTES, AddTextChunk, AddTextStorage, Arc, ByteSpan, ChunkText, FILE_CHUNK_BYTES,
    FileBackedChunk, FileBackedOriginal, OriginalTextStorage, PathBuf, PieceBuffer,
    PieceTreeStorage, io,
};
use crate::app::services::chunk_store::{Chunk, ChunkStoreHandle};

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
    pub(crate) fn span_char_len(&self, span: ByteSpan) -> io::Result<u64> {
        if !self.contains_span(span) {
            return Err(io::Error::other("history span exceeds storage"));
        }
        let start = span.start_byte as usize;
        let end = span.byte_end() as usize;
        if let (
            OriginalTextStorage::FileBacked(original) | OriginalTextStorage::Chunked(original),
            PieceBuffer::Original,
        ) = (&self.original, span.buffer)
        {
            let mut total = 0;
            for chunk in original
                .chunks
                .iter()
                .skip(
                    original
                        .chunks
                        .partition_point(|c| c.logical_start + c.byte_len <= start),
                )
                .take_while(|c| c.logical_start < end)
            {
                let lo = start.saturating_sub(chunk.logical_start);
                let hi = (end - chunk.logical_start).min(chunk.byte_len);
                total += if lo == 0 && hi == chunk.byte_len {
                    chunk.char_len as u64
                } else {
                    chunk
                        .text
                        .try_load_for_scan(false)?
                        .get(lo..hi)
                        .ok_or_else(|| io::Error::other("history span splits UTF-8"))?
                        .chars()
                        .count() as u64
                };
            }
            return Ok(total);
        }
        let mut count = 0;
        let mut visit = |base: usize, text: &ChunkText| -> io::Result<()> {
            let lo = start.saturating_sub(base);
            let hi = end.saturating_sub(base).min(text.len());
            if hi <= lo {
                return Ok(());
            }
            if lo == 0
                && hi == text.len()
                && let ChunkText::Stored(chunk) = text
                && let Some(reference) = chunk.reference()
            {
                count += reference.metadata.char_len;
                return Ok(());
            }
            count += text
                .try_load_for_scan(false)?
                .get(lo..hi)
                .ok_or_else(|| io::Error::other("history span splits UTF-8"))?
                .chars()
                .count() as u64;
            Ok(())
        };
        match (&self.original, span.buffer) {
            (OriginalTextStorage::Owned(text), PieceBuffer::Original) => {
                return Ok(text
                    .get(start..end)
                    .ok_or_else(|| io::Error::other("history span splits UTF-8"))?
                    .chars()
                    .count() as u64);
            }
            (
                OriginalTextStorage::FileBacked(original) | OriginalTextStorage::Chunked(original),
                PieceBuffer::Original,
            ) => {
                for chunk in &original.chunks {
                    if chunk.logical_start < end && chunk.logical_start + chunk.byte_len > start {
                        visit(chunk.logical_start, &chunk.text)?;
                    }
                }
            }
            (_, PieceBuffer::Add) => {
                for chunk in self.add.chunks.iter() {
                    if chunk.start_byte < end && chunk.start_byte + chunk.text.len() > start {
                        visit(chunk.start_byte, &chunk.text)?;
                    }
                }
            }
        }
        Ok(count)
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
        let known = self
            .durable_original
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&epoch)
            .cloned();
        let original = if let Some(known) = known {
            known
        } else {
            let mut chunks = Vec::new();
            match &self.original {
                OriginalTextStorage::Owned(text) => {
                    let mut start = 0;
                    while start < text.len() {
                        let mut end = (start + FILE_CHUNK_BYTES).min(text.len());
                        while !text.is_char_boundary(end) {
                            end -= 1;
                        }
                        if self.original_reachable.as_ref().is_none_or(|ranges| {
                            ranges.iter().any(|r| r.start < end && r.end > start)
                        }) {
                            chunks.push((
                                start,
                                store.seal(Arc::new(text[start..end].to_owned()), self.add.owner),
                            ));
                        }
                        start = end;
                    }
                }
                OriginalTextStorage::FileBacked(original)
                | OriginalTextStorage::Chunked(original) => {
                    for chunk in &original.chunks {
                        if matches!(chunk.text, ChunkText::Missing(_)) {
                            continue;
                        }
                        chunks.push((
                            chunk.logical_start,
                            match &chunk.text {
                                ChunkText::Stored(chunk) => chunk.clone(),
                                text => store.seal(text.try_load_for_scan(false)?, self.add.owner),
                            },
                        ));
                    }
                }
            }
            self.durable_original
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .insert(epoch, chunks.clone());
            chunks
        };
        let mut blocks = Vec::new();
        for (start, chunk) in original {
            blocks.push(StorageBlock {
                buffer: PieceBuffer::Original,
                start,
                chunk,
            });
        }
        let mut tail = None;
        for chunk in self.add.chunks.iter() {
            if matches!(chunk.text, ChunkText::Missing(_)) {
                continue;
            }
            if let ChunkText::Inline(text) = &chunk.text {
                tail = Some((chunk.start_byte, text.clone()));
            } else {
                let body = match &chunk.text {
                    ChunkText::Stored(body) => body.clone(),
                    text => store.seal(text.try_load_for_scan(false)?, self.add.owner),
                };
                blocks.push(StorageBlock {
                    buffer: PieceBuffer::Add,
                    start: chunk.start_byte,
                    chunk: body,
                });
            }
        }
        Ok(StorageCapture { blocks, tail })
    }

    pub(crate) fn restore_storage(
        store: ChunkStoreHandle,
        blocks: Vec<StorageBlock>,
        tail: Option<(usize, String)>,
        original_length: usize,
        add_length: usize,
    ) -> io::Result<Self> {
        let mut original = Vec::new();
        let mut add = AddTextStorage {
            store: Some(store.clone()),
            ..AddTextStorage::default()
        };
        let mut original_len = 0;
        for block in blocks {
            if block
                .chunk
                .reference()
                .is_none_or(|r| r.epoch.id != store.epoch().id)
            {
                return Err(io::Error::other("uncommitted or foreign stored block"));
            }
            match block.buffer {
                PieceBuffer::Original => {
                    if block.start > original_len {
                        original.push(FileBackedChunk {
                            logical_start: original_len,
                            text: ChunkText::Missing(block.start - original_len),
                            byte_len: block.start - original_len,
                            char_len: 0,
                        });
                        original_len = block.start;
                    }
                    if block.start != original_len {
                        return Err(io::Error::other("noncontiguous original mapping"));
                    }
                    let metadata = block
                        .chunk
                        .reference()
                        .ok_or_else(|| io::Error::other("missing chunk reference"))?
                        .metadata;
                    original.push(FileBackedChunk {
                        logical_start: block.start,
                        byte_len: metadata.byte_len,
                        char_len: usize::try_from(metadata.char_len).map_err(io::Error::other)?,
                        text: ChunkText::Stored(block.chunk),
                    });
                    original_len += metadata.byte_len;
                }
                PieceBuffer::Add => {
                    if block.start > add.len() {
                        add.chunks.push(AddTextChunk {
                            start_byte: add.len(),
                            text: ChunkText::Missing(block.start - add.len()),
                        });
                    }
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
        if original_len > original_length || add.len() > add_length {
            return Err(io::Error::other("stored mappings exceed address space"));
        }
        if original_len < original_length {
            original.push(FileBackedChunk {
                logical_start: original_len,
                text: ChunkText::Missing(original_length - original_len),
                byte_len: original_length - original_len,
                char_len: 0,
            });
        }
        Ok(Self {
            original: OriginalTextStorage::Chunked(Arc::new(FileBackedOriginal {
                path: PathBuf::new(),
                chunks: original,
                byte_len: original_length,
                fingerprint: 0,
            })),
            add,
            fault: Arc::default(),
            original_reachable: None,
            durable_original: Arc::default(),
        })
    }

    pub(crate) fn retain_reachable(&mut self, spans: &[ByteSpan]) {
        let used = |buffer, start: usize, len: usize| {
            spans.iter().any(|s| {
                s.buffer == buffer
                    && s.start_byte < (start + len) as u64
                    && s.byte_end() > start as u64
            })
        };
        for index in 0..self.add.chunks.len() {
            let chunk = &self.add.chunks[index];
            if !matches!(chunk.text, ChunkText::Inline(_))
                && !used(PieceBuffer::Add, chunk.start_byte, chunk.text.len())
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
            OriginalTextStorage::FileBacked(original) | OriginalTextStorage::Chunked(original) => {
                for chunk in &mut Arc::make_mut(original).chunks {
                    if !used(PieceBuffer::Original, chunk.logical_start, chunk.byte_len) {
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
        let missing = |base: usize, text: &ChunkText| {
            matches!(text, ChunkText::Missing(_))
                && (base as u64) < end
                && ((base + text.len()) as u64) > span.start_byte
        };
        match (&self.original, span.buffer) {
            (OriginalTextStorage::Owned(_), PieceBuffer::Original) => true,
            (
                OriginalTextStorage::FileBacked(original) | OriginalTextStorage::Chunked(original),
                PieceBuffer::Original,
            ) => {
                !original
                    .chunks
                    .iter()
                    .skip(original.chunks.partition_point(|c| {
                        (c.logical_start + c.byte_len) as u64 <= span.start_byte
                    }))
                    .take_while(|c| (c.logical_start as u64) < end)
                    .any(|c| missing(c.logical_start, &c.text))
            }
            (_, PieceBuffer::Add) => {
                !self
                    .add
                    .chunks
                    .iter()
                    .skip(self.add.chunks.partition_point(|c| {
                        (c.start_byte + c.text.len()) as u64 <= span.start_byte
                    }))
                    .take_while(|c| (c.start_byte as u64) < end)
                    .any(|c| missing(c.start_byte, &c.text))
            }
        }
    }
}

impl AddTextStorage {
    fn restore_tail(&mut self, tail: Option<(usize, String)>, length: usize) -> io::Result<()> {
        if let Some((start_byte, text)) = tail {
            if start_byte > self.len() {
                self.chunks.push(AddTextChunk {
                    start_byte: self.len(),
                    text: ChunkText::Missing(start_byte - self.len()),
                });
            }
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
        if self.len() < length {
            self.chunks.push(AddTextChunk {
                start_byte: self.len(),
                text: ChunkText::Missing(length - self.len()),
            });
        }
        Ok(())
    }
}
