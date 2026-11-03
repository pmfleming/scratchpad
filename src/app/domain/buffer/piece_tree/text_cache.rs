//! Disposable disk backing for immutable text chunks. The session database remains
//! the durable source. A failed spill retains its in-memory copy and reports it.
use std::{
    collections::{HashMap, VecDeque},
    fs::File,
    io::{self, Read, Seek, SeekFrom, Write},
    sync::{Arc, Mutex, OnceLock},
};

const DEFAULT_LIMIT: usize = 16 * 1024 * 1024;

#[derive(Clone, Debug)]
pub(super) enum ChunkText {
    Inline(Arc<String>),
    Stored { offset: u64, len: usize, owner: u64 },
}

#[derive(Default)]
struct Cache {
    file: Option<File>,
    end: u64,
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
            let owner = self.loaded[&key].0;
            if self.bytes > self.limit || self.owner_bytes[&owner] > self.per_file_limit {
                self.remove(key);
            } else {
                kept.push_back(key);
            }
        }
        self.order = kept;
    }
    fn insert(&mut self, key: u64, owner: u64, text: Arc<String>) {
        self.remove(key);
        self.bytes += text.len();
        *self.owner_bytes.entry(owner).or_default() += text.len();
        self.loaded.insert(key, (owner, text));
        self.order.retain(|&entry| entry != key);
        self.order.push_back(key);
        self.evict();
    }
    fn spill(&mut self, text: &str) -> io::Result<u64> {
        if self.file.is_none() {
            self.file = Some(tempfile::tempfile()?);
        }
        let file = self
            .file
            .as_mut()
            .ok_or_else(|| io::Error::other("missing history cache"))?;
        let offset = self.end;
        file.seek(SeekFrom::Start(offset))?;
        file.write_all(text.as_bytes())?;
        self.end += text.len() as u64;
        Ok(offset)
    }
    fn read(&mut self, offset: u64, len: usize, owner: u64) -> io::Result<Arc<String>> {
        if let Some((_, text)) = self.loaded.get(&offset).cloned() {
            self.order.retain(|&entry| entry != offset);
            self.order.push_back(offset);
            return Ok(text);
        }
        let file = self
            .file
            .as_mut()
            .ok_or_else(|| io::Error::other("missing history cache"))?;
        file.seek(SeekFrom::Start(offset))?;
        let mut bytes = vec![0; len];
        file.read_exact(&mut bytes)?;
        let text = Arc::new(String::from_utf8(bytes).map_err(io::Error::other)?);
        self.insert(offset, owner, text.clone());
        Ok(text)
    }
}

impl ChunkText {
    pub(super) fn len(&self) -> usize {
        match self {
            Self::Inline(text) => text.len(),
            Self::Stored { len, .. } => *len,
        }
    }
    pub(super) fn spill(&mut self, owner: u64) {
        let Self::Inline(text) = self else {
            return;
        };
        let mut cache = cache()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        match cache.spill(text) {
            Ok(offset) => {
                let len = text.len();
                cache.insert(offset, owner, text.clone());
                *self = Self::Stored { offset, len, owner };
            }
            Err(error) => {
                let message = format!(
                    "History memory cache could not spill to disk: {error}. Text remains in memory."
                );
                log::warn!("{message}");
                cache.error = Some(message);
            }
        }
    }
    pub(super) fn try_load(&self) -> io::Result<Arc<String>> {
        match self {
            Self::Inline(text) => Ok(text.clone()),
            Self::Stored { offset, len, owner } => cache()
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .read(*offset, *len, *owner),
        }
    }
    pub(super) fn load(&self) -> Arc<String> {
        // Like file-backed originals, infallible painting assumes backing storage
        // stays readable. History seeks preflight their spans through try_load.
        self.try_load().expect("read immutable history text cache")
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn evicted_chunks_reload_exact_unicode_bytes_and_clones_share_backing() {
        // An isolated cache avoids changing the application-wide test budget.
        let mut cache = Cache {
            limit: 8,
            per_file_limit: 8,
            ..Default::default()
        };
        let first = cache.spill("λ\r\n🦀").unwrap();
        let second = cache.spill("another text").unwrap();
        assert_eq!(&*cache.read(first, 8, 1).unwrap(), "λ\r\n🦀");
        assert_eq!(&*cache.read(second, 12, 1).unwrap(), "another text");
        assert!(cache.bytes <= 8);
        assert_eq!(&*cache.read(first, 8, 1).unwrap(), "λ\r\n🦀");
    }
}
