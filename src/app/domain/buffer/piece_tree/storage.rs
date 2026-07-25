use super::support::measure_text;
use super::{ByteSpan, Piece, PieceBuffer, PieceProvenance, PieceSource, PieceTreeText};
use crate::app::domain::buffer::{accumulate_line_count, history::PieceProvenanceStore};
use std::collections::{HashMap, VecDeque};
use std::fs::File;
use std::io::{self, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

const FILE_CHUNK_BYTES: usize = 256 * 1024;
const FILE_CHUNK_CACHE_BYTES: usize = 8 * 1024 * 1024;
const FILE_CHUNK_CACHE_LIMIT: usize = FILE_CHUNK_CACHE_BYTES / FILE_CHUNK_BYTES;

#[derive(Clone, Debug)]
pub(super) struct PieceTreeStorage {
    original: OriginalTextStorage,
    add: String,
    provenance: PieceProvenanceStore,
}

#[derive(Clone, Debug)]
enum OriginalTextStorage {
    Owned(Arc<str>),
    FileBacked(Arc<FileBackedOriginal>),
}

#[derive(Debug)]
struct FileBackedOriginal {
    path: PathBuf,
    file: Mutex<File>,
    chunks: Vec<FileBackedChunk>,
    cache: Mutex<FileChunkCache>,
    byte_len: usize,
}

#[derive(Debug)]
struct FileBackedChunk {
    logical_start: usize,
    file_offset: u64,
    byte_len: usize,
}

#[derive(Debug, Default)]
struct FileChunkCache {
    loaded: HashMap<usize, Arc<String>>,
    least_to_most_recent: VecDeque<usize>,
}

impl FileChunkCache {
    fn get(&mut self, chunk_index: usize) -> Option<Arc<String>> {
        let text = self.loaded.get(&chunk_index)?.clone();
        self.touch(chunk_index);
        Some(text)
    }

    fn insert(&mut self, chunk_index: usize, text: Arc<String>) -> Arc<String> {
        self.loaded.insert(chunk_index, text.clone());
        self.touch(chunk_index);
        while self.loaded.len() > FILE_CHUNK_CACHE_LIMIT {
            if let Some(evicted) = self.least_to_most_recent.pop_front() {
                self.loaded.remove(&evicted);
            }
        }
        text
    }

    fn touch(&mut self, chunk_index: usize) {
        if let Some(position) = self
            .least_to_most_recent
            .iter()
            .position(|cached| *cached == chunk_index)
        {
            self.least_to_most_recent.remove(position);
        }
        self.least_to_most_recent.push_back(chunk_index);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct AddTextSpan {
    pub(super) start_byte: usize,
    pub(super) byte_len: usize,
}

impl AddTextSpan {
    pub(super) fn byte_span(self) -> ByteSpan {
        add_byte_span(self.start_byte, self.byte_len)
    }
}

impl PieceTreeStorage {
    pub(super) fn from_original(text: String) -> Self {
        Self {
            original: OriginalTextStorage::Owned(Arc::from(text.into_boxed_str())),
            add: String::new(),
            provenance: PieceProvenanceStore::default(),
        }
    }

    pub(super) fn from_utf8_file(
        path: &Path,
        file_offset: u64,
        sample_limit: usize,
    ) -> io::Result<(Self, Vec<Piece>, String, usize)> {
        let mut reader = BufReader::new(File::open(path)?);
        reader.seek(SeekFrom::Start(file_offset))?;
        let mut carry = Vec::new();
        let mut chunks = Vec::new();
        let mut pieces = Vec::new();
        let mut sample = String::new();
        let mut logical_start = 0usize;
        let mut line_count = 1usize;
        let mut pending_cr = false;

        while let Some(bytes) = read_utf8_chunk(&mut reader, &mut carry, logical_start)? {
            let text = std::str::from_utf8(&bytes).expect("validated UTF-8 chunk");
            let metrics = measure_text(text);
            pieces.push(Piece {
                buffer: PieceBuffer::Original,
                start_byte: logical_start,
                byte_len: metrics.byte_len,
                char_len: metrics.char_len,
                newline_count: metrics.newline_count,
                is_ascii: metrics.is_ascii,
            });
            append_sample(&mut sample, text, sample_limit);
            line_count = accumulate_line_count(text, line_count, &mut pending_cr);
            chunks.push(FileBackedChunk {
                logical_start,
                file_offset: file_offset + logical_start as u64,
                byte_len: bytes.len(),
            });
            logical_start += bytes.len();
        }

        let original = FileBackedOriginal {
            path: path.to_path_buf(),
            file: Mutex::new(File::open(path)?),
            chunks,
            cache: Mutex::new(FileChunkCache::default()),
            byte_len: logical_start,
        };
        Ok((
            Self {
                original: OriginalTextStorage::FileBacked(Arc::new(original)),
                add: String::new(),
                provenance: PieceProvenanceStore::default(),
            },
            pieces,
            sample,
            line_count,
        ))
    }

    pub(super) fn owned_original_text(&self) -> Option<&str> {
        match &self.original {
            OriginalTextStorage::Owned(text) => Some(text),
            OriginalTextStorage::FileBacked(_) => None,
        }
    }

    pub(super) fn original_len(&self) -> usize {
        match &self.original {
            OriginalTextStorage::Owned(text) => text.len(),
            OriginalTextStorage::FileBacked(original) => original.byte_len,
        }
    }

    pub(super) fn is_file_backed(&self) -> bool {
        matches!(self.original, OriginalTextStorage::FileBacked(_))
    }

    pub(super) fn loaded_file_chunk_count(&self) -> usize {
        match &self.original {
            OriginalTextStorage::Owned(_) => 0,
            OriginalTextStorage::FileBacked(original) => original
                .cache
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .loaded
                .len(),
        }
    }

    pub(super) fn file_chunk_cache_limit(&self) -> usize {
        if self.is_file_backed() {
            FILE_CHUNK_CACHE_LIMIT
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

    pub(super) fn text_for_span(&self, span: ByteSpan) -> PieceTreeText<'_> {
        let start = usize::try_from(span.start_byte).expect("byte span start fits this platform");
        let byte_len = usize::try_from(span.byte_len).expect("byte span length fits this platform");
        match span.buffer {
            PieceBuffer::Original => self.original_text_for_range(start, byte_len),
            PieceBuffer::Add => PieceTreeText::borrowed(&self.add[start..start + byte_len]),
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
            PieceBuffer::Add => {
                PieceTreeText::borrowed(&self.add[start_byte..start_byte + byte_len])
            }
        }
    }

    fn original_text_for_range(&self, start_byte: usize, byte_len: usize) -> PieceTreeText<'_> {
        match &self.original {
            OriginalTextStorage::Owned(text) => {
                PieceTreeText::borrowed(&text[start_byte..start_byte + byte_len])
            }
            OriginalTextStorage::FileBacked(original) => {
                original.text_for_range(start_byte, byte_len)
            }
        }
    }

    pub(super) fn append_add_text(
        &mut self,
        text: &str,
        source: PieceSource,
        generation: u64,
    ) -> AddTextSpan {
        let span = AddTextSpan {
            start_byte: self.add.len(),
            byte_len: text.len(),
        };
        self.add.push_str(text);
        self.record_add_provenance(span, source, generation);
        span
    }

    pub(super) fn take_add_if_nonempty(&mut self) -> Option<String> {
        (!self.add.is_empty()).then(|| std::mem::take(&mut self.add))
    }

    pub(super) fn replace_add(&mut self, add: String) {
        self.add = add;
    }

    pub(super) fn provenance_entry_count(&self) -> usize {
        self.provenance.len()
    }

    pub(super) fn provenance_for_span(&self, span: ByteSpan) -> PieceProvenance {
        self.provenance.provenance_for(span)
    }

    pub(super) fn rewrite_add_spans(&mut self, moves: Vec<(ByteSpan, ByteSpan)>) {
        self.provenance.rewrite_add_spans(moves);
    }

    fn record_add_provenance(&mut self, span: AddTextSpan, source: PieceSource, generation: u64) {
        self.provenance.record(
            span.byte_span(),
            PieceProvenance {
                change_id: generation,
                source,
                session_generation: 0,
            },
        );
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
        debug_assert!(relative_start + byte_len <= chunk.byte_len);
        let text = self.cached_chunk(chunk_index, chunk);
        PieceTreeText::shared(text, relative_start..relative_start + byte_len)
    }

    fn cached_chunk(&self, chunk_index: usize, chunk: &FileBackedChunk) -> Arc<String> {
        let mut cache = self
            .cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(text) = cache.get(chunk_index) {
            return text;
        }
        cache.insert(chunk_index, self.load_chunk(chunk))
    }

    fn load_chunk(&self, chunk: &FileBackedChunk) -> Arc<String> {
        let mut bytes = vec![0; chunk.byte_len];
        let mut file = self
            .file
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        file.seek(SeekFrom::Start(chunk.file_offset))
            .unwrap_or_else(|error| {
                panic!("seek file-backed buffer {}: {error}", self.path.display())
            });
        file.read_exact(&mut bytes).unwrap_or_else(|error| {
            panic!("read file-backed buffer {}: {error}", self.path.display())
        });
        let text = String::from_utf8(bytes).unwrap_or_else(|error| {
            panic!(
                "file-backed UTF-8 changed for {}: {error}",
                self.path.display()
            )
        });
        Arc::new(text)
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
    }
}
