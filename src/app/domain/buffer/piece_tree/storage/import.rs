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
        let add = AddTextStorage {
            store: context.store.clone(),
            ..AddTextStorage::default()
        };
        let mut inspector = StreamingTextInspector::new();
        let mut pieces = Vec::new();
        let mut chunks = Vec::new();
        let mut logical_start = 0;
        for body in Utf8Chunks::new(file) {
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
