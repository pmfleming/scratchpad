//! Checked scalar conversions and row grouping shared by row decoders.
use super::{error, invalid, quarantine::damaged};
use crate::app::domain::buffer::{ByteSpan, PieceBuffer};
use std::{collections::HashMap, io};

pub(super) struct MetadataRow<'a>(pub(super) &'a turso::Row);

impl MetadataRow<'_> {
    pub fn unsigned(&self, index: usize) -> io::Result<u64> {
        unsigned(self.0.get(index).map_err(error)?)
    }
    pub fn index(&self, index: usize) -> io::Result<usize> {
        usize::try_from(self.0.get::<i64>(index).map_err(error)?).map_err(invalid)
    }
    pub fn boolean(&self, index: usize) -> io::Result<bool> {
        boolean(self.0.get(index).map_err(error)?)
    }
    fn optional<T>(
        &self,
        index: usize,
        convert: impl FnOnce(i64) -> io::Result<T>,
    ) -> io::Result<Option<T>> {
        self.0
            .get::<Option<i64>>(index)
            .map_err(error)?
            .map(convert)
            .transpose()
    }
    pub fn optional_index(&self, index: usize) -> io::Result<Option<usize>> {
        self.optional(index, |value| usize::try_from(value).map_err(invalid))
    }
    pub fn optional_boolean(&self, index: usize) -> io::Result<Option<bool>> {
        self.optional(index, boolean)
    }
    pub fn optional_unsigned(&self, index: usize) -> io::Result<Option<u64>> {
        self.optional(index, unsigned)
    }
}

pub(super) fn signed(value: u64) -> io::Result<i64> {
    i64::try_from(value).map_err(invalid)
}
pub(super) fn unsigned(value: i64) -> io::Result<u64> {
    u64::try_from(value).map_err(invalid)
}
pub(super) fn boolean(value: i64) -> io::Result<bool> {
    match value {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(invalid("invalid metadata boolean")),
    }
}

/// Groups `(document id, row)` pairs by document, keeping each document's row order.
pub(super) fn group_by_id<T>(rows: Vec<(String, T)>) -> HashMap<String, Vec<T>> {
    let mut grouped: HashMap<String, Vec<T>> = HashMap::new();
    for (id, row) in rows {
        grouped.entry(id).or_default().push(row);
    }
    grouped
}

/// A stored span `[buffer, start_byte, byte_len, char_len]`, where buffer 0 is
/// the original text and any other value the add buffer. Callers reject flags above 1.
pub(super) fn byte_span([buffer, start_byte, byte_len, char_len]: [u64; 4]) -> ByteSpan {
    ByteSpan {
        buffer: if buffer == 0 {
            PieceBuffer::Original
        } else {
            PieceBuffer::Add
        },
        start_byte,
        byte_len,
        char_len,
    }
}

/// Decodes a row that names its document in `id_column`, attributing any
/// decoding failure to that document so it can be quarantined alone.
pub(super) fn document_row<T>(
    r: &turso::Row,
    id_column: usize,
    decode: impl FnOnce(&turso::Row) -> io::Result<T>,
) -> io::Result<(String, T)> {
    let id = r.get::<String>(id_column).map_err(error)?;
    let row = decode(r).map_err(|e| damaged(&id, e))?;
    Ok((id, row))
}
