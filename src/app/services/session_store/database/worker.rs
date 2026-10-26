use super::{Database, error};
use futures_lite::future::block_on;
use std::io;
use std::path::PathBuf;
use std::sync::{Arc, mpsc};
use std::thread::{self, JoinHandle};

type Job = Box<dyn FnOnce(&mut Database) + Send>;

#[derive(Clone)]
pub(in crate::app::services::session_store) struct DatabaseWorker(Arc<Worker>);
struct Worker {
    sender: Option<mpsc::SyncSender<Job>>,
    thread: Option<JoinHandle<()>>,
}

impl DatabaseWorker {
    pub fn open(root: PathBuf) -> io::Result<Self> {
        let (sender, receiver) = mpsc::sync_channel::<Job>(2);
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let thread = thread::Builder::new()
            .name("session-database".into())
            .spawn(move || match block_on(Database::open(&root)) {
                Ok(mut database) => {
                    let _ = ready_tx.send(Ok(()));
                    for job in receiver {
                        job(&mut database);
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
            })?;
        let worker = Self(Arc::new(Worker {
            sender: Some(sender),
            thread: Some(thread),
        }));
        ready_rx.recv().map_err(error)??;
        Ok(worker)
    }

    pub fn call<T: Send + 'static>(
        &self,
        operation: impl FnOnce(&mut Database) -> io::Result<T> + Send + 'static,
    ) -> io::Result<T> {
        let (sender, receiver) = mpsc::sync_channel(1);
        let job: Job = Box::new(move |database| {
            let _ = sender.send(operation(database));
        });
        self.0
            .sender
            .as_ref()
            .ok_or_else(|| error("database worker closed"))?
            .send(job)
            .map_err(error)?;
        receiver.recv().map_err(error)?
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
