use super::{Database, error};
use crate::app::services::chunk_store::{ChunkStoreHandle, Health};
use futures_lite::future::block_on;
use std::{
    fs::File,
    io,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

type DatabaseJob = Box<dyn FnOnce(&mut Database, &AtomicBool) + Send>;
enum Job {
    Call(DatabaseJob),
    Drain,
    Close(mpsc::SyncSender<File>),
}
#[derive(Clone)]
pub(in crate::app::services::session_store) struct DatabaseWorker(Arc<Worker>);
struct Worker {
    sender: Option<mpsc::SyncSender<Job>>,
    thread: Option<JoinHandle<()>>,
    notices: Arc<Mutex<Vec<super::super::RestoreStatus>>>,
    blocked: Arc<AtomicBool>,
    chunks: ChunkStoreHandle,
    wake_pending: Arc<AtomicBool>,
}
struct WorkerHealthGuard {
    blocked: Arc<AtomicBool>,
    chunks: Option<ChunkStoreHandle>,
}
impl Drop for WorkerHealthGuard {
    fn drop(&mut self) {
        self.blocked.store(true, Ordering::Release);
        if let Some(store) = &self.chunks {
            store.set_health(Health::Unavailable("Session writer has stopped".into()));
        }
    }
}
struct Actor {
    root: PathBuf,
    ownership: Option<File>,
    receiver: mpsc::Receiver<Job>,
    ready_tx: mpsc::SyncSender<io::Result<ChunkStoreHandle>>,
    worker_notices: Arc<Mutex<Vec<super::super::RestoreStatus>>>,
    worker_blocked: Arc<AtomicBool>,
    worker_wake_pending: Arc<AtomicBool>,
}
impl Actor {
    fn run(self) {
        let Self {
            root,
            ownership,
            receiver,
            ready_tx,
            worker_notices,
            worker_blocked,
            worker_wake_pending,
        } = self;

        let mut health = WorkerHealthGuard {
            blocked: worker_blocked.clone(),
            chunks: None,
        };
        let opened = block_on(async {
            match ownership {
                Some(ownership) => Database::open_owned(&root, ownership).await,
                None => Database::open(&root).await,
            }
        });
        let mut database = match opened {
            Ok(database) => database,
            Err(cause) => {
                let _ = ready_tx.send(Err(cause));
                return;
            }
        };
        let chunks = match database.chunk_store() {
            Ok(chunks) => chunks,
            Err(cause) => {
                let _ = ready_tx.send(Err(cause));
                return;
            }
        };
        health.chunks = Some(chunks.clone());
        *worker_notices
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) =
            std::mem::take(&mut database.startup_notices);
        let _ = ready_tx.send(Ok(chunks.clone()));
        let mut retry_at = Instant::now();
        let mut backoff = Duration::from_millis(100);
        loop {
            let drain = chunks.pending_bytes() > 0
                && !matches!(chunks.health(), Health::Unavailable(_))
                && Instant::now() >= retry_at;
            match receiver.recv_timeout(if drain {
                Duration::ZERO
            } else {
                Duration::from_millis(100)
            }) {
                Ok(Job::Call(job)) => job(&mut database, &worker_blocked),
                Ok(Job::Drain) => {
                    worker_wake_pending.store(false, Ordering::Release);
                    continue;
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Ok(Job::Close(reply)) => {
                    chunks.close_readers();
                    let ownership = database.ownership;
                    drop(database.conn);
                    drop(database.database);
                    let _ = reply.send(ownership);
                    return;
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
            if database.blocked.is_some() {
                chunks.set_health(Health::Unavailable(
                    database.blocked.clone().unwrap_or_default(),
                ));
                continue;
            }
            // One bounded batch before checking queued jobs again. A queued
            // manifest waits behind at most the currently running batch.
            if chunks.pending_bytes() > 0 && Instant::now() >= retry_at {
                match block_on(database.drain_chunk_batch()) {
                    Ok(()) => {
                        backoff = Duration::from_millis(100);
                        retry_at = Instant::now();
                    }
                    Err(cause) => {
                        log::warn!("chunk batch failed: {cause}");
                        retry_at = Instant::now() + backoff;
                        backoff = (backoff * 2).min(Duration::from_secs(5));
                    }
                }
            }
        }
        while chunks.pending_bytes() > 0 && block_on(database.drain_chunk_batch()).is_ok() {}
        chunks.close_readers();
        if database.ensure_healthy().is_ok()
            && let Err(cause) = block_on(database.checkpoint())
        {
            crate::app::diagnostics::record_io_error(
                "session_checkpoint",
                Some(&root),
                "session_store",
                &cause,
            );
        }
    }
}
impl DatabaseWorker {
    pub fn open(root: PathBuf) -> io::Result<Self> {
        Self::spawn(root, None)
    }
    pub fn open_owned(root: PathBuf, ownership: File) -> io::Result<Self> {
        Self::spawn(root, Some(ownership))
    }
    fn spawn(root: PathBuf, ownership: Option<File>) -> io::Result<Self> {
        let (sender, receiver) = mpsc::sync_channel::<Job>(2);
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let notices = Arc::new(Mutex::new(Vec::new()));
        let worker_notices = notices.clone();
        let blocked = Arc::new(AtomicBool::new(false));
        let worker_blocked = blocked.clone();
        let wake_pending = Arc::new(AtomicBool::new(false));
        let worker_wake_pending = wake_pending.clone();
        let thread = thread::Builder::new()
            .name("session-database".into())
            .spawn(move || {
                Actor {
                    root,
                    ownership,
                    receiver,
                    ready_tx,
                    worker_notices,
                    worker_blocked,
                    worker_wake_pending,
                }
                .run();
            })?;
        let chunks = ready_rx.recv().map_err(error)??;
        let worker = Self(Arc::new(Worker {
            sender: Some(sender),
            thread: Some(thread),
            notices,
            blocked,
            chunks: chunks.clone(),
            wake_pending,
        }));
        let weak = Arc::downgrade(&worker.0);
        chunks.set_waker(Arc::new(move || {
            if let Some(worker) = weak.upgrade()
                && let Some(sender) = &worker.sender
                && !worker.wake_pending.swap(true, Ordering::AcqRel)
                && sender.try_send(Job::Drain).is_err()
            {
                worker.wake_pending.store(false, Ordering::Release);
            }
        }));
        Ok(worker)
    }
    pub fn take_notices(&self) -> Vec<super::super::RestoreStatus> {
        std::mem::take(
            &mut *self
                .0
                .notices
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        )
    }
    pub fn writes_blocked(&self) -> bool {
        self.0.blocked.load(Ordering::Acquire)
    }
    pub fn chunk_store(&self) -> ChunkStoreHandle {
        self.0.chunks.clone()
    }
    pub fn call<T: Send + 'static>(
        &self,
        operation: impl FnOnce(&mut Database) -> io::Result<T> + Send + 'static,
    ) -> io::Result<T> {
        let (sender, receiver) = mpsc::sync_channel(1);
        let job = Job::Call(Box::new(move |database, blocked| {
            let result = operation(database);
            blocked.store(database.blocked.is_some(), Ordering::Release);
            let _ = sender.send(result);
        }));
        self.0
            .sender
            .as_ref()
            .ok_or_else(|| error("database worker closed"))?
            .send(job)
            .map_err(error)?;
        receiver.recv().map_err(|cause| {
            self.0.blocked.store(true, Ordering::Release);
            error(cause)
        })?
    }
    pub fn close_for_replacement(&self) -> io::Result<File> {
        let (sender, receiver) = mpsc::sync_channel(1);
        self.0
            .sender
            .as_ref()
            .ok_or_else(|| error("database worker closed"))?
            .send(Job::Close(sender))
            .map_err(error)?;
        receiver.recv().map_err(error)
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.sender.take();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn health_query_does_not_wait_for_a_running_job_and_ack_publishes_latch() {
        let directory = tempfile::tempdir().unwrap();
        let worker = DatabaseWorker::open(directory.path().into()).unwrap();
        let (started, ready) = mpsc::sync_channel(1);
        let (release, permit) = mpsc::sync_channel(1);
        let saving = worker.clone();
        let job = thread::spawn(move || {
            saving.call(move |database| {
                started.send(()).unwrap();
                let _ = permit.recv_timeout(Duration::from_secs(2));
                database.blocked = Some("injected protection latch".into());
                Ok(())
            })
        });
        ready.recv_timeout(Duration::from_secs(5)).unwrap();
        let timer = Instant::now();
        let blocked = worker.writes_blocked();
        let elapsed = timer.elapsed();
        let _ = release.send(());
        job.join().unwrap().unwrap();
        assert!(!blocked);
        assert!(
            elapsed < Duration::from_millis(200),
            "health query waited {elapsed:?}"
        );
        assert!(worker.writes_blocked());
    }
    #[test]
    fn a_queued_job_waits_for_at_most_one_four_chunk_batch() {
        let root = tempfile::tempdir().unwrap();
        let worker = DatabaseWorker::open(root.path().into()).unwrap();
        let store = worker.chunk_store();
        let (started, ready) = mpsc::sync_channel(1);
        let (release, permit) = mpsc::sync_channel(1);
        let held = worker.clone();
        let holding = thread::spawn(move || {
            held.call(move |_| {
                started.send(()).unwrap();
                permit.recv_timeout(Duration::from_secs(5)).unwrap();
                Ok(())
            })
        });
        ready.recv_timeout(Duration::from_secs(5)).unwrap();
        let mut chunks = Vec::new();
        for n in 0..40 {
            chunks.push(store.seal(Arc::new(format!("{n}{}", "x".repeat(256 * 1024 - 2))), 1));
        }
        let (result, measured) = mpsc::sync_channel(1);
        worker
            .0
            .sender
            .as_ref()
            .unwrap()
            .send(Job::Call(Box::new(move |db, _| {
                result
                    .send(block_on(db.integer("SELECT count(*) FROM chunks")))
                    .unwrap();
            })))
            .unwrap();
        release.send(()).unwrap();
        holding.join().unwrap().unwrap();
        let count = measured
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .unwrap();
        assert!(count <= 4, "queued work followed {count} chunks");
        worker
            .call(|db| {
                block_on(async {
                    while db.chunk_store()?.pending_bytes() > 0 {
                        db.drain_chunk_batch().await?;
                    }
                    Ok(())
                })
            })
            .unwrap();
        assert!(chunks.iter().all(|c| c.reference().is_some()));
    }
}
