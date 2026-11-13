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
    Close(mpsc::SyncSender<io::Result<File>>),
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
    reset_documents:
        Mutex<std::collections::HashMap<String, crate::app::domain::buffer::HistorySnapshot>>,
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
        let (mut database, chunks) = match open_database(&root, ownership) {
            Ok(opened) => opened,
            Err(cause) => {
                let _ = ready_tx.send(Err(cause));
                return;
            }
        };
        health.chunks = Some(chunks.clone());
        publish_notices(&mut database, &worker_notices);
        let _ = ready_tx.send(Ok(chunks.clone()));
        let mut retry = Retry::new();
        loop {
            chunks.refresh_headroom();
            chunks.reap_abandoned();
            let drain = chunks.pending_bytes() > 0
                && !matches!(chunks.health(), Health::Unavailable(_))
                && retry.due();
            let wait = if drain { Duration::ZERO } else { IDLE_POLL };
            match receiver.recv_timeout(wait) {
                Ok(Job::Call(job)) => job(&mut database, &worker_blocked),
                Ok(Job::Drain) => {
                    worker_wake_pending.store(false, Ordering::Release);
                    continue;
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Ok(Job::Close(reply)) => {
                    if let Err(cause) =
                        block_on(database.checkpoint_for_replacement(&chunks, &root))
                    {
                        let _ = reply.send(Err(cause));
                        continue;
                    }
                    let ownership = database.release_for_replacement();
                    drop(health);
                    let _ = reply.send(Ok(ownership));
                    return;
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
            publish_notices(&mut database, &worker_notices);
            if let Some(cause) = &database.blocked {
                chunks.set_health(Health::Unavailable(cause.clone()));
                continue;
            }
            maintain_chunks(&mut database, &chunks, &mut retry);
        }
        finish_on_disconnect(&mut database, &chunks, &root);
    }
}

const IDLE_POLL: Duration = Duration::from_millis(100);
const MAX_BACKOFF: Duration = Duration::from_secs(5);

fn open_database(
    root: &std::path::Path,
    ownership: Option<File>,
) -> io::Result<(Database, ChunkStoreHandle)> {
    let database = block_on(async {
        match ownership {
            Some(ownership) => Database::open_owned(root, ownership).await,
            None => Database::open(root).await,
        }
    })?;
    let chunks = database.chunk_store()?;
    Ok((database, chunks))
}

/// When background chunk work may next run, backing off after failures.
struct Retry {
    at: Instant,
    backoff: Duration,
}

impl Retry {
    fn new() -> Self {
        Self {
            at: Instant::now(),
            backoff: IDLE_POLL,
        }
    }
    fn due(&self) -> bool {
        Instant::now() >= self.at
    }
    fn succeeded(&mut self) {
        self.backoff = IDLE_POLL;
        self.at = Instant::now();
    }
    fn failed(&mut self) {
        self.postpone();
        self.backoff = (self.backoff * 2).min(MAX_BACKOFF);
    }
    fn postpone(&mut self) {
        self.at = Instant::now() + self.backoff;
    }
}

/// Runs at most one bounded batch of pending chunk writes, or idle collection
/// when nothing is pending. A queued manifest waits behind at most this batch.
fn maintain_chunks(database: &mut Database, chunks: &ChunkStoreHandle, retry: &mut Retry) {
    if chunks.pending_bytes() > 0 && retry.due() {
        match block_on(database.drain_chunk_batch()) {
            Ok(()) => retry.succeeded(),
            Err(cause) => {
                log::warn!("chunk batch failed: {cause}");
                retry.failed();
            }
        }
    }
    if chunks.pending_bytes() == 0
        && retry.due()
        && let Err(cause) = block_on(database.collect_idle_chunk_batch(Instant::now()))
    {
        chunks.set_health(Health::Failing(cause.to_string()));
        retry.postpone();
    }
}
impl Database {
    async fn checkpoint_for_replacement(
        &self,
        chunks: &ChunkStoreHandle,
        root: &std::path::Path,
    ) -> io::Result<()> {
        chunks.close_readers();
        if let Err(cause) = self.checkpoint().await {
            return match chunks.epoch().reopen(&root.join("session.db")) {
                Ok(()) => Err(cause),
                Err(reopen) => Err(error(format!(
                    "Could not checkpoint the previous session: {cause}. Could not reopen its readers: {reopen}"
                ))),
            };
        }
        Ok(())
    }
    fn release_for_replacement(self) -> File {
        // Cached statements own connection clones. Consume the entire Database
        // before returning its lock so no file handle outlives the close reply.
        drop(self.statements);
        drop(self.chunks);
        drop(self.conn);
        drop(self.database);
        self.ownership
    }
}
fn finish_on_disconnect(
    database: &mut Database,
    chunks: &ChunkStoreHandle,
    root: &std::path::Path,
) {
    while chunks.pending_bytes() > 0 && block_on(database.drain_chunk_batch()).is_ok() {}
    if database.ensure_healthy().is_ok()
        && let Err(cause) = block_on(database.checkpoint())
    {
        crate::app::diagnostics::record_io_error(
            "session_checkpoint",
            Some(root),
            "session_store",
            &cause,
        );
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
            reset_documents: Mutex::default(),
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
    pub(super) fn prepare_reset_request(
        &self,
        mut request: super::super::SessionPersistRequest,
    ) -> io::Result<super::super::SessionPersistRequest> {
        let mut cached = self
            .0
            .reset_documents
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        for tab in request.all_tabs_mut() {
            tab.restore_cached_histories(&cached);
        }
        if !request.all_tabs().any(|tab| tab.preserve_existing) {
            return Ok(request);
        }
        let request = self.call(move |db| block_on(db.hydrate_reset_request(request)))?;
        let histories = request
            .all_tabs()
            .flat_map(|tab| &tab.session_tab.buffers)
            .filter_map(|buffer| Some((buffer.temp_id.clone(), buffer.history_capture.clone()?)));
        cached.extend(histories);
        Ok(request)
    }
    pub fn call<T: Send + 'static>(
        &self,
        operation: impl FnOnce(&mut Database) -> io::Result<T> + Send + 'static,
    ) -> io::Result<T> {
        let (sender, receiver) = mpsc::sync_channel(1);
        let chunks = self.0.chunks.clone();
        let job = Job::Call(Box::new(move |database, blocked| {
            let result = operation(database);
            blocked.store(database.blocked.is_some(), Ordering::Release);
            if let Some(cause) = &database.blocked {
                chunks.set_health(Health::Unavailable(cause.clone()));
            }
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
        receiver.recv().map_err(error)?
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
    use super::{Database, DatabaseWorker, Health, Job, block_on, error};
    use std::sync::{Arc, mpsc};
    use std::thread;
    use std::time::{Duration, Instant};

    #[test]
    fn replacement_reply_retires_the_worker_and_keeps_the_session_locked() {
        let root = tempfile::tempdir().unwrap();
        let worker = DatabaseWorker::open(root.path().into()).unwrap();
        worker
            .call(|db| {
                block_on(db.integer("SELECT 42"))?;
                assert!(!db.statements.lock().unwrap().is_empty());
                Ok(())
            })
            .unwrap();
        let ownership = worker.close_for_replacement().unwrap();
        assert!(worker.writes_blocked());
        assert!(matches!(
            worker.chunk_store().health(),
            Health::Unavailable(_)
        ));
        let other = super::super::private_file(&root.path().join("session.db.lock")).unwrap();
        assert!(other.try_lock().is_err());
        assert!(worker.call(|_| Ok(())).is_err());

        #[cfg(target_os = "linux")]
        {
            let root = root.path().canonicalize().unwrap();
            let main = root.join("session.db");
            let wal = root.join("session.db-wal");
            for entry in std::fs::read_dir("/proc/self/fd").unwrap().flatten() {
                if let Ok(path) = std::fs::read_link(entry.path()) {
                    assert!(
                        path != main && path != wal,
                        "database handle still open: {}",
                        path.display()
                    );
                }
            }
        }
        drop(ownership);
        other.try_lock().unwrap();
    }

    #[test]
    fn replacement_checkpoints_committed_data_into_the_main_file() {
        let root = tempfile::tempdir().unwrap();
        let worker = DatabaseWorker::open(root.path().into()).unwrap();
        worker
            .call(|db| {
                block_on(async {
                    db.conn
                        .execute_batch("PRAGMA wal_autocheckpoint=0; CREATE TABLE checkpoint_probe(value TEXT); INSERT INTO checkpoint_probe VALUES ('committed');")
                        .await
                        .map_err(error)
                })
            })
            .unwrap();
        assert!(
            std::fs::metadata(root.path().join("session.db-wal"))
                .unwrap()
                .len()
                > 0
        );
        let _ownership = worker.close_for_replacement().unwrap();
        let copy = tempfile::tempdir().unwrap();
        std::fs::copy(
            root.path().join("session.db"),
            copy.path().join("session.db"),
        )
        .unwrap();
        block_on(async {
            let db = Database::open(copy.path()).await.unwrap();
            assert_eq!(
                db.integer("SELECT count(*) FROM checkpoint_probe WHERE value='committed'")
                    .await
                    .unwrap(),
                1
            );
        });
    }

    #[test]
    fn failed_replacement_checkpoint_keeps_the_worker_and_readers_usable() {
        let root = tempfile::tempdir().unwrap();
        let worker = DatabaseWorker::open(root.path().into()).unwrap();
        let chunks = worker.chunk_store();
        let chunk = chunks.seal(Arc::new("retained text".into()), 1);
        worker
            .call(|db| {
                block_on(async {
                    db.drain_chunk_batch().await?;
                    db.conn
                        .execute("BEGIN IMMEDIATE", ())
                        .await
                        .map_err(error)?;
                    Ok(())
                })
            })
            .unwrap();
        chunks.set_limits(0, 0);
        assert!(worker.close_for_replacement().is_err());
        assert!(!worker.writes_blocked());
        assert_eq!(chunk.load(false, false).unwrap().as_str(), "retained text");
        worker
            .call(|db| {
                block_on(db.conn.execute("ROLLBACK", ()))
                    .map(|_| ())
                    .map_err(error)
            })
            .unwrap();
        let _ownership = worker.close_for_replacement().unwrap();
    }

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

fn publish_notices(database: &mut Database, notices: &Mutex<Vec<super::super::RestoreStatus>>) {
    if !database.startup_notices.is_empty() {
        notices
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .append(&mut database.startup_notices);
    }
}
