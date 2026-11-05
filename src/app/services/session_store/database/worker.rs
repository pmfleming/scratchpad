use super::{Database, error};
use futures_lite::future::block_on;
use std::fs::File;
use std::io;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread::{self, JoinHandle};

type DatabaseJob = Box<dyn FnOnce(&mut Database, &AtomicBool) + Send>;

enum Job {
    Call(DatabaseJob),
    Close(mpsc::SyncSender<File>),
}

#[derive(Clone)]
pub(in crate::app::services::session_store) struct DatabaseWorker(Arc<Worker>);
struct Worker {
    sender: Option<mpsc::SyncSender<Job>>,
    thread: Option<JoinHandle<()>>,
    notices: Arc<Mutex<Vec<super::super::RestoreStatus>>>,
    blocked: Arc<AtomicBool>,
}

struct WorkerHealthGuard(Arc<AtomicBool>);
impl Drop for WorkerHealthGuard {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
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
        let thread = thread::Builder::new()
            .name("session-database".into())
            .spawn(move || {
                let _health = WorkerHealthGuard(worker_blocked.clone());
                let opened = block_on(async {
                    match ownership {
                        Some(ownership) => Database::open_owned(&root, ownership).await,
                        None => Database::open(&root).await,
                    }
                });
                match opened {
                    Ok(mut database) => {
                        *worker_notices.lock().unwrap() =
                            std::mem::take(&mut database.startup_notices);
                        let _ = ready_tx.send(Ok(()));
                        for job in receiver {
                            match job {
                                Job::Call(job) => job(&mut database, &worker_blocked),
                                Job::Close(reply) => {
                                    // Close handles before moving files, retaining exclusive
                                    // ownership throughout backup and replacement.
                                    let ownership = database.ownership;
                                    drop(database.conn);
                                    drop(database.database);
                                    let _ = reply.send(ownership);
                                    return;
                                }
                            }
                        }
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
                    Err(cause) => {
                        let _ = ready_tx.send(Err(cause));
                    }
                }
            })?;
        let worker = Self(Arc::new(Worker {
            sender: Some(sender),
            thread: Some(thread),
            notices,
            blocked,
        }));
        ready_rx.recv().map_err(error)??;
        Ok(worker)
    }

    pub fn take_notices(&self) -> Vec<super::super::RestoreStatus> {
        std::mem::take(&mut *self.0.notices.lock().unwrap())
    }

    pub fn writes_blocked(&self) -> bool {
        self.0.blocked.load(Ordering::Acquire)
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
        // Close before joining so the worker can drain and release the file lock.
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
}
