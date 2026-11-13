mod adopt;
mod import;
mod metrics;
mod persistence;
mod ranges;
use super::text_cache::{self, ChunkText};
use super::{ByteSpan, Piece, PieceBuffer, PieceSource, PieceTreeText};
use crate::app::domain::buffer::PersistentVector;
use crate::app::services::chunk_store::ChunkStoreHandle;
pub(crate) use persistence::{StorageBlock, StorageCapture};
use std::io;
use std::ops::Range;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

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
    /// Checks that `range` cuts `body` (stored at logical offset `base`) on
    /// UTF-8 boundaries, then keeps the body when its residency needs a pin.
    fn hold(
        &mut self,
        text: &ChunkText,
        body: Arc<String>,
        base: usize,
        range: Range<usize>,
        include_owned: bool,
    ) -> io::Result<()> {
        body.get(range.start.saturating_sub(base)..(range.end - base).min(body.len()))
            .ok_or_else(invalid_span)?;
        // Owned RAM remains alive through the tree. Retaining its Arc here
        // would force a full tail copy during the following edit. Stored
        // bodies need a pin because their resident handle is weak.
        if include_owned || matches!(text, ChunkText::Stored(_)) {
            self.bodies.push(body);
        }
        Ok(())
    }
}

fn checked_span_bounds(span: ByteSpan) -> io::Result<(usize, usize)> {
    let start = usize::try_from(span.start_byte).map_err(io::Error::other)?;
    let len = usize::try_from(span.byte_len).map_err(io::Error::other)?;
    let end = start
        .checked_add(len)
        .ok_or_else(|| io::Error::other("text span overflow"))?;
    Ok((start, end))
}

fn invalid_span() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "text span exceeds storage or splits UTF-8",
    )
}

/// Whether `text` can move to a store of `epoch` without copying its body.
fn stored_in_epoch(text: &ChunkText, epoch: u64) -> bool {
    match text {
        ChunkText::Stored(chunk) => chunk.reference().is_some_and(|r| r.epoch.id == epoch),
        ChunkText::Inline(_) | ChunkText::Memory(_) | ChunkText::Missing(_) => true,
    }
}

#[derive(Clone, Debug)]
enum OriginalTextStorage {
    Owned(Arc<String>),
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
    store: Option<ChunkStoreHandle>,
    bulk_intake: bool,
    intake_error: Option<String>,
}

impl Default for AddTextStorage {
    fn default() -> Self {
        Self {
            owner: text_cache::new_owner(),
            chunks: PersistentVector::default(),
            store: None,
            bulk_intake: false,
            intake_error: None,
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
        let start = span.start_byte as usize;
        let end = start.saturating_add(span.byte_len as usize);
        let prefetch = |text: &ChunkText| {
            if let ChunkText::Stored(chunk) = text {
                chunk.prefetch();
            }
        };
        match (&self.original, span.buffer) {
            (OriginalTextStorage::Owned(_), PieceBuffer::Original) => {}
            (OriginalTextStorage::Chunked(original), PieceBuffer::Original) => original
                .chunks_overlapping(start, end)
                .for_each(|chunk| prefetch(&chunk.text)),
            (_, PieceBuffer::Add) => self
                .add
                .chunks_overlapping(start, end)
                .for_each(|chunk| prefetch(&chunk.text)),
        }
    }
    pub(super) fn bind_store(&mut self, store: ChunkStoreHandle) -> io::Result<()> {
        if let Some(previous) = &self.add.store {
            if previous.epoch().id == store.epoch().id {
                return Ok(());
            }
            self.rebind_stored_text(&store)?;
        }
        let owner = self.add.owner;
        let bulk_intake = self.add.bulk_intake;
        self.for_each_text_mut(|text| {
            if bulk_intake {
                let _ = store.wait_for_capacity();
            }
            text.bind_store(owner, &store);
        });
        self.add.store = Some(store);
        Ok(())
    }
    /// Moves stored references from the previous handle onto `store`. Only text
    /// already committed to the epoch of `store` can move without a copy.
    fn rebind_stored_text(&mut self, store: &ChunkStoreHandle) -> io::Result<()> {
        let epoch = store.epoch().id;
        if !self.chunk_texts().all(|text| stored_in_epoch(text, epoch)) {
            return Err(io::Error::other(
                "text belongs to another store epoch; explicit copy/rebind required",
            ));
        }
        let owner = self.add.owner;
        self.for_each_text_mut(|text| {
            if let ChunkText::Stored(chunk) = text {
                *chunk = store.restored(
                    chunk.reference().expect("validated migrated reference"),
                    owner,
                );
            }
        });
        self.durable_original = Arc::default();
        self.add.store = None;
        Ok(())
    }
    /// Every original and add chunk body, in address order.
    fn chunk_texts(&self) -> impl Iterator<Item = &ChunkText> {
        let original: &[OriginalChunk] = match &self.original {
            OriginalTextStorage::Owned(_) => &[],
            OriginalTextStorage::Chunked(original) => &original.chunks,
        };
        original
            .iter()
            .map(|chunk| &chunk.text)
            .chain(self.add.chunks.iter().map(|chunk| &chunk.text))
    }
    fn for_each_text_mut(&mut self, mut visit: impl FnMut(&mut ChunkText)) {
        let _ = self.try_for_each_text_mut(|text| {
            visit(text);
            Ok(())
        });
    }
    /// Visits every original and add chunk body mutably, in address order,
    /// stopping at the first error.
    fn try_for_each_text_mut(
        &mut self,
        mut visit: impl FnMut(&mut ChunkText) -> io::Result<()>,
    ) -> io::Result<()> {
        if let OriginalTextStorage::Chunked(original) = &mut self.original {
            for chunk in &mut Arc::make_mut(original).chunks {
                visit(&mut chunk.text)?;
            }
        }
        for index in 0..self.add.chunks.len() {
            visit(&mut self.add.chunks[index].text)?;
        }
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
    pub(super) fn store(&self) -> Option<&ChunkStoreHandle> {
        self.add.store.as_ref()
    }
    pub(super) fn intake_error(&self) -> Option<String> {
        self.add.intake_error.clone()
    }
    pub(super) fn set_bulk_intake(&mut self, enabled: bool) {
        if enabled {
            self.add.intake_error = None;
        }
        self.add.bulk_intake = enabled;
    }
    #[cfg(test)]
    pub(super) fn inject_read_failure_for_test(&self) {
        *self.fault.lock().unwrap() = Some("injected unreadable text".into());
    }
    pub(super) fn from_original(text: String) -> Self {
        Self {
            // Share the existing allocation; Arc<str> would copy the entire body.
            original: OriginalTextStorage::Owned(Arc::new(text)),
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

    pub(super) fn loaded_chunk_count(&self) -> usize {
        self.add
            .store
            .as_ref()
            .map_or(0, |store| store.owner_loaded(self.add.owner))
    }
    pub(super) fn chunk_cache_bytes(&self) -> (usize, usize) {
        self.add.store.as_ref().map_or((0, 0), |store| {
            (
                store.owner_cached_bytes(self.add.owner),
                store.owner_cache_limit(),
            )
        })
    }
    pub(super) fn chunk_cache_limit(&self) -> usize {
        self.add
            .store
            .as_ref()
            .map_or(0, |store| store.owner_cache_limit() / (16 * 1024 - 3) + 1)
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
        self.pin_span_with_owned(span, retain, false)
    }

    // Text extraction also needs strong references to owned chunks in logical order;
    // mutation preflight only needs bodies whose residency depends on the cache.
    fn pin_span_with_owned(
        &self,
        span: ByteSpan,
        retain: bool,
        include_owned: bool,
    ) -> io::Result<TextPin> {
        let result = if let Some(cause) = self.read_fault() {
            Err(io::Error::other(cause))
        } else {
            self.pin_span_inner(span, retain, include_owned)
        };
        if let Err(error) = &result
            && error.kind() != io::ErrorKind::Interrupted
        {
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

    fn pin_span_inner(
        &self,
        span: ByteSpan,
        retain: bool,
        include_owned: bool,
    ) -> io::Result<TextPin> {
        let (start, end) = checked_span_bounds(span)?;
        let mut pin = TextPin::default();
        match (&self.original, span.buffer) {
            (OriginalTextStorage::Owned(text), PieceBuffer::Original) => {
                text.get(start..end).ok_or_else(invalid_span)?;
            }
            (OriginalTextStorage::Chunked(original), PieceBuffer::Original) => {
                if end > original.byte_len {
                    return Err(invalid_span());
                }
                for chunk in original.chunks_overlapping(start, end) {
                    let body = chunk.text.try_load_for_scan(retain).map_err(|error| {
                        io::Error::new(
                            error.kind(),
                            format!("{}: {error}", original.path.display()),
                        )
                    })?;
                    pin.hold(
                        &chunk.text,
                        body,
                        chunk.logical_start,
                        start..end,
                        include_owned,
                    )?;
                }
            }
            (_, PieceBuffer::Add) => {
                if end > self.add.len() {
                    return Err(invalid_span());
                }
                for chunk in self.add.chunks_overlapping(start, end) {
                    let body = chunk.text.try_load_for_scan(retain)?;
                    pin.hold(
                        &chunk.text,
                        body,
                        chunk.start_byte,
                        start..end,
                        include_owned,
                    )?;
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
        let pin = self.pin_span_with_owned(span, retain, true)?;
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

    /// Chunks intersecting the byte range `start..end`, in address order.
    fn chunks_overlapping(&self, start: usize, end: usize) -> impl Iterator<Item = &AddTextChunk> {
        let first = self
            .chunks
            .partition_point(|chunk| chunk.start_byte + chunk.text.len() <= start);
        self.chunks
            .iter()
            .skip(first)
            .take_while(move |chunk| chunk.start_byte < end)
    }

    /// Bytes the mutable inline tail can still take; sealed tails take none.
    fn tail_capacity(&self) -> usize {
        match self.chunks.last().map(|tail| &tail.text) {
            Some(ChunkText::Inline(text)) => ADD_TAIL_BYTES.saturating_sub(text.len()),
            _ => 0,
        }
    }

    /// Seals the current tail so a new one can start. Returns false when bulk
    /// intake could not store the tail, in which case appending must stop.
    fn seal_tail(&mut self) -> bool {
        let Some(tail) = self.chunks.last_mut() else {
            return true;
        };
        if self.bulk_intake
            && let Some(store) = &self.store
            && let ChunkText::Inline(body) = &tail.text
        {
            match store.seal_bulk(body.clone(), self.owner) {
                Ok(chunk) => tail.text = ChunkText::Stored(chunk),
                Err(cause) => {
                    self.intake_error = Some(cause.to_string());
                    return false;
                }
            }
        } else {
            tail.text.seal(self.owner, self.store.as_ref());
        }
        true
    }

    fn append(&mut self, text: &str) -> AddTextSpan {
        let span = AddTextSpan {
            start_byte: self.len(),
            byte_len: text.len(),
            char_len: text.chars().count(),
        };
        let mut remaining = text;
        while !remaining.is_empty() {
            let take = remaining.floor_char_boundary(remaining.len().min(self.tail_capacity()));
            if take == 0 {
                if !self.seal_tail() {
                    break;
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
    /// Chunks intersecting the logical byte range `start..end`, in order.
    fn chunks_overlapping(&self, start: usize, end: usize) -> impl Iterator<Item = &OriginalChunk> {
        let first = self
            .chunks
            .partition_point(|chunk| chunk.logical_start + chunk.byte_len <= start);
        self.chunks
            .iter()
            .skip(first)
            .take_while(move |chunk| chunk.logical_start < end)
    }

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

#[cfg(test)]
mod pin_tests {
    use super::{Arc, ByteSpan, ChunkText, PieceBuffer, PieceSource};
    use crate::app::domain::buffer::PieceTreeLite;

    #[test]
    fn boundary_pin_does_not_force_tail_copy_but_real_snapshots_stay_immutable() {
        let mut tree = PieceTreeLite::from_string(String::new());
        let initial = "λ".repeat(4096);
        tree.insert_with_source(0, &initial, PieceSource::Paste);
        let ChunkText::Inline(tail) = &mut tree.storage.add.chunks.last_mut().unwrap().text else {
            panic!("inline tail");
        };
        Arc::get_mut(tail).unwrap().reserve(16);
        let original_pointer = tail.as_ptr();
        let pin = tree.pin_range(tree.len_chars()..tree.len_chars()).unwrap();
        assert!(pin.bodies.is_empty());
        tree.insert_with_source(tree.len_chars(), "é", PieceSource::Paste);
        let ChunkText::Inline(tail) = &tree.storage.add.chunks.last().unwrap().text else {
            panic!("inline tail");
        };
        assert_eq!(tail.as_ptr(), original_pointer);
        let snapshot = tree.clone();
        tree.insert_with_source(tree.len_chars(), "東京", PieceSource::Paste);
        assert_eq!(snapshot.extract_text(), format!("{initial}é"));
        assert_eq!(tree.extract_text(), format!("{initial}é東京"));
        drop(pin);
    }

    #[test]
    fn owned_span_pins_still_reject_invalid_utf8_boundaries() {
        let mut tree = PieceTreeLite::from_string(String::new());
        tree.insert_with_source(0, "λ", PieceSource::Paste);
        assert!(
            tree.storage
                .pin_span(
                    ByteSpan {
                        buffer: PieceBuffer::Add,
                        start_byte: 1,
                        byte_len: 1,
                        char_len: 1
                    },
                    true
                )
                .is_err()
        );
        assert!(tree.read_fault().is_some());
    }
}
