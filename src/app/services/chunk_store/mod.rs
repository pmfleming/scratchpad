//! Immutable text handles shared by domain clones and the single database writer.
mod readers;
pub use readers::ReadEpoch;
use std::{
    collections::{HashMap, VecDeque},
    io,
    path::Path,
    sync::{
        Arc, Condvar, Mutex, Weak,
        atomic::{AtomicU64, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

/// Healthy bulk producers wait above this fixed threshold; typing never waits.
pub const BULK_PENDING_BYTES: usize = 32 * 1024 * 1024;
static NEXT_ID: AtomicU64 = AtomicU64::new(1);
static NEXT_EPOCH: AtomicU64 = AtomicU64::new(1);
fn failed(cause: impl std::fmt::Display) -> io::Error {
    io::Error::other(cause.to_string())
}

#[derive(Clone, Debug, PartialEq, Eq)]
/// Verified immutable body metadata, stored separately from overflow body pages.
pub struct Metadata {
    pub digest: [u8; 32],
    pub byte_len: usize,
    pub char_len: u64,
    pub newline_count: u64,
    pub is_ascii: bool,
}
impl Metadata {
    pub(crate) fn measure(text: &str) -> Self {
        Self {
            digest: *blake3::hash(text.as_bytes()).as_bytes(),
            byte_len: text.len(),
            char_len: text.chars().count() as u64,
            newline_count: text.bytes().filter(|&b| b == b'\n').count() as u64,
            is_ascii: text.is_ascii(),
        }
    }
}
#[derive(Clone, Debug)]
/// A committed row plus its database epoch; row IDs alone are never portable.
pub struct ChunkReference {
    pub id: i64,
    pub metadata: Metadata,
    pub epoch: Arc<ReadEpoch>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
/// Retryable write failure is distinct from a store requiring explicit reset.
pub enum Health {
    Healthy,
    Failing(String),
    Unavailable(String),
}

#[derive(Clone)]
/// Explicit per-session binding. Clones share pending bodies, cache, and readers.
pub struct ChunkStoreHandle(Arc<Core>);
impl std::fmt::Debug for ChunkStoreHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChunkStoreHandle")
            .field("epoch", &self.0.epoch.id)
            .finish_non_exhaustive()
    }
}
type Waker = Arc<dyn Fn() + Send + Sync>;
struct Core {
    prefetch: std::sync::mpsc::SyncSender<Weak<Chunk>>,
    epoch: Arc<ReadEpoch>,
    pending: Mutex<VecDeque<Arc<Chunk>>>,
    pending_bytes: AtomicUsize,
    hashed_bytes: AtomicU64,
    peak_pending_bytes: AtomicUsize,
    changed: Condvar,
    health: Mutex<Health>,
    wake: Mutex<Option<Waker>>,
    cache: Mutex<Cache>,
    handles: Mutex<Vec<Weak<Chunk>>>,
    restored: Mutex<HashMap<i64, Weak<Chunk>>>,
}
#[derive(Debug)]
/// Shared immutable text. Accepted pending bodies remain readable after a write failure.
pub struct Chunk {
    id: u64,
    owner: u64,
    len: usize,
    core: Weak<Core>,
    state: Mutex<State>,
    changed: Condvar,
}
#[derive(Debug)]
enum State {
    Pending(Arc<String>),
    Committed {
        reference: ChunkReference,
        resident: Weak<String>,
        loading: bool,
    },
}
#[derive(Default)]
struct Cache {
    loaded: HashMap<u64, (u64, Arc<String>)>,
    order: VecDeque<u64>,
    bytes: usize,
    owners: HashMap<u64, usize>,
    limit: usize,
    per_owner: usize,
}
impl Cache {
    fn remove(&mut self, id: u64) {
        if let Some((owner, body)) = self.loaded.remove(&id) {
            self.bytes -= body.len();
            if let Some(bytes) = self.owners.get_mut(&owner) {
                *bytes -= body.len();
                if *bytes == 0 {
                    self.owners.remove(&owner);
                }
            }
        }
    }
    fn insert(&mut self, id: u64, owner: u64, body: Arc<String>) {
        self.remove(id);
        self.order.retain(|&key| key != id);
        self.bytes += body.len();
        *self.owners.entry(owner).or_default() += body.len();
        self.loaded.insert(id, (owner, body));
        self.order.push_back(id);
        self.evict();
    }
    fn evict(&mut self) {
        let mut kept = VecDeque::new();
        while let Some(id) = self.order.pop_front() {
            let Some((owner, _)) = self.loaded.get(&id) else {
                continue;
            };
            if self.bytes > self.limit || self.owners[owner] > self.per_owner {
                self.remove(id);
            } else {
                kept.push_back(id);
            }
        }
        self.order = kept;
    }
}
impl ChunkStoreHandle {
    pub(crate) fn open(path: &Path) -> io::Result<Self> {
        let (prefetch, incoming) = std::sync::mpsc::sync_channel::<Weak<Chunk>>(2);
        // The bounded coordinator holds only weak handles. Closing the store's
        // sender stops it; epoch closure joins the actual engine reader threads.
        drop(
            std::thread::Builder::new()
                .name("text-prefetch".into())
                .spawn(move || {
                    while let Ok(chunk) = incoming.recv() {
                        if let Some(chunk) = chunk.upgrade() {
                            let _ = chunk.load(true, true);
                        }
                    }
                })?,
        );
        let epoch = ReadEpoch::open(path, NEXT_EPOCH.fetch_add(1, Ordering::Relaxed))?;
        Ok(Self(Arc::new(Core {
            prefetch,
            epoch,
            pending: Mutex::new(VecDeque::new()),
            pending_bytes: AtomicUsize::new(0),
            hashed_bytes: AtomicU64::new(0),
            peak_pending_bytes: AtomicUsize::new(0),
            changed: Condvar::new(),
            health: Mutex::new(Health::Healthy),
            wake: Mutex::new(None),
            handles: Mutex::default(),
            restored: Mutex::default(),
            cache: Mutex::new(Cache {
                limit: 16 * 1024 * 1024,
                per_owner: 4 * 1024 * 1024,
                ..Cache::default()
            }),
        })))
    }
    #[must_use]
    /// The epoch used to interpret committed row references from this store.
    pub fn epoch(&self) -> Arc<ReadEpoch> {
        self.0.epoch.clone()
    }
    pub(crate) fn set_waker(&self, wake: Waker) {
        *self
            .0
            .wake
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(wake);
    }
    /// Read health directly, without entering the database worker's queue.
    pub fn health(&self) -> Health {
        self.0
            .health
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
    pub(crate) fn set_health(&self, health: Health) {
        let mut state = self
            .0
            .health
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if *state != health {
            if std::mem::discriminant(&*state) != std::mem::discriminant(&health) {
                log::warn!("text store epoch {} health: {:?}", self.0.epoch.id, health);
            }
            *state = health;
        }
        self.0.changed.notify_all();
    }
    #[must_use]
    /// RAM held by accepted sealed bodies that have not committed yet.
    pub fn pending_bytes(&self) -> usize {
        self.0.pending_bytes.load(Ordering::Acquire)
    }

    /// Resident cached bytes, excluding pinned-only and pending bodies.
    #[must_use]
    pub fn cached_bytes(&self) -> usize {
        self.0
            .cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .bytes
    }

    /// High-water mark for pending sealed bodies in this store epoch.
    #[must_use]
    pub fn peak_pending_bytes(&self) -> usize {
        self.0.peak_pending_bytes.load(Ordering::Acquire)
    }
    /// Accept an immutable body without hashing or waiting for the writer.
    pub fn seal(&self, body: Arc<String>, owner: u64) -> Arc<Chunk> {
        let chunk = Arc::new(Chunk {
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
            owner,
            len: body.len(),
            core: Arc::downgrade(&self.0),
            state: Mutex::new(State::Pending(body)),
            changed: Condvar::new(),
        });
        self.0
            .handles
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(Arc::downgrade(&chunk));
        let pending = self.0.pending_bytes.fetch_add(chunk.len, Ordering::AcqRel) + chunk.len;
        self.0
            .peak_pending_bytes
            .fetch_max(pending, Ordering::AcqRel);
        self.0
            .pending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push_back(chunk.clone());
        let wake = self
            .0
            .wake
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        if let Some(wake) = wake {
            wake();
        }
        chunk
    }
    /// Bulk callers only; typing/paint must never wait for the writer.
    pub fn wait_for_capacity(&self) -> io::Result<()> {
        let mut pending = self
            .0
            .pending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        while self.pending_bytes() >= BULK_PENDING_BYTES {
            if !matches!(self.health(), Health::Healthy) {
                return Err(failed(
                    "chunk store is not saving; bulk intake must use the failure RAM budget",
                ));
            }
            pending = self
                .0
                .changed
                .wait_timeout(pending, Duration::from_millis(50))
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .0;
        }
        Ok(())
    }
    pub(crate) fn pop_pending(&self) -> Option<Arc<Chunk>> {
        self.0
            .pending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .pop_front()
    }
    pub(crate) fn retry(&self, chunks: Vec<Arc<Chunk>>) {
        let mut queue = self
            .0
            .pending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        for chunk in chunks.into_iter().rev() {
            queue.push_front(chunk);
        }
        self.0.changed.notify_all();
    }
    pub(crate) fn committed(&self, chunk: &Chunk, reference: ChunkReference) {
        let body = {
            let mut state = chunk
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let State::Pending(body) = &*state else {
                return;
            };
            let body = body.clone();
            *state = State::Committed {
                reference,
                resident: Arc::downgrade(&body),
                loading: false,
            };
            chunk.changed.notify_all();
            body
        };
        self.0
            .cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(chunk.id, chunk.owner, body);
        self.0.pending_bytes.fetch_sub(chunk.len, Ordering::AcqRel);
        self.0.changed.notify_all();
    }
    /// Reconstruct a lazy handle from metadata without reading its body.
    pub fn restored(&self, reference: ChunkReference, owner: u64) -> Arc<Chunk> {
        let mut restored = self
            .0
            .restored
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(chunk) = restored.get(&reference.id).and_then(Weak::upgrade) {
            return chunk;
        }
        let row = reference.id;
        let chunk = Arc::new(Chunk {
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
            owner,
            len: reference.metadata.byte_len,
            core: Arc::downgrade(&self.0),
            state: Mutex::new(State::Committed {
                reference,
                resident: Weak::new(),
                loading: false,
            }),
            changed: Condvar::new(),
        });
        restored.insert(row, Arc::downgrade(&chunk));
        self.0
            .handles
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(Arc::downgrade(&chunk));
        chunk
    }
    /// Bytes hashed by this epoch's writer, including deduplication attempts.
    #[must_use]
    pub fn hashed_bytes(&self) -> u64 {
        self.0.hashed_bytes.load(Ordering::Relaxed)
    }
    pub(crate) fn record_hashed(&self, bytes: usize) {
        self.0
            .hashed_bytes
            .fetch_add(bytes as u64, Ordering::Relaxed);
    }
    pub(crate) fn live_row_ids(&self) -> std::collections::HashSet<i64> {
        let mut handles = self
            .0
            .handles
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut ids = std::collections::HashSet::new();
        handles.retain(|handle| {
            if let Some(chunk) = handle.upgrade() {
                if let Some(reference) = chunk.reference() {
                    ids.insert(reference.id);
                }
                true
            } else {
                false
            }
        });
        ids
    }
    /// Apply aggregate and per-document cache budgets immediately.
    pub fn set_limits(&self, limit: usize, per_owner: usize) {
        let mut cache = self
            .0
            .cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        cache.limit = limit;
        cache.per_owner = per_owner;
        cache.evict();
    }
    pub(crate) fn close_readers(&self) {
        self.0.epoch.close();
    }
}
impl Chunk {
    /// Queue nearby text without waiting; saturated/stale prefetches are expendable.
    pub fn prefetch(self: &Arc<Self>) {
        if let Some(core) = self.core.upgrade() {
            let _ = core.prefetch.try_send(Arc::downgrade(self));
        }
    }
    /// Whether this handle represents an empty body.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
    /// UTF-8 byte length; reading this never faults text into memory.
    pub fn len(&self) -> usize {
        self.len
    }
    pub(crate) fn pending_body(&self) -> Option<Arc<String>> {
        match &*self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
        {
            State::Pending(body) => Some(body.clone()),
            State::Committed { .. } => None,
        }
    }
    pub(crate) fn reference(&self) -> Option<ChunkReference> {
        match &*self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
        {
            State::Pending(_) => None,
            State::Committed { reference, .. } => Some(reference.clone()),
        }
    }
    /// Load verified text through the selected reader. Scans should not retain
    /// bodies in the foreground cache; returned Arcs remain valid through eviction.
    pub fn load(&self, background: bool, retain: bool) -> io::Result<Arc<String>> {
        let deadline = Instant::now()
            + if background {
                Duration::from_secs(5)
            } else {
                Duration::from_millis(250)
            };
        let reference = {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            loop {
                match &mut *state {
                    State::Pending(body) => return Ok(body.clone()),
                    State::Committed {
                        reference,
                        resident,
                        loading,
                    } => {
                        if let Some(body) = resident.upgrade() {
                            if retain && let Some(core) = self.core.upgrade() {
                                core.cache
                                    .lock()
                                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                                    .insert(self.id, self.owner, body.clone());
                            }
                            return Ok(body);
                        }
                        if !*loading {
                            *loading = true;
                            break reference.clone();
                        }
                        let remaining = deadline.saturating_duration_since(Instant::now());
                        if remaining.is_zero() {
                            return Err(io::Error::new(
                                io::ErrorKind::TimedOut,
                                "coalesced chunk read deadline exceeded",
                            ));
                        }
                        state = self
                            .changed
                            .wait_timeout(state, remaining)
                            .unwrap_or_else(std::sync::PoisonError::into_inner)
                            .0;
                    }
                }
            }
        };
        let result = reference.epoch.load(&reference, background);
        {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let State::Committed {
                resident, loading, ..
            } = &mut *state
            {
                *loading = false;
                if let Ok(body) = &result {
                    *resident = Arc::downgrade(body);
                }
            }
            self.changed.notify_all();
        }
        if retain && let (Ok(body), Some(core)) = (&result, self.core.upgrade()) {
            core.cache
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .insert(self.id, self.owner, body.clone());
        }
        result
    }
}
