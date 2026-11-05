//! A reset copies references privately and publishes remapping only after FULL commit.
use super::{Chunk, ChunkStoreHandle, State};
use std::{
    collections::HashMap,
    io,
    sync::{Arc, atomic::Ordering},
};
pub(crate) struct Rebind {
    pub store: ChunkStoreHandle,
    copied: HashMap<usize, (Arc<Chunk>, Arc<Chunk>)>,
}
impl Rebind {
    pub(crate) fn new(store: ChunkStoreHandle) -> Self {
        Self {
            store,
            copied: HashMap::new(),
        }
    }
    pub(crate) fn copy(&mut self, old: &Arc<Chunk>) -> io::Result<Arc<Chunk>> {
        let key = Arc::as_ptr(old) as usize;
        if let Some((_, new)) = self.copied.get(&key) {
            return Ok(new.clone());
        }
        self.store.wait_for_capacity()?;
        let body = old.load(true, false)?;
        let new = self.store.seal(body, old.owner);
        self.copied.insert(key, (old.clone(), new.clone()));
        Ok(new)
    }
    pub(crate) fn publish(self) -> io::Result<()> {
        // Verify the entire new reference set before changing any old handle.
        let prepared = self
            .copied
            .into_values()
            .map(|(old, new)| {
                new.reference()
                    .map(|r| (old, r))
                    .ok_or_else(|| io::Error::other("reset body did not commit"))
            })
            .collect::<io::Result<Vec<_>>>()?;
        for (old, reference) in prepared {
            self.store
                .0
                .handles
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(Arc::downgrade(&old));
            let mut state = old
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let resident = match &*state {
                State::Pending(body) => Arc::downgrade(body),
                State::Committed { resident, .. } => resident.clone(),
            };
            if matches!(*state, State::Pending(_))
                && let Some(core) = old.core.upgrade()
            {
                let mut queue = core
                    .pending
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                queue.retain(|chunk| chunk.id != old.id);
                core.pending_bytes.fetch_sub(old.len, Ordering::AcqRel);
                core.changed.notify_all();
            }
            *state = State::Committed {
                reference,
                resident,
                loading: false,
            };
            old.changed.notify_all();
        }
        Ok(())
    }
}

impl Rebind {
    pub(crate) fn copy_live(&mut self, old: &ChunkStoreHandle) -> io::Result<()> {
        old.reap_abandoned();
        let handles = old
            .0
            .handles
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        for handle in handles {
            if let Some(chunk) = handle.upgrade() {
                self.copy(&chunk)?;
            }
        }
        Ok(())
    }
}
