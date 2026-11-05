mod persistence;
use super::text_cache::{self, Backing, ChunkText};
use super::{ByteSpan, Piece, PieceBuffer, PieceSource, PieceTreeText};
use crate::app::domain::buffer::PersistentVector;
use crate::app::domain::buffer::accumulate_line_count;
pub(crate) use persistence::{StorageBlock, StorageCapture};
use std::fs::File;
use std::hash::{Hash, Hasher};
use std::io::{self, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
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
    FileBacked(Arc<FileBackedOriginal>),
    Chunked(Arc<FileBackedOriginal>),
}

#[derive(Clone, Debug)]
struct FileBackedOriginal {
    path: PathBuf,
    chunks: Vec<FileBackedChunk>,
    byte_len: usize,
    fingerprint: u64,
}

#[derive(Clone, Debug)]
struct FileBackedChunk {
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
        if span.buffer != PieceBuffer::Add {
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
            return if previous.epoch().id == store.epoch().id {
                Ok(())
            } else {
                Err(io::Error::other(
                    "text belongs to another store epoch; explicit copy/rebind required",
                ))
            };
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
    pub(super) fn store(&self) -> Option<&crate::app::services::chunk_store::ChunkStoreHandle> {
        self.add.store.as_ref()
    }
    pub(super) fn set_bulk_intake(&mut self, enabled: bool) {
        self.add.bulk_intake = enabled;
    }
    #[cfg(test)]
    pub(super) fn truncate_backing_for_test(&self) -> io::Result<()> {
        let (OriginalTextStorage::FileBacked(original) | OriginalTextStorage::Chunked(original)) =
            &self.original
        else {
            return Err(io::Error::other("original not file-backed"));
        };
        original
            .chunks
            .first()
            .ok_or_else(|| io::Error::other("empty backing"))?
            .text
            .truncate_backing_for_test()
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

    pub(super) fn from_utf8_file(
        path: &Path,
        file_offset: u64,
        sample_limit: usize,
    ) -> io::Result<(Self, Vec<Piece>, String, usize)> {
        let mut reader = BufReader::new(File::open(path)?);
        reader.seek(SeekFrom::Start(file_offset))?;
        let immutable = Backing::new(tempfile::tempfile()?);
        let add = AddTextStorage::default();
        let mut fingerprint = rustc_hash::FxHasher::default();
        let mut carry = Vec::new();
        let mut chunks = Vec::new();
        let mut pieces = Vec::new();
        let mut sample = String::new();
        let mut logical_start = 0usize;
        let mut line_count = 1usize;
        let mut pending_cr = false;

        while let Some(bytes) = read_utf8_chunk(&mut reader, &mut carry, logical_start)? {
            let text = std::str::from_utf8(&bytes).map_err(|error| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("invalid UTF-8 in {}: {error}", path.display()),
                )
            })?;
            // Pin exact decoded bytes. External file writes cannot change an old node.
            let offset = immutable.append(&bytes)?;
            text.hash(&mut fingerprint);
            0xff_u8.hash(&mut fingerprint);
            pieces.push(Piece::from_slice(
                PieceBuffer::Original,
                logical_start,
                text,
            ));
            append_sample(&mut sample, text, sample_limit);
            line_count = accumulate_line_count(text, line_count, &mut pending_cr);
            chunks.push(FileBackedChunk {
                logical_start,
                text: ChunkText::from_file(
                    immutable.clone(),
                    offset,
                    bytes.len(),
                    add.owner,
                    blake3::hash(&bytes),
                ),
                char_len: text.chars().count(),
                byte_len: bytes.len(),
            });
            logical_start += bytes.len();
        }

        let original = FileBackedOriginal {
            path: path.to_path_buf(),
            chunks,
            byte_len: logical_start,
            fingerprint: fingerprint.finish(),
        };
        Ok((
            Self {
                original: OriginalTextStorage::FileBacked(Arc::new(original)),
                add,
                fault: Arc::default(),
                original_reachable: None,
                durable_original: Arc::default(),
            },
            pieces,
            sample,
            line_count,
        ))
    }

    pub(super) fn initial_file_history(&self) -> Option<(super::super::PieceHistoryEdits, u64)> {
        let (OriginalTextStorage::FileBacked(original) | OriginalTextStorage::Chunked(original)) =
            &self.original
        else {
            return None;
        };
        let mut start = 0;
        let edits = original
            .chunks
            .iter()
            .map(|chunk| {
                let edit = super::super::PieceHistoryEdit::Inserted {
                    start_char: start as u64,
                    span: ByteSpan {
                        buffer: PieceBuffer::Original,
                        start_byte: chunk.logical_start as u64,
                        byte_len: chunk.byte_len as u64,
                        char_len: chunk.char_len as u64,
                    },
                };
                start += chunk.char_len;
                edit
            })
            .collect();
        Some((edits, original.fingerprint))
    }

    pub(super) fn owned_original_text(&self) -> Option<&str> {
        match &self.original {
            OriginalTextStorage::Owned(text) => Some(text),
            OriginalTextStorage::FileBacked(_) | OriginalTextStorage::Chunked(_) => None,
        }
    }

    pub(super) fn original_len(&self) -> usize {
        match &self.original {
            OriginalTextStorage::Owned(text) => text.len(),
            OriginalTextStorage::FileBacked(original) | OriginalTextStorage::Chunked(original) => {
                original.byte_len
            }
        }
    }

    pub(super) fn is_file_backed(&self) -> bool {
        matches!(self.original, OriginalTextStorage::FileBacked(_))
    }

    pub(super) fn loaded_file_chunk_count(&self) -> usize {
        if self.is_file_backed() {
            text_cache::owner_loaded(self.add.owner)
        } else {
            0
        }
    }

    pub(super) fn file_chunk_cache_limit(&self) -> usize {
        if self.is_file_backed() {
            text_cache::file_chunk_limit(FILE_CHUNK_BYTES)
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
            (OriginalTextStorage::FileBacked(left), OriginalTextStorage::FileBacked(right)) => {
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
    }

    pub(super) fn pin_span(&self, span: ByteSpan, retain: bool) -> io::Result<TextPin> {
        let result = self.pin_span_inner(span, retain);
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
            OriginalTextStorage::FileBacked(original) | OriginalTextStorage::Chunked(original)
                if span.buffer == PieceBuffer::Original =>
            {
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
            (
                OriginalTextStorage::FileBacked(original) | OriginalTextStorage::Chunked(original),
                PieceBuffer::Original,
            ) => {
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
            OriginalTextStorage::FileBacked(original) | OriginalTextStorage::Chunked(original) => {
                original.text_for_range(start_byte, byte_len)
            }
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
                    ChunkText::Sealed(_)
                    | ChunkText::Memory(_)
                    | ChunkText::Stored(_)
                    | ChunkText::Missing(_) => None,
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

impl FileBackedOriginal {
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

    fn cached_chunk(&self, _chunk_index: usize, chunk: &FileBackedChunk) -> Arc<String> {
        chunk.text.load()
    }
}

fn read_utf8_chunk(
    reader: &mut impl Read,
    carry: &mut Vec<u8>,
    logical_start: usize,
) -> io::Result<Option<Vec<u8>>> {
    let mut bytes = std::mem::take(carry);
    let existing = bytes.len();
    bytes.resize(FILE_CHUNK_BYTES, 0);
    let mut filled = existing;
    while filled < FILE_CHUNK_BYTES {
        let read = reader.read(&mut bytes[filled..])?;
        if read == 0 {
            break;
        }
        filled += read;
    }
    bytes.truncate(filled);
    if bytes.is_empty() {
        return Ok(None);
    }

    let valid_len = match std::str::from_utf8(&bytes) {
        Ok(_) => bytes.len(),
        Err(error) if error.error_len().is_none() => error.valid_up_to(),
        Err(error) => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "Invalid UTF-8 at byte {}",
                    logical_start + error.valid_up_to()
                ),
            ));
        }
    };
    if valid_len < bytes.len() {
        *carry = bytes.split_off(valid_len);
    }
    bytes.truncate(valid_len);
    if filled < FILE_CHUNK_BYTES && !carry.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Incomplete UTF-8 sequence at end of file",
        ));
    }
    Ok(Some(bytes))
}

fn append_sample(sample: &mut String, text: &str, sample_limit: usize) {
    if sample.len() >= sample_limit {
        return;
    }
    let mut end = (sample_limit - sample.len()).min(text.len());
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    sample.push_str(&text[..end]);
}

pub(super) fn add_byte_span(start_byte: usize, byte_len: usize) -> ByteSpan {
    ByteSpan {
        buffer: PieceBuffer::Add,
        start_byte: start_byte as u64,
        byte_len: byte_len as u64,
        char_len: byte_len as u64,
    }
}
