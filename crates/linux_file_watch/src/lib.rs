//! Asynchronous, nonrecursive directory invalidation hints backed by inotify.
//!
//! Notifications tell the caller to recheck disk state; they are not a complete
//! file-operation log and do not contain file contents. See [`FileWatchService`]
//! for startup, root replacement and shutdown semantics.

use rustix::fd::AsFd;
use rustix::fs::inotify::{self, CreateFlags, ReadFlags, Reader, WatchFlags};
use rustix::io::Errno;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::mem::MaybeUninit;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

/// A disk-state invalidation hint or a watcher failure.
#[derive(Clone, Debug)]
pub enum FileWatchEvent {
    /// Recheck files in this watched directory, not just a particular filename.
    ///
    /// Events may be repeated or coalesced. Queue overflow also invalidates all
    /// currently watched directories, so this is not proof of a specific edit.
    DirectoryChanged(
        /// The watched directory whose disk state should be rechecked.
        PathBuf,
    ),
    /// A watch could not be installed, or the worker could not start or continue.
    ///
    /// Missing-root failures are retried, with repeated errors suppressed until
    /// recovery or removal from the desired roots. Worker failures are not
    /// automatically restarted by this service instance.
    WatchError {
        /// The failed root, or `None` for a worker-wide startup/read failure.
        path: Option<PathBuf>,
        /// A human-readable error, not a stable machine-readable error code.
        message: String,
    },
}

enum WatchCommand {
    SetDirectories(BTreeSet<PathBuf>),
    Stop,
}

/// Owns a background worker that watches the desired directory roots.
///
/// Root updates and notifications are asynchronous. Callers must verify disk
/// state themselves and must not infer successful startup or complete event
/// delivery from an empty event queue. Dropping the service requests shutdown
/// without joining the worker; queued events are then discarded.
///
/// ```
/// use linux_file_watch::{FileWatchEvent, FileWatchService};
/// use std::collections::BTreeSet;
///
/// let mut watcher = FileWatchService::new();
/// watcher.set_watched_directories(BTreeSet::new());
/// for event in watcher.drain_events() {
///     match event {
///         FileWatchEvent::DirectoryChanged(directory) => {
///             // Schedule disk-state checks for files in this directory.
///             let _ = directory;
///         }
///         FileWatchEvent::WatchError { path, message } => {
///             eprintln!("{path:?}: {message}");
///         }
///     }
/// }
/// ```
pub struct FileWatchService {
    command_tx: Sender<WatchCommand>,
    event_rx: Receiver<FileWatchEvent>,
    watched_dirs: BTreeSet<PathBuf>,
}

impl FileWatchService {
    /// Starts an initially rootless watcher without waiting for worker startup.
    ///
    /// Thread-creation and inotify-startup failures are delivered as
    /// [`FileWatchEvent::WatchError`] with no path through [`Self::drain_events`],
    /// rather than returned from this constructor.
    pub fn new() -> Self {
        let (command_tx, command_rx) = mpsc::channel();
        let (event_tx, event_rx) = mpsc::channel();
        let startup_errors = event_tx.clone();
        let spawned = std::thread::Builder::new()
            .name("scratchpad-file-watch".to_owned())
            .spawn(move || {
                if let Err(error) = linux_watch_loop(command_rx, &event_tx) {
                    report_error(&event_tx, None, error);
                }
            });
        if let Err(error) = spawned {
            report_error(&startup_errors, None, error);
        }
        Self {
            command_tx,
            event_rx,
            watched_dirs: BTreeSet::new(),
        }
    }

    /// Replaces, rather than appends to, the desired nonrecursive watch roots.
    ///
    /// An identical set is a no-op. Returning does not acknowledge installation:
    /// the worker applies the update asynchronously and retries missing roots.
    /// Events already queued for removed roots may still be drained afterward.
    pub fn set_watched_directories(&mut self, dirs: BTreeSet<PathBuf>) {
        if self.watched_dirs != dirs {
            let _ = self
                .command_tx
                .send(WatchCommand::SetDirectories(dirs.clone()));
            self.watched_dirs = dirs;
        }
    }

    /// Removes and returns queued events without waiting for a new event.
    ///
    /// An empty result is not a worker-health or disk-freshness guarantee. Future
    /// changes and asynchronous installation errors may arrive on a later call.
    pub fn drain_events(&mut self) -> Vec<FileWatchEvent> {
        self.event_rx.try_iter().collect()
    }
}

impl Default for FileWatchService {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for FileWatchService {
    fn drop(&mut self) {
        let _ = self.command_tx.send(WatchCommand::Stop);
    }
}

const WATCH_FLAGS: WatchFlags = WatchFlags::MODIFY
    .union(WatchFlags::ATTRIB)
    .union(WatchFlags::CLOSE_WRITE)
    .union(WatchFlags::MOVED_FROM)
    .union(WatchFlags::MOVED_TO)
    .union(WatchFlags::CREATE)
    .union(WatchFlags::DELETE)
    .union(WatchFlags::DELETE_SELF)
    .union(WatchFlags::MOVE_SELF);
const INVALIDATED: ReadFlags = ReadFlags::IGNORED
    .union(ReadFlags::DELETE_SELF)
    .union(ReadFlags::MOVE_SELF)
    .union(ReadFlags::UNMOUNT);

#[derive(Default)]
struct Watches {
    by_wd: HashMap<i32, PathBuf>,
    desired: BTreeSet<PathBuf>,
    reported_failures: BTreeSet<PathBuf>,
}

impl Watches {
    fn configure(&mut self, fd: impl AsFd, dirs: BTreeSet<PathBuf>) {
        self.desired = dirs;
        self.by_wd.retain(|wd, dir| {
            if self.desired.contains(dir) {
                return true;
            }
            let _ = inotify::remove_watch(&fd, *wd);
            false
        });
        self.reported_failures
            .retain(|dir| self.desired.contains(dir));
    }

    fn retry_missing(&mut self, fd: impl AsFd, events: &Sender<FileWatchEvent>) {
        // Borrow paths during discovery; clone only when a new watch/error owns one.
        let missing: Vec<_> = {
            let watched: HashSet<_> = self.by_wd.values().collect();
            self.desired
                .iter()
                .filter(|dir| !watched.contains(dir))
                .collect()
        };
        for dir in missing {
            match inotify::add_watch(&fd, dir, WATCH_FLAGS) {
                Ok(wd) => {
                    self.by_wd.insert(wd, dir.clone());
                    self.reported_failures.remove(dir);
                }
                Err(error) if self.reported_failures.insert(dir.clone()) => {
                    report_error(events, Some(dir.clone()), error)
                }
                Err(_) => {}
            }
        }
    }

    fn dispatch(&mut self, wd: i32, mask: ReadFlags, events: &Sender<FileWatchEvent>) {
        if mask.contains(ReadFlags::QUEUE_OVERFLOW) {
            for dir in self.by_wd.values() {
                let _ = events.send(FileWatchEvent::DirectoryChanged(dir.clone()));
            }
        } else if mask
            .intersects(ReadFlags::from_bits_retain(WATCH_FLAGS.bits()) | ReadFlags::UNMOUNT)
            && let Some(dir) = self.by_wd.get(&wd)
        {
            let _ = events.send(FileWatchEvent::DirectoryChanged(dir.clone()));
        }
        if mask.intersects(INVALIDATED) {
            self.by_wd.remove(&wd);
        }
    }

    fn drain(
        &mut self,
        reader: &mut Reader<'_, impl AsFd>,
        events: &Sender<FileWatchEvent>,
    ) -> Result<(), Errno> {
        loop {
            match reader.next() {
                Ok(event) => self.dispatch(event.wd(), event.events(), events),
                Err(Errno::INTR) => continue,
                Err(Errno::AGAIN) => return Ok(()),
                Err(error) => return Err(error),
            }
        }
    }
}

fn linux_watch_loop(
    commands: Receiver<WatchCommand>,
    events: &Sender<FileWatchEvent>,
) -> Result<(), Errno> {
    let fd = inotify::init(CreateFlags::NONBLOCK | CreateFlags::CLOEXEC)?;
    // Reader owns initialization/alignment checks; no application-side FFI or casts.
    let mut buffer = [MaybeUninit::uninit(); 16 * 1024];
    let mut reader = Reader::new(&fd, &mut buffer);
    let mut watches = Watches::default();
    loop {
        match commands.recv_timeout(Duration::from_millis(250)) {
            Ok(WatchCommand::SetDirectories(dirs)) => watches.configure(&fd, dirs),
            Ok(WatchCommand::Stop) | Err(mpsc::RecvTimeoutError::Disconnected) => return Ok(()),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
        watches.retry_missing(&fd, events);
        watches.drain(&mut reader, events)?;
    }
}

fn report_error(
    events: &Sender<FileWatchEvent>,
    path: Option<PathBuf>,
    error: impl std::fmt::Display,
) {
    let _ = events.send(FileWatchEvent::WatchError {
        path,
        message: error.to_string(),
    });
}

#[cfg(test)]
mod tests {
    use super::{CreateFlags, FileWatchEvent, FileWatchService, ReadFlags, Watches, inotify};
    use std::{
        collections::BTreeSet,
        fs,
        path::Path,
        sync::mpsc,
        thread,
        time::{Duration, Instant},
    };

    #[test]
    fn watcher_reports_changes_recovers_recreated_directories_and_replaces_roots() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("watched");
        fs::create_dir(&dir).unwrap();
        let mut service = FileWatchService::new();
        service.set_watched_directories(BTreeSet::from([dir.clone()]));
        wait_for(
            &mut service,
            || {
                fs::write(dir.join("probe"), "initial").unwrap();
            },
            |event| changed(event, &dir),
        );
        fs::remove_dir_all(&dir).unwrap();
        wait_for(
            &mut service,
            || {},
            |event| matches!(event, FileWatchEvent::WatchError { path: Some(path), .. } if path == &dir),
        );
        fs::create_dir(&dir).unwrap();
        wait_for(
            &mut service,
            || {
                fs::write(dir.join("probe"), "recreated").unwrap();
            },
            |event| changed(event, &dir),
        );
        service.set_watched_directories(BTreeSet::from([root.path().to_owned()]));
        wait_for(
            &mut service,
            || {
                fs::write(root.path().join("other"), "replacement").unwrap();
            },
            |event| changed(event, root.path()),
        );
    }

    #[test]
    fn overflow_invalidation_and_missing_path_errors_preserve_watch_contract() {
        let root = tempfile::tempdir().unwrap();
        let fd = inotify::init(CreateFlags::NONBLOCK | CreateFlags::CLOEXEC).unwrap();
        let (events, received) = mpsc::channel();
        let mut watches = Watches::default();
        let missing = root.path().join("missing");
        watches.configure(
            &fd,
            BTreeSet::from([root.path().to_owned(), missing.clone()]),
        );
        watches.retry_missing(&fd, &events);
        assert!(
            matches!(received.try_recv().unwrap(), FileWatchEvent::WatchError { path: Some(path), .. } if path == missing)
        );
        watches.retry_missing(&fd, &events);
        assert!(
            received.try_recv().is_err(),
            "missing path failures must not flood the UI"
        );
        let wd = *watches.by_wd.keys().next().unwrap();
        watches.dispatch(wd, ReadFlags::ACCESS, &events);
        assert!(received.try_recv().is_err());
        watches.dispatch(-1, ReadFlags::QUEUE_OVERFLOW, &events);
        assert!(changed(&received.try_recv().unwrap(), root.path()));
        watches.dispatch(wd, ReadFlags::IGNORED, &events);
        assert!(watches.by_wd.is_empty());
        watches.retry_missing(&fd, &events);
        assert_eq!(watches.by_wd.len(), 1);
        watches.configure(&fd, BTreeSet::new());
        assert!(watches.by_wd.is_empty());
        assert!(watches.reported_failures.is_empty());
    }

    fn changed(event: &FileWatchEvent, dir: &Path) -> bool {
        matches!(event, FileWatchEvent::DirectoryChanged(path) if path == dir)
    }

    fn wait_for(
        service: &mut FileWatchService,
        mut trigger: impl FnMut(),
        matches: impl Fn(&FileWatchEvent) -> bool,
    ) {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            trigger();
            if service.drain_events().iter().any(&matches) {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "watcher did not deliver expected event"
            );
            thread::sleep(Duration::from_millis(50));
        }
    }
}
