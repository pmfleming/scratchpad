mod import;
mod persistence;
use super::text_cache::{self, ChunkText};
use super::{ByteSpan, Piece, PieceBuffer, PieceSource, PieceTreeText};
use crate::app::domain::buffer::PersistentVector;
pub(crate) use persistence::{StorageBlock, StorageCapture};
use std::io;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

const FILE_CHUNK_BYTES: usize = 256 * 1024;

type StoredOriginalBlock = (usize, Arc<crate::app::services::chunk_store::Chunk>);
type DurableOriginalCache = Arc<Mutex<std::collections::HashMap<u64, Vec<StoredOriginalBlock>>>>;

#[derive(Clone, Debug)]
pub(super) struct PieceTreeStorage {
    original: OriginalTextStorage,
    add: AddTextStorage,
    fault: Arc<Mutex<Option<String>>>,
    original_reachable: Option<Arc<Vec<std::ops::Range<usize>>>>,
    durable_original: DurableOriginalCache,
}

/// Retains verified bodies independently of cache eviction for one operation.
#[derive(Default)]
#[must_use]
pub struct TextPin {
    pub(super) bodies: Vec<Arc<String>>,
}
impl TextPin {
    pub(crate) fn extend(&mut self, other: Self) {
        self.bodies.extend(other.bodies);
    }
}

#[derive(Clone, Debug)]
enum OriginalTextStorage {
    Owned(Arc<str>),
    Chunked(Arc<ChunkedOriginal>),
}

#[derive(Clone, Debug)]
struct ChunkedOriginal {
    path: PathBuf,
    chunks: Vec<OriginalChunk>,
    byte_len: usize,
}

#[derive(Clone, Debug)]
struct OriginalChunk {
    logical_start: usize,
    text: ChunkText,
    byte_len: usize,
    char_len: usize,
}

/// Append-only text split by edit boundaries. Cloning a piece tree shares
/// completed chunks, so editing while a snapshot is alive no longer copies the
/// entire accumulated add buffer. Small consecutive edits may share a bounded
/// tail; copy-on-write then copies at most one tail chunk.
#[derive(Clone, Debug)]
struct AddTextStorage {
    owner: u64,
    chunks: PersistentVector<AddTextChunk>,
    store: Option<crate::app::services::chunk_store::ChunkStoreHandle>,
    bulk_intake: bool,
}

impl Default for AddTextStorage {
    fn default() -> Self {
        Self {
            owner: text_cache::new_owner(),
            chunks: PersistentVector::default(),
            store: None,
            bulk_intake: false,
        }
    }
}

#[derive(Clone, Debug)]
struct AddTextChunk {
    start_byte: usize,
    text: ChunkText,
}

const ADD_TAIL_BYTES: usize = 256 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct AddTextSpan {
    pub(super) start_byte: usize,
    pub(super) byte_len: usize,
    char_len: usize,
}

impl AddTextSpan {
    pub(super) fn byte_span(self) -> ByteSpan {
        let mut span = add_byte_span(self.start_byte, self.byte_len);
        span.char_len = self.char_len as u64;
        span
    }
}

impl PieceTreeStorage {
    pub(super) fn prefetch_span(&self, span: ByteSpan) {
        if span.buffer == PieceBuffer::Original {
            if let OriginalTextStorage::Chunked(original) = &self.original {
                let start = span.start_byte as usize;
                let end = start.saturating_add(span.byte_len as usize);
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
                    if let ChunkText::Stored(chunk) = &chunk.text {
                        chunk.prefetch();
                    }
                }
            }
            return;
        }
        let start = span.start_byte as usize;
        let end = start.saturating_add(span.byte_len as usize);
        for chunk in self
            .add
            .chunks
            .iter()
            .skip(
                self.add
                    .chunks
                    .partition_point(|c| c.start_byte + c.text.len() <= start),
            )
            .take_while(|c| c.start_byte < end)
        {
            if let ChunkText::Stored(chunk) = &chunk.text {
                chunk.prefetch();
            }
        }
    }
    pub(super) fn bind_store(
        &mut self,
        store: crate::app::services::chunk_store::ChunkStoreHandle,
    ) -> io::Result<()> {
        if let Some(previous) = &self.add.store {
            if previous.epoch().id == store.epoch().id {
                return Ok(());
            }
            let valid = |text: &ChunkText| match text {
                ChunkText::Stored(chunk) => chunk
                    .reference()
                    .is_some_and(|r| r.epoch.id == store.epoch().id),
                ChunkText::Inline(_) | ChunkText::Memory(_) | ChunkText::Missing(_) => true,
            };
            let originals = match &self.original {
                OriginalTextStorage::Owned(_) => true,
                OriginalTextStorage::Chunked(original) => {
                    original.chunks.iter().all(|c| valid(&c.text))
                }
            };
            if !originals || !self.add.chunks.iter().all(|c| valid(&c.text)) {
                return Err(io::Error::other(
                    "text belongs to another store epoch; explicit copy/rebind required",
                ));
            }
            let rebind = |text: &mut ChunkText| {
                if let ChunkText::Stored(chunk) = text {
                    *chunk = store.restored(
                        chunk.reference().expect("validated migrated reference"),
                        self.add.owner,
                    );
                }
            };
            if let OriginalTextStorage::Chunked(original) = &mut self.original {
                for chunk in &mut Arc::make_mut(original).chunks {
                    rebind(&mut chunk.text);
                }
            }
            for index in 0..self.add.chunks.len() {
                rebind(&mut self.add.chunks[index].text);
            }
            self.durable_original = Arc::default();
            self.add.store = None;
        }
        if let OriginalTextStorage::Chunked(original) = &mut self.original {
            for chunk in &mut Arc::make_mut(original).chunks {
                if self.add.bulk_intake {
                    let _ = store.wait_for_capacity();
                }
                chunk.text.bind_store(self.add.owner, &store);
            }
        }
        for index in 0..self.add.chunks.len() {
            if self.add.bulk_intake {
                let _ = store.wait_for_capacity();
            }
            self.add.chunks[index]
                .text
                .bind_store(self.add.owner, &store);
        }
        self.add.store = Some(store);
        Ok(())
    }
    pub(super) fn ram_tail_bytes(&self) -> usize {
        let original = match &self.original {
            OriginalTextStorage::Owned(text) if text.len() >= 16 * 1024 * 1024 => text.len(),
            _ => 0,
        };
        original.saturating_add(
            self.add
                .chunks
                .iter()
                .filter_map(|chunk| {
                    if let ChunkText::Inline(text) = &chunk.text {
                        Some(text.len())
                    } else {
                        None
                    }
                })
                .sum::<usize>(),
        )
    }
    pub(super) fn store(&self) -> Option<&crate::app::services::chunk_store::ChunkStoreHandle> {
        self.add.store.as_ref()
    }
    pub(super) fn set_bulk_intake(&mut self, enabled: bool) {
        self.add.bulk_intake = enabled;
    }
    #[cfg(test)]
    pub(super) fn inject_read_failure_for_test(&self) {
        *self.fault.lock().unwrap() = Some("injected unreadable text".into());
    }
    pub(super) fn from_original(text: String) -> Self {
        Self {
            original: OriginalTextStorage::Owned(Arc::from(text.into_boxed_str())),
            add: AddTextStorage::default(),
            fault: Arc::default(),
            original_reachable: None,
            durable_original: Arc::default(),
        }
    }

    pub(super) fn owned_original_text(&self) -> Option<&str> {
        match &self.original {
            OriginalTextStorage::Owned(text) => Some(text),
            OriginalTextStorage::Chunked(_) => None,
        }
    }

    pub(super) fn original_len(&self) -> usize {
        match &self.original {
            OriginalTextStorage::Owned(text) => text.len(),
            OriginalTextStorage::Chunked(original) => original.byte_len,
        }
    }

    pub(super) fn is_chunked(&self) -> bool {
        matches!(self.original, OriginalTextStorage::Chunked(_))
    }

    pub(super) fn loaded_chunk_count(&self) -> usize {
        if self.is_chunked() {
            self.add
                .store
                .as_ref()
                .map_or(0, |store| store.owner_loaded(self.add.owner))
        } else {
            0
        }
    }

    pub(super) fn chunk_cache_limit(&self) -> usize {
        if self.is_chunked() {
            self.add
                .store
                .as_ref()
                .map_or(0, |store| store.owner_cache_limit() / FILE_CHUNK_BYTES)
        } else {
            0
        }
    }

    #[cfg(test)]
    pub(super) fn shares_original_storage_with(&self, other: &Self) -> bool {
        match (&self.original, &other.original) {
            (OriginalTextStorage::Owned(left), OriginalTextStorage::Owned(right)) => {
                Arc::ptr_eq(left, right)
            }
            (OriginalTextStorage::Chunked(left), OriginalTextStorage::Chunked(right)) => {
                Arc::ptr_eq(left, right)
            }
            _ => false,
        }
    }

    pub(super) fn add_is_empty(&self) -> bool {
        self.add.is_empty()
    }

    pub(super) fn read_fault(&self) -> Option<String> {
        self.fault
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
            .or_else(|| {
                self.add
                    .store
                    .as_ref()
                    .and_then(|store| store.owner_fault(self.add.owner))
            })
    }

    pub(super) fn pin_span(&self, span: ByteSpan, retain: bool) -> io::Result<TextPin> {
        let result = if let Some(cause) = self.read_fault() {
            Err(io::Error::other(cause))
        } else {
            self.pin_span_inner(span, retain)
        };
        if let Err(error) = &result {
            let mut fault = self
                .fault
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if fault.is_none() {
                *fault = Some(error.to_string());
            }
        }
        result
    }

    fn pin_span_inner(&self, span: ByteSpan, retain: bool) -> io::Result<TextPin> {
        let start = usize::try_from(span.start_byte).map_err(io::Error::other)?;
        let len = usize::try_from(span.byte_len).map_err(io::Error::other)?;
        let end = start
            .checked_add(len)
            .ok_or_else(|| io::Error::other("text span overflow"))?;
        let invalid = || {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "text span exceeds storage or splits UTF-8",
            )
        };
        let mut pin = TextPin::default();
        match &self.original {
            OriginalTextStorage::Owned(text) if span.buffer == PieceBuffer::Original => {
                text.get(start..end).ok_or_else(invalid)?;
                return Ok(pin);
            }
            OriginalTextStorage::Chunked(original) if span.buffer == PieceBuffer::Original => {
                if end > original.byte_len {
                    return Err(invalid());
                }
                for chunk in original
                    .chunks
                    .iter()
                    .skip(
                        original
                            .chunks
                            .partition_point(|chunk| chunk.logical_start + chunk.byte_len <= start),
                    )
                    .take_while(|chunk| chunk.logical_start < end)
                {
                    let body = chunk.text.try_load_for_scan(retain).map_err(|error| {
                        io::Error::new(
                            error.kind(),
                            format!("{}: {error}", original.path.display()),
                        )
                    })?;
                    body.get(
                        start.saturating_sub(chunk.logical_start)
                            ..(end - chunk.logical_start).min(body.len()),
                    )
                    .ok_or_else(invalid)?;
                    pin.bodies.push(body);
                }
            }
            _ => {
                if end > self.add.len() {
                    return Err(invalid());
                }
                for chunk in self
                    .add
                    .chunks
                    .iter()
                    .skip(
                        self.add
                            .chunks
                            .partition_point(|chunk| chunk.start_byte + chunk.text.len() <= start),
                    )
                    .take_while(|chunk| chunk.start_byte < end)
                {
                    let body = chunk.text.try_load_for_scan(retain)?;
                    body.get(
                        start.saturating_sub(chunk.start_byte)
                            ..(end - chunk.start_byte).min(body.len()),
                    )
                    .ok_or_else(invalid)?;
                    pin.bodies.push(body);
                }
            }
        }
        Ok(pin)
    }

    pub(super) fn try_text_for_span(
        &self,
        span: ByteSpan,
        retain: bool,
    ) -> io::Result<PieceTreeText<'_>> {
        let pin = self.pin_span(span, retain)?;
        let start = span.start_byte as usize;
        let end = start + span.byte_len as usize;
        if start == end {
            return Ok(PieceTreeText::owned(String::new()));
        }
        let first = match (&self.original, span.buffer) {
            (OriginalTextStorage::Owned(text), PieceBuffer::Original) => {
                return Ok(PieceTreeText::borrowed(&text[start..end]));
            }
            (OriginalTextStorage::Chunked(original), PieceBuffer::Original) => {
                let index = original
                    .chunks
                    .partition_point(|c| c.logical_start + c.byte_len <= start);
                original.chunks[index].logical_start
            }
            (_, PieceBuffer::Add) => {
                let index = self
                    .add
                    .chunks
                    .partition_point(|c| c.start_byte + c.text.len() <= start);
                self.add.chunks[index].start_byte
            }
        };
        if pin.bodies.len() == 1 {
            return Ok(PieceTreeText::shared(
                pin.bodies[0].clone(),
                start - first..end - first,
            ));
        }
        let mut result = String::with_capacity(end - start);
        let mut position = first;
        for body in pin.bodies {
            result
                .push_str(&body[start.saturating_sub(position)..(end - position).min(body.len())]);
            position += body.len();
        }
        Ok(PieceTreeText::owned(result))
    }

    #[cfg(test)]
    pub(super) fn add_bytes(&self) -> usize {
        self.add.len()
    }

    pub(super) fn text_for_span(&self, span: ByteSpan) -> PieceTreeText<'_> {
        let start = usize::try_from(span.start_byte).expect("byte span start fits this platform");
        let byte_len = usize::try_from(span.byte_len).expect("byte span length fits this platform");
        match span.buffer {
            PieceBuffer::Original => self.original_text_for_range(start, byte_len),
            PieceBuffer::Add => self.add.text_for_range(start, byte_len),
        }
    }

    pub(super) fn piece_text(
        &self,
        buffer: PieceBuffer,
        start_byte: usize,
        byte_len: usize,
    ) -> PieceTreeText<'_> {
        match buffer {
            PieceBuffer::Original => self.original_text_for_range(start_byte, byte_len),
            PieceBuffer::Add => self.add.text_for_range(start_byte, byte_len),
        }
    }

    fn original_text_for_range(&self, start_byte: usize, byte_len: usize) -> PieceTreeText<'_> {
        match &self.original {
            OriginalTextStorage::Owned(text) => {
                PieceTreeText::borrowed(&text[start_byte..start_byte + byte_len])
            }
            OriginalTextStorage::Chunked(original) => original.text_for_range(start_byte, byte_len),
        }
    }

    pub(super) fn append_add_text(
        &mut self,
        text: &str,
        _source: PieceSource,
        _generation: u64,
    ) -> AddTextSpan {
        self.add.append(text)
    }
}

impl AddTextStorage {
    fn len(&self) -> usize {
        self.chunks
            .last()
            .map_or(0, |chunk| chunk.start_byte + chunk.text.len())
    }

    fn is_empty(&self) -> bool {
        self.chunks.is_empty()
    }

    fn append(&mut self, text: &str) -> AddTextSpan {
        let start_byte = self.len();
        let span = AddTextSpan {
            start_byte,
            byte_len: text.len(),
            char_len: text.chars().count(),
        };
        if text.is_empty() {
            return span;
        }

        let mut remaining = text;
        while !remaining.is_empty() {
            let capacity = self
                .chunks
                .last()
                .and_then(|tail| match &tail.text {
                    ChunkText::Inline(text) => Some(ADD_TAIL_BYTES.saturating_sub(text.len())),
                    ChunkText::Memory(_) | ChunkText::Stored(_) | ChunkText::Missing(_) => None,
                })
                .unwrap_or(0);
            let mut take = remaining.len().min(capacity);
            while take > 0 && !remaining.is_char_boundary(take) {
                take -= 1;
            }
            if take == 0 {
                if self.bulk_intake
                    && let Some(store) = &self.store
                {
                    // Healthy bulk work waits here only on its background lane.
                    // Accepted bodies stay in RAM after a write failure.
                    let _ = store.wait_for_capacity();
                }
                if let Some(tail) = self.chunks.last_mut() {
                    tail.text.seal(self.owner, self.store.as_ref());
                }
                let start_byte = self.len();
                self.chunks.push(AddTextChunk {
                    start_byte,
                    text: ChunkText::Inline(Arc::new(String::new())),
                });
                continue;
            }
            if let Some(chunk) = self.chunks.last_mut()
                && let ChunkText::Inline(tail) = &mut chunk.text
            {
                Arc::make_mut(tail).push_str(&remaining[..take]);
            }
            remaining = &remaining[take..];
        }
        span
    }

    fn text_for_range(&self, start_byte: usize, byte_len: usize) -> PieceTreeText<'static> {
        if byte_len == 0 {
            return PieceTreeText::owned(String::new());
        }
        let chunk_index = self
            .chunks
            .partition_point(|chunk| chunk.start_byte <= start_byte)
            .saturating_sub(1);
        let chunk = &self.chunks[chunk_index];
        let relative_start = start_byte.saturating_sub(chunk.start_byte);
        if relative_start + byte_len <= chunk.text.len() {
            return PieceTreeText::shared(
                chunk.text.load(),
                relative_start..relative_start + byte_len,
            );
        }

        let end_byte = start_byte + byte_len;
        let mut text = String::with_capacity(byte_len);
        for chunk in self.chunks.iter().skip(chunk_index) {
            if chunk.start_byte >= end_byte {
                break;
            }
            let local_start = start_byte.saturating_sub(chunk.start_byte);
            let local_end = end_byte
                .saturating_sub(chunk.start_byte)
                .min(chunk.text.len());
            if local_start < local_end {
                text.push_str(&chunk.text.load()[local_start..local_end]);
            }
        }
        PieceTreeText::owned(text)
    }
}

impl ChunkedOriginal {
    fn text_for_range(&self, start_byte: usize, byte_len: usize) -> PieceTreeText<'static> {
        let chunk_index = self
            .chunks
            .partition_point(|chunk| chunk.logical_start <= start_byte)
            .saturating_sub(1);
        let chunk = &self.chunks[chunk_index];
        let relative_start = start_byte.saturating_sub(chunk.logical_start);
        if relative_start + byte_len > chunk.byte_len {
            let end = start_byte + byte_len;
            let mut text = String::with_capacity(byte_len);
            for chunk in self
                .chunks
                .iter()
                .skip(chunk_index)
                .take_while(|c| c.logical_start < end)
            {
                let body = chunk.text.load();
                text.push_str(
                    &body[start_byte.saturating_sub(chunk.logical_start)
                        ..(end - chunk.logical_start).min(chunk.byte_len)],
                );
            }
            return PieceTreeText::owned(text);
        }
        let text = self.cached_chunk(chunk_index, chunk);
        PieceTreeText::shared(text, relative_start..relative_start + byte_len)
    }

    fn cached_chunk(&self, _chunk_index: usize, chunk: &OriginalChunk) -> Arc<String> {
        chunk.text.load()
    }
}

pub(super) fn add_byte_span(start_byte: usize, byte_len: usize) -> ByteSpan {
    ByteSpan {
        buffer: PieceBuffer::Add,
        start_byte: start_byte as u64,
        byte_len: byte_len as u64,
        char_len: byte_len as u64,
    }
}
