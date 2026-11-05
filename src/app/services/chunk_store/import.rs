//! Bounded UTF-8-adjusted CDC; no file backing or whole-file staging.
use super::{Chunk, ChunkStoreHandle};
use std::{
    io::{self, Read},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

pub(crate) const MAX_BYTES: usize = 256 * 1024;
#[derive(Clone, Default)]
pub(crate) struct ImportContext {
    pub(crate) store: Option<ChunkStoreHandle>,
    pub(crate) cancelled: Option<Arc<AtomicBool>>,
}
impl ImportContext {
    pub(crate) fn check(&self) -> io::Result<()> {
        if self
            .store
            .as_ref()
            .is_some_and(super::ChunkStoreHandle::bulk_stopped)
            || self
                .cancelled
                .as_ref()
                .is_some_and(|flag| flag.load(Ordering::Acquire))
        {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "Text import was cancelled",
            ));
        }
        Ok(())
    }
    pub(crate) fn seal(&self, body: Arc<String>, owner: u64) -> io::Result<Option<Arc<Chunk>>> {
        self.check()?;
        let Some(store) = &self.store else {
            return Ok(None);
        };
        if let Err(cause) = store.wait_for_capacity_while(|| self.check().is_ok())
            && cause.kind() == io::ErrorKind::Interrupted
        {
            return Err(cause);
        }
        self.check()?;
        store.seal_bulk(body, owner).map(Some)
    }
}

pub(crate) struct Utf8Chunks<R> {
    reader: R,
    bytes: Vec<u8>,
    offset: usize,
    eof: bool,
    failed: bool,
}
impl<R: Read> Utf8Chunks<R> {
    pub(crate) fn new(reader: R) -> Self {
        Self {
            reader,
            bytes: Vec::with_capacity(MAX_BYTES),
            offset: 0,
            eof: false,
            failed: false,
        }
    }
    fn body(&mut self) -> io::Result<Option<String>> {
        if !self.eof {
            self.bytes.copy_within(self.offset.., 0);
            self.bytes.truncate(self.bytes.len() - self.offset);
            self.offset = 0;
            let mut filled = self.bytes.len();
            self.bytes.resize(MAX_BYTES, 0);
            while filled < MAX_BYTES {
                match self.reader.read(&mut self.bytes[filled..]) {
                    Ok(0) => {
                        self.eof = true;
                        break;
                    }
                    Ok(read) => filled += read,
                    Err(cause) if cause.kind() == io::ErrorKind::Interrupted => {}
                    Err(cause) => return Err(cause),
                }
            }
            self.bytes.truncate(filled);
        }
        let available = &self.bytes[self.offset..];
        if available.is_empty() {
            return Ok(None);
        }
        let cut = fastcdc::v2020::FastCDC::new(available, 16 * 1024, 64 * 1024, MAX_BYTES)
            .next()
            .ok_or_else(|| io::Error::other("CDC produced no text chunk"))?
            .length;
        let mut body = available[..cut].to_vec();
        let text = match String::from_utf8(body) {
            Ok(text) => text,
            Err(cause)
                if cause.utf8_error().error_len().is_none()
                    && !(self.eof && cut == available.len()) =>
            {
                let valid = cause.utf8_error().valid_up_to();
                body = cause.into_bytes();
                body.truncate(valid);
                String::from_utf8(body)
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?
            }
            Err(cause) => return Err(io::Error::new(io::ErrorKind::InvalidData, cause)),
        };
        if text.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Incomplete UTF-8 sequence in import",
            ));
        }
        self.offset += text.len();
        Ok(Some(text))
    }
}
impl<R: Read> Iterator for Utf8Chunks<R> {
    type Item = io::Result<String>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.failed {
            return None;
        }
        match self.body() {
            Ok(Some(body)) => Some(Ok(body)),
            Ok(None) => None,
            Err(cause) => {
                self.failed = true;
                Some(Err(cause))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cdc_preserves_utf8_boundaries_and_rejects_truncated_input() {
        let text = "λ🦀\r\n東京 café ".repeat(100_000);
        let chunks = Utf8Chunks::new(text.as_bytes())
            .collect::<io::Result<Vec<_>>>()
            .unwrap();
        assert!(chunks.iter().all(|c| c.len() <= MAX_BYTES));
        assert_eq!(chunks.concat(), text);
        let mut invalid = text.into_bytes();
        invalid.extend([0xe2, 0x82]);
        assert!(
            Utf8Chunks::new(invalid.as_slice())
                .collect::<io::Result<Vec<_>>>()
                .is_err()
        );
    }
}
