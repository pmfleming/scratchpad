use super::{
    AddTextStorage, ChunkText, ChunkedOriginal, OriginalChunk, OriginalTextStorage, Piece,
    PieceBuffer, PieceTreeStorage,
};
use crate::app::{
    domain::buffer::{StreamedTextInspection, StreamingTextInspector},
    services::chunk_store::{ImportContext, Utf8Chunks},
};
use std::{
    fs::File,
    io::{self, BufReader, Seek, SeekFrom},
    path::Path,
    sync::Arc,
};

impl PieceTreeStorage {
    pub(crate) fn import_utf8(
        path: &Path,
        offset: u64,
        context: &ImportContext,
    ) -> io::Result<(Self, Vec<Piece>, StreamedTextInspection)> {
        context.check()?;
        let mut file = BufReader::new(File::open(path)?);
        file.seek(SeekFrom::Start(offset))?;
        Self::import_reader(file, path, context)
    }
    pub(crate) fn import_reader(
        reader: impl std::io::Read,
        path: &Path,
        context: &ImportContext,
    ) -> io::Result<(Self, Vec<Piece>, StreamedTextInspection)> {
        let add = AddTextStorage {
            store: context.store.clone(),
            ..AddTextStorage::default()
        };
        let mut inspector = StreamingTextInspector::new();
        let mut pieces = Vec::new();
        let mut chunks = Vec::new();
        let mut logical_start = 0;
        for body in Utf8Chunks::new(reader) {
            context.check()?;
            let body = Arc::new(body?);
            if body.contains('\0') {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Binary files are not supported",
                ));
            }
            let piece = Piece::from_slice(PieceBuffer::Original, logical_start, &body);
            inspector.observe(&body);
            let text = context
                .seal(body.clone(), add.owner)?
                .map_or_else(|| ChunkText::Memory(body), ChunkText::Stored);
            chunks.push(OriginalChunk {
                logical_start,
                text,
                byte_len: piece.byte_len,
                char_len: piece.char_len,
            });
            logical_start += piece.byte_len;
            pieces.push(piece);
        }
        context.check()?;
        Ok((
            Self {
                original: OriginalTextStorage::Chunked(Arc::new(ChunkedOriginal {
                    path: path.to_owned(),
                    chunks,
                    byte_len: logical_start,
                })),
                add,
                fault: Arc::default(),
                original_reachable: None,
                durable_original: Arc::default(),
            },
            pieces,
            inspector.finish(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    #[test]
    fn cancellation_after_a_chunk_abandons_the_entire_import() {
        struct CancelDuringRead {
            reads: usize,
            flag: Arc<AtomicBool>,
        }
        impl std::io::Read for CancelDuringRead {
            fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
                self.reads += 1;
                if self.reads == 2 {
                    self.flag.store(true, Ordering::Release);
                }
                bytes.fill(b'x');
                Ok(bytes.len())
            }
        }
        let flag = Arc::new(AtomicBool::new(false));
        let context = ImportContext {
            cancelled: Some(flag.clone()),
            ..Default::default()
        };
        let cause = PieceTreeStorage::import_reader(
            CancelDuringRead { reads: 0, flag },
            Path::new("cancelled.txt"),
            &context,
        )
        .err()
        .expect("cancelled import must not publish partial text");
        assert_eq!(cause.kind(), io::ErrorKind::Interrupted);
    }
}
