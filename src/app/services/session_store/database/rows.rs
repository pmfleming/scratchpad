//! Checked scalar conversions shared by metadata row decoders.
use super::{error, invalid};
use std::io;

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
    pub fn optional_index(&self, index: usize) -> io::Result<Option<usize>> {
        self.0
            .get::<Option<i64>>(index)
            .map_err(error)?
            .map(|value| usize::try_from(value).map_err(invalid))
            .transpose()
    }
    pub fn optional_boolean(&self, index: usize) -> io::Result<Option<bool>> {
        self.0
            .get::<Option<i64>>(index)
            .map_err(error)?
            .map(boolean)
            .transpose()
    }
    pub fn optional_unsigned(&self, index: usize) -> io::Result<Option<u64>> {
        self.0
            .get::<Option<i64>>(index)
            .map_err(error)?
            .map(unsigned)
            .transpose()
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
