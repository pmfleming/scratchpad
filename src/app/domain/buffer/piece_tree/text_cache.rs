//! Immutable bodies are either accepted RAM or explicit session-store handles.
use std::{
    io,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};
static NEXT_ID: AtomicU64 = AtomicU64::new(1);
pub(crate) fn new_owner() -> u64 {
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}
#[derive(Clone, Debug)]
pub(super) enum ChunkText {
    Inline(Arc<String>),
    Missing(usize),
    Memory(Arc<String>),
    Stored(Arc<crate::app::services::chunk_store::Chunk>),
}
impl ChunkText {
    pub(super) fn len(&self) -> usize {
        match self {
            Self::Inline(body) | Self::Memory(body) => body.len(),
            Self::Stored(chunk) => chunk.len(),
            Self::Missing(len) => *len,
        }
    }
    pub(super) fn seal(
        &mut self,
        owner: u64,
        store: Option<&crate::app::services::chunk_store::ChunkStoreHandle>,
    ) {
        let Self::Inline(body) = self else { return };
        *self = match store {
            Some(store) => Self::Stored(store.seal(body.clone(), owner)),
            None => Self::Memory(body.clone()),
        };
    }
    pub(super) fn bind_store(
        &mut self,
        owner: u64,
        store: &crate::app::services::chunk_store::ChunkStoreHandle,
    ) {
        if let Self::Memory(body) = self {
            *self = Self::Stored(store.seal(body.clone(), owner));
        }
    }
    pub(super) fn try_load(&self) -> io::Result<Arc<String>> {
        self.try_load_for_scan(true)
    }
    pub(super) fn try_load_for_scan(&self, retain: bool) -> io::Result<Arc<String>> {
        match self {
            Self::Inline(body) | Self::Memory(body) => Ok(body.clone()),
            Self::Stored(chunk) => chunk.load(!retain, retain),
            Self::Missing(_) => Err(io::Error::other(
                "text range was released after Forget history",
            )),
        }
    }
    pub(super) fn load(&self) -> Arc<String> {
        self.try_load()
            .expect("immutable text must be preflighted and pinned before infallible access")
    }
}
