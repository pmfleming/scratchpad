//! Dedicated bounded readers. A panic disconnects replies instead of unwinding a caller.
use super::{ChunkReference, Metadata, failed};
use futures_lite::future::block_on;
use std::{
    io,
    path::Path,
    sync::{Arc, Mutex, mpsc},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

struct Request {
    #[cfg(test)]
    panic: bool,
    metadata: Metadata,
    id: i64,
    deadline: Instant,
    reply: mpsc::SyncSender<io::Result<Arc<String>>>,
}
struct Reader {
    sender: Mutex<Option<mpsc::SyncSender<Request>>>,
    thread: Mutex<Option<JoinHandle<()>>>,
}
impl Reader {
    fn open(path: &Path, name: &str) -> io::Result<Arc<Self>> {
        let (sender, incoming) = mpsc::sync_channel::<Request>(2);
        let (ready, opened) = mpsc::sync_channel(1);
        let path = path.to_owned();
        let thread = thread::Builder::new().name(name.into()).spawn(move || {
            // Includes connection/stat refresh and every query. A panic disconnects
            // each request's private reply rather than crossing the service boundary.
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                block_on(run(path, incoming, ready));
            }));
        })?;
        let reader = Arc::new(Self {
            sender: Mutex::new(Some(sender)),
            thread: Mutex::new(Some(thread)),
        });
        opened
            .recv_timeout(Duration::from_secs(10))
            .map_err(failed)??;
        Ok(reader)
    }
    fn load(&self, reference: &ChunkReference, background: bool) -> io::Result<Arc<String>> {
        let deadline = Instant::now()
            + if background {
                Duration::from_secs(5)
            } else {
                Duration::from_millis(250)
            };
        let sender = self
            .sender
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
            .ok_or_else(|| io::Error::new(io::ErrorKind::BrokenPipe, "chunk reader is closed"))?;
        let (reply, result) = mpsc::sync_channel(1);
        let mut request = Request {
            #[cfg(test)]
            panic: false,
            metadata: reference.metadata.clone(),
            id: reference.id,
            deadline,
            reply,
        };
        loop {
            match sender.try_send(request) {
                Ok(()) => break,
                Err(mpsc::TrySendError::Disconnected(_)) => {
                    return Err(io::Error::new(
                        io::ErrorKind::BrokenPipe,
                        "chunk reader stopped or panicked",
                    ));
                }
                Err(mpsc::TrySendError::Full(queued)) => request = queued,
            }
            if Instant::now() >= deadline {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "chunk reader queue deadline exceeded",
                ));
            }
            thread::sleep(Duration::from_millis(1));
        }
        result
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .map_err(|cause| match cause {
                mpsc::RecvTimeoutError::Timeout => {
                    io::Error::new(io::ErrorKind::TimedOut, "chunk read deadline exceeded")
                }
                mpsc::RecvTimeoutError::Disconnected => io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "chunk reader stopped or panicked",
                ),
            })?
    }
    fn close(&self) {
        self.sender
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
        if let Some(thread) = self
            .thread
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
        {
            let _ = thread.join();
        }
    }
}
impl Drop for Reader {
    fn drop(&mut self) {
        self.close();
    }
}
struct Readers {
    foreground: Arc<Reader>,
    background: Arc<Reader>,
}
/// An epoch can be closed for Windows replacement, then reopened against its backup.
pub struct ReadEpoch {
    pub id: u64,
    reads: std::sync::atomic::AtomicU64,
    readers: Mutex<Option<Readers>>,
    path: Mutex<Option<std::path::PathBuf>>,
}
impl std::fmt::Debug for ReadEpoch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReadEpoch")
            .field("id", &self.id)
            .finish_non_exhaustive()
    }
}
impl ReadEpoch {
    /// Number of body-read requests; metadata reconstruction does not increment it.
    pub fn read_count(&self) -> u64 {
        self.reads.load(std::sync::atomic::Ordering::Relaxed)
    }
    #[cfg(test)]
    pub(crate) fn panic_foreground_for_test(&self) {
        let reader = self
            .readers
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .foreground
            .clone();
        let (reply, result) = mpsc::sync_channel(1);
        reader
            .sender
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .send(Request {
                panic: true,
                id: 0,
                metadata: Metadata::measure(""),
                deadline: Instant::now() + Duration::from_secs(1),
                reply,
            })
            .unwrap();
        assert!(result.recv_timeout(Duration::from_secs(2)).is_err());
    }
    pub(super) fn open(path: &Path, id: u64) -> io::Result<Arc<Self>> {
        let epoch = Arc::new(Self {
            id,
            reads: std::sync::atomic::AtomicU64::new(0),
            readers: Mutex::new(None),
            path: Mutex::new(None),
        });
        epoch.reopen(path)?;
        Ok(epoch)
    }
    pub(crate) fn empty(id: u64) -> Arc<Self> {
        Arc::new(Self {
            id,
            reads: std::sync::atomic::AtomicU64::new(0),
            readers: Mutex::new(None),
            path: Mutex::new(None),
        })
    }
    pub(crate) fn ensure_open(&self) -> io::Result<()> {
        let closed = self
            .readers
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .is_none();
        if closed && let Some(path) = self.path() {
            self.reopen(&path)?;
        }
        Ok(())
    }
    pub(crate) fn path(&self) -> Option<std::path::PathBuf> {
        self.path
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
    pub(crate) fn reopen(&self, path: &Path) -> io::Result<()> {
        let readers = Readers {
            foreground: Reader::open(path, "text-foreground")?,
            background: Reader::open(path, "text-background")?,
        };
        let previous = self
            .readers
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .replace(readers);
        drop(previous);
        *self
            .path
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(path.to_owned());
        Ok(())
    }
    pub(crate) fn close(&self) {
        let readers = self
            .readers
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
        if let Some(readers) = readers {
            readers.foreground.close();
            readers.background.close();
        }
    }
    pub(super) fn load(
        &self,
        reference: &ChunkReference,
        background: bool,
    ) -> io::Result<Arc<String>> {
        self.reads
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let reader = {
            let readers = self
                .readers
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let readers = readers.as_ref().ok_or_else(|| {
                io::Error::new(io::ErrorKind::BrokenPipe, "chunk epoch is closed")
            })?;
            if background {
                readers.background.clone()
            } else {
                readers.foreground.clone()
            }
        };
        reader.load(reference, background)
    }
}
async fn run(
    path: std::path::PathBuf,
    incoming: mpsc::Receiver<Request>,
    ready: mpsc::SyncSender<io::Result<()>>,
) {
    let result = async {
        let database = turso::Builder::new_local(
            path.to_str()
                .ok_or_else(|| failed("chunk store path is not UTF-8"))?,
        )
        .with_io(turso::IoBackend::Syscall)
        .build()
        .await
        .map_err(failed)?;
        let connection = database.connect().map_err(failed)?;
        connection
            .execute_batch(
                "PRAGMA synchronous=FULL; PRAGMA cache_size=-512; PRAGMA foreign_keys=ON;",
            )
            .await
            .map_err(failed)?;
        Ok::<_, io::Error>((database, connection))
    }
    .await;
    let (_database, connection) = match result {
        Ok(pair) => {
            let _ = ready.send(Ok(()));
            pair
        }
        Err(cause) => {
            let _ = ready.send(Err(cause));
            return;
        }
    };
    while let Ok(request) = incoming.recv() {
        #[cfg(test)]
        assert!(!request.panic, "injected chunk reader engine panic");
        let result = loop {
            if Instant::now() >= request.deadline {
                break Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "chunk read deadline exceeded",
                ));
            }
            match read(&connection, request.id, &request.metadata).await {
                Err(turso::Error::Busy(_)) => thread::sleep(Duration::from_millis(1)),
                result => break result.map_err(failed),
            }
        };
        let _ = request.reply.send(result);
    }
}
async fn read(
    connection: &turso::Connection,
    id: i64,
    metadata: &Metadata,
) -> Result<Arc<String>, turso::Error> {
    let body = {
        let mut rows = connection
            .query("SELECT body FROM chunk_bodies WHERE chunk_id=?", [id])
            .await?;
        let row = rows.next().await?.ok_or_else(|| {
            turso::Error::ConversionFailure("missing immutable text chunk".into())
        })?;
        row.get::<Vec<u8>>(0)?
    };
    if body.len() != metadata.byte_len || blake3::hash(&body).as_bytes() != &metadata.digest {
        return Err(turso::Error::ConversionFailure(
            "immutable text chunk length/digest mismatch".into(),
        ));
    }
    let text = String::from_utf8(body)
        .map_err(|cause| turso::Error::ConversionFailure(cause.to_string()))?;
    if text.chars().count() as u64 != metadata.char_len
        || text.bytes().filter(|&b| b == b'\n').count() as u64 != metadata.newline_count
        || text.is_ascii() != metadata.is_ascii
        || text.ends_with('\n') != metadata.ends_with_lf
    {
        return Err(turso::Error::ConversionFailure(
            "immutable text metrics mismatch".into(),
        ));
    }
    Ok(Arc::new(text))
}
