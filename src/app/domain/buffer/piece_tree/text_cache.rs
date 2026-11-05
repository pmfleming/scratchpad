//! One temporary backing/cache for original and add chunks. Cache/state locks
//! protect metadata only: positioned file I/O always runs outside those locks.
use std::{
    collections::{HashMap, VecDeque},
    fs::File,
    io,
    sync::{
        Arc, Condvar, Mutex, OnceLock, Weak,
        atomic::{AtomicU64, Ordering},
    },
};

const DEFAULT_LIMIT: usize = 16 * 1024 * 1024;
static NEXT_ID: AtomicU64 = AtomicU64::new(1);
pub(super) fn new_owner() -> u64 {
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}

#[derive(Clone, Debug)]
pub(super) enum ChunkText {
    Inline(Arc<String>),
    Sealed(Arc<Chunk>),
}

#[derive(Debug)]
pub(super) struct Backing {
    file: File,
    end: AtomicU64,
}
impl Backing {
    pub(super) fn new(file: File) -> Arc<Self> {
        Arc::new(Self {
            file,
            end: AtomicU64::new(0),
        })
    }
    pub(super) fn append(&self, bytes: &[u8]) -> io::Result<u64> {
        let offset = self
            .end
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |old| {
                old.checked_add(bytes.len() as u64)
            })
            .map_err(|_| io::Error::other("temporary backing offset overflow"))?;
        write_all_at(&self.file, bytes, offset)?;
        Ok(offset)
    }
}

#[derive(Debug)]
pub(super) struct Chunk {
    id: u64,
    owner: u64,
    len: usize,
    digest: blake3::Hash,
    state: Mutex<ChunkState>,
    ready: Condvar,
}
#[derive(Debug)]
enum ChunkState {
    Pending(Arc<String>),
    Committed {
        backing: Arc<Backing>,
        offset: u64,
        resident: Weak<String>,
        loading: bool,
    },
    Failed {
        body: Arc<String>,
        _cause: String,
    },
}

#[derive(Default)]
struct Cache {
    loaded: HashMap<u64, (u64, Arc<String>)>,
    order: VecDeque<u64>,
    bytes: usize,
    limit: usize,
    per_file_limit: usize,
    owner_bytes: HashMap<u64, usize>,
    error: Option<String>,
}
fn cache() -> &'static Mutex<Cache> {
    static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();
    CACHE.get_or_init(|| {
        Mutex::new(Cache {
            limit: DEFAULT_LIMIT,
            per_file_limit: DEFAULT_LIMIT / 4,
            ..Default::default()
        })
    })
}
fn temporary_backing() -> io::Result<Arc<Backing>> {
    static FILE: Mutex<Option<Arc<Backing>>> = Mutex::new(None);
    if let Some(backing) = FILE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone()
    {
        return Ok(backing);
    }
    // Creation can fail or race; neither path holds the cache or a chunk lock.
    let fresh = Backing::new(tempfile::tempfile()?);
    Ok(FILE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get_or_insert(fresh)
        .clone())
}
impl Cache {
    fn remove(&mut self, key: u64) {
        if let Some((owner, text)) = self.loaded.remove(&key) {
            self.bytes -= text.len();
            if let Some(total) = self.owner_bytes.get_mut(&owner) {
                *total -= text.len();
                if *total == 0 {
                    self.owner_bytes.remove(&owner);
                }
            }
        }
    }
    fn evict(&mut self) {
        let mut kept = VecDeque::new();
        while let Some(key) = self.order.pop_front() {
            let Some((owner, _)) = self.loaded.get(&key) else {
                continue;
            };
            if self.bytes > self.limit || self.owner_bytes[owner] > self.per_file_limit {
                self.remove(key);
            } else {
                kept.push_back(key);
            }
        }
        self.order = kept;
    }
    fn touch(&mut self, key: u64) {
        self.order.retain(|&id| id != key);
        if self.loaded.contains_key(&key) {
            self.order.push_back(key);
        }
    }
    fn insert(&mut self, key: u64, owner: u64, text: Arc<String>) {
        self.remove(key);
        self.bytes += text.len();
        *self.owner_bytes.entry(owner).or_default() += text.len();
        self.loaded.insert(key, (owner, text));
        self.touch(key);
        self.evict();
    }
}
impl Chunk {
    fn load(&self, retain: bool) -> io::Result<Arc<String>> {
        let (backing, offset) = {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            loop {
                match &mut *state {
                    ChunkState::Pending(body) | ChunkState::Failed { body, .. } => {
                        return Ok(body.clone());
                    }
                    ChunkState::Committed {
                        backing,
                        offset,
                        resident,
                        loading,
                    } => {
                        if let Some(body) = resident.upgrade() {
                            if retain {
                                cache()
                                    .lock()
                                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                                    .touch(self.id);
                            }
                            return Ok(body);
                        }
                        if *loading {
                            state = self
                                .ready
                                .wait(state)
                                .unwrap_or_else(std::sync::PoisonError::into_inner);
                            continue;
                        }
                        *loading = true;
                        break (backing.clone(), *offset);
                    }
                }
            }
        };
        let result = (|| {
            let mut bytes = vec![0; self.len];
            read_exact_at(&backing.file, &mut bytes, offset)?;
            if blake3::hash(&bytes) != self.digest {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "immutable text chunk digest mismatch",
                ));
            }
            Ok(Arc::new(String::from_utf8(bytes).map_err(|error| {
                io::Error::new(io::ErrorKind::InvalidData, error)
            })?))
        })();
        {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let ChunkState::Committed {
                resident, loading, ..
            } = &mut *state
            {
                *loading = false;
                if let Ok(body) = &result {
                    *resident = Arc::downgrade(body);
                }
            }
            self.ready.notify_all();
        }
        if retain && let Ok(body) = &result {
            cache()
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .insert(self.id, self.owner, body.clone());
        }
        result
    }
}
impl ChunkText {
    #[cfg(test)]
    pub(super) fn truncate_backing_for_test(&self) -> io::Result<()> {
        let Self::Sealed(chunk) = self else {
            return Err(io::Error::other("chunk is still inline"));
        };
        let backing = match &*chunk
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
        {
            ChunkState::Committed { backing, .. } => backing.clone(),
            _ => return Err(io::Error::other("chunk not committed")),
        };
        backing.file.set_len(0)?;
        cache()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(chunk.id);
        Ok(())
    }
    pub(super) fn from_file(
        backing: Arc<Backing>,
        offset: u64,
        len: usize,
        owner: u64,
        digest: blake3::Hash,
    ) -> Self {
        Self::Sealed(Arc::new(Chunk {
            id: new_owner(),
            owner,
            len,
            digest,
            state: Mutex::new(ChunkState::Committed {
                backing,
                offset,
                resident: Weak::new(),
                loading: false,
            }),
            ready: Condvar::new(),
        }))
    }
    pub(super) fn len(&self) -> usize {
        match self {
            Self::Inline(body) => body.len(),
            Self::Sealed(chunk) => chunk.len,
        }
    }
    pub(super) fn spill(&mut self, owner: u64) {
        let Self::Inline(body) = self else { return };
        let body = body.clone();
        let chunk = Arc::new(Chunk {
            id: new_owner(),
            owner,
            len: body.len(),
            digest: blake3::hash(body.as_bytes()),
            state: Mutex::new(ChunkState::Pending(body.clone())),
            ready: Condvar::new(),
        });
        *self = Self::Sealed(chunk.clone());
        match temporary_backing().and_then(|backing| {
            backing
                .append(body.as_bytes())
                .map(|offset| (backing, offset))
        }) {
            Ok((backing, offset)) => {
                *chunk
                    .state
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) = ChunkState::Committed {
                    backing,
                    offset,
                    resident: Arc::downgrade(&body),
                    loading: false,
                };
                cache()
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .insert(chunk.id, owner, body);
            }
            Err(error) => {
                let cause = format!("Text cache could not spill: {error}. Text remains in memory.");
                log::warn!("{cause}");
                *chunk
                    .state
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) = ChunkState::Failed {
                    body,
                    _cause: cause.clone(),
                };
                cache()
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .error = Some(cause);
            }
        }
    }
    pub(super) fn try_load(&self) -> io::Result<Arc<String>> {
        self.try_load_for_scan(true)
    }
    pub(super) fn try_load_for_scan(&self, retain: bool) -> io::Result<Arc<String>> {
        match self {
            Self::Inline(body) => Ok(body.clone()),
            Self::Sealed(chunk) => chunk.load(retain),
        }
    }
    pub(super) fn load(&self) -> Arc<String> {
        self.try_load()
            .expect("immutable text must be preflighted and pinned before infallible access")
    }
}
pub(crate) fn set_limits(limit: usize, per_file_limit: usize) {
    let mut cache = cache()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    cache.limit = limit;
    cache.per_file_limit = per_file_limit;
    cache.evict();
}
pub(crate) fn take_error() -> Option<String> {
    cache()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .error
        .take()
}
pub(super) fn owner_loaded(owner: u64) -> usize {
    cache()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .loaded
        .values()
        .filter(|(id, _)| *id == owner)
        .count()
}
pub(super) fn file_chunk_limit(chunk_bytes: usize) -> usize {
    cache()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .per_file_limit
        / chunk_bytes
}

fn read_exact_at(file: &File, mut bytes: &mut [u8], mut offset: u64) -> io::Result<()> {
    while !bytes.is_empty() {
        #[cfg(unix)]
        let result = std::os::unix::fs::FileExt::read_at(file, bytes, offset);
        #[cfg(windows)]
        let result = std::os::windows::fs::FileExt::seek_read(file, bytes, offset);
        match result {
            Ok(0) => {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "immutable text backing truncated",
                ));
            }
            Ok(count) => {
                offset += count as u64;
                bytes = &mut bytes[count..];
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error),
        }
    }
    Ok(())
}
fn write_all_at(file: &File, mut bytes: &[u8], mut offset: u64) -> io::Result<()> {
    while !bytes.is_empty() {
        #[cfg(unix)]
        let result = std::os::unix::fs::FileExt::write_at(file, bytes, offset);
        #[cfg(windows)]
        let result = std::os::windows::fs::FileExt::seek_write(file, bytes, offset);
        match result {
            Ok(0) => {
                return Err(io::Error::new(
                    io::ErrorKind::WriteZero,
                    "immutable text backing write stopped",
                ));
            }
            Ok(count) => {
                offset += count as u64;
                bytes = &bytes[count..];
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pinned_text_survives_eviction_and_corrupt_reload_is_an_error() {
        let backing = Backing::new(tempfile::tempfile().unwrap());
        let text = "λ\r\n🦀";
        let offset = backing.append(text.as_bytes()).unwrap();
        let chunk = ChunkText::from_file(
            backing.clone(),
            offset,
            text.len(),
            new_owner(),
            blake3::hash(text.as_bytes()),
        );
        let pin = chunk.try_load().unwrap();
        let chunk_id = match &chunk {
            ChunkText::Sealed(chunk) => chunk.id,
            ChunkText::Inline(_) => unreachable!(),
        };
        cache().lock().unwrap().remove(chunk_id);
        backing.file.set_len(0).unwrap();
        assert_eq!(&*chunk.try_load().unwrap(), text);
        drop(pin);
        assert_eq!(
            chunk.try_load().unwrap_err().kind(),
            io::ErrorKind::UnexpectedEof
        );
    }
    #[test]
    fn unified_cache_applies_aggregate_and_owner_limits() {
        let mut cache = Cache {
            limit: 12,
            per_file_limit: 8,
            ..Default::default()
        };
        cache.insert(1, 1, Arc::new("first".into()));
        cache.insert(2, 2, Arc::new("second".into()));
        cache.insert(3, 1, Arc::new("third".into()));
        assert!(cache.bytes <= 12);
        assert!(cache.owner_bytes.values().all(|bytes| *bytes <= 8));
    }
}
