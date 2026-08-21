use std::collections::{BTreeSet, HashMap};
use std::ffi::{CString, c_char, c_int, c_void};
use std::io;
use std::mem::size_of;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

#[derive(Clone, Debug)]
pub enum FileWatchEvent {
    DirectoryChanged(PathBuf),
    WatchError {
        path: Option<PathBuf>,
        message: String,
    },
}

enum WatchCommand {
    SetDirectories(Vec<PathBuf>),
    Stop,
}

pub struct FileWatchService {
    command_tx: Sender<WatchCommand>,
    event_rx: Receiver<FileWatchEvent>,
    watched_dirs: BTreeSet<PathBuf>,
}

impl FileWatchService {
    pub fn new() -> Self {
        let (command_tx, command_rx) = mpsc::channel();
        let (event_tx, event_rx) = mpsc::channel();
        spawn_watch_thread(command_rx, event_tx);
        Self {
            command_tx,
            event_rx,
            watched_dirs: BTreeSet::new(),
        }
    }

    pub fn set_watched_directories(&mut self, dirs: BTreeSet<PathBuf>) {
        if self.watched_dirs == dirs {
            return;
        }
        self.watched_dirs = dirs.clone();
        let _ = self
            .command_tx
            .send(WatchCommand::SetDirectories(dirs.into_iter().collect()));
    }

    pub fn drain_events(&mut self) -> Vec<FileWatchEvent> {
        let mut events = Vec::new();
        while let Ok(event) = self.event_rx.try_recv() {
            events.push(event);
        }
        events
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

fn spawn_watch_thread(command_rx: Receiver<WatchCommand>, event_tx: Sender<FileWatchEvent>) {
    std::thread::Builder::new()
        .name("scratchpad-file-watch".to_owned())
        .spawn(move || linux_watch_loop(command_rx, event_tx))
        .expect("file watch thread should start");
}

const IN_MODIFY: u32 = 0x0000_0002;
const IN_ATTRIB: u32 = 0x0000_0004;
const IN_CLOSE_WRITE: u32 = 0x0000_0008;
const IN_MOVED_FROM: u32 = 0x0000_0040;
const IN_MOVED_TO: u32 = 0x0000_0080;
const IN_CREATE: u32 = 0x0000_0100;
const IN_DELETE: u32 = 0x0000_0200;
const IN_DELETE_SELF: u32 = 0x0000_0400;
const IN_MOVE_SELF: u32 = 0x0000_0800;
const IN_UNMOUNT: u32 = 0x0000_2000;
const IN_Q_OVERFLOW: u32 = 0x0000_4000;
const IN_IGNORED: u32 = 0x0000_8000;

const IN_NONBLOCK: c_int = 0o0004000;
const IN_CLOEXEC: c_int = 0o2000000;

const EAGAIN: i32 = 11;
const EINTR: i32 = 4;

#[repr(C)]
#[derive(Clone, Copy)]
struct InotifyEventRaw {
    wd: c_int,
    mask: u32,
    cookie: u32,
    len: u32,
}

unsafe extern "C" {
    fn inotify_init1(flags: c_int) -> c_int;
    fn inotify_add_watch(fd: c_int, pathname: *const c_char, mask: u32) -> c_int;
    fn inotify_rm_watch(fd: c_int, wd: c_int) -> c_int;
    fn read(fd: c_int, buf: *mut c_void, count: usize) -> isize;
    fn close(fd: c_int) -> c_int;
}

fn linux_watch_loop(command_rx: Receiver<WatchCommand>, event_tx: Sender<FileWatchEvent>) {
    const WAIT_TIMEOUT: Duration = Duration::from_millis(250);
    const EVENT_BUFFER_SIZE: usize = 16 * 1024;

    struct InotifyFd(c_int);

    impl Drop for InotifyFd {
        fn drop(&mut self) {
            unsafe {
                close(self.0);
            }
        }
    }

    #[derive(Default)]
    struct Watches {
        by_wd: HashMap<c_int, PathBuf>,
        desired: BTreeSet<PathBuf>,
        reported_failures: BTreeSet<PathBuf>,
    }

    impl Watches {
        fn rebuild(&mut self, fd: c_int, dirs: Vec<PathBuf>, event_tx: &Sender<FileWatchEvent>) {
            for wd in self.by_wd.keys().copied().collect::<Vec<_>>() {
                unsafe {
                    inotify_rm_watch(fd, wd);
                }
            }
            self.by_wd.clear();
            self.desired = dirs.into_iter().collect();
            self.reported_failures
                .retain(|dir| self.desired.contains(dir));
            self.retry_missing(fd, event_tx);
        }

        fn retry_missing(&mut self, fd: c_int, event_tx: &Sender<FileWatchEvent>) {
            let watched = self.by_wd.values().cloned().collect::<BTreeSet<_>>();
            for dir in self
                .desired
                .difference(&watched)
                .cloned()
                .collect::<Vec<_>>()
            {
                let Some(c_path) = path_to_c_string(&dir) else {
                    self.report_failure(
                        event_tx,
                        dir,
                        "watch path contains an interior null byte".to_owned(),
                    );
                    continue;
                };
                let wd = unsafe { inotify_add_watch(fd, c_path.as_ptr(), watch_mask()) };
                if wd >= 0 {
                    self.by_wd.insert(wd, dir.clone());
                    self.reported_failures.remove(&dir);
                } else {
                    self.report_failure(event_tx, dir, io::Error::last_os_error().to_string());
                }
            }
        }

        fn report_failure(
            &mut self,
            event_tx: &Sender<FileWatchEvent>,
            dir: PathBuf,
            message: String,
        ) {
            if self.reported_failures.insert(dir.clone()) {
                let _ = event_tx.send(FileWatchEvent::WatchError {
                    path: Some(dir),
                    message,
                });
            }
        }

        fn watched_dirs(&self) -> impl Iterator<Item = PathBuf> + '_ {
            self.by_wd.values().cloned()
        }
    }

    fn watch_mask() -> u32 {
        IN_MODIFY
            | IN_ATTRIB
            | IN_CLOSE_WRITE
            | IN_MOVED_FROM
            | IN_MOVED_TO
            | IN_CREATE
            | IN_DELETE
            | IN_DELETE_SELF
            | IN_MOVE_SELF
    }

    let fd = unsafe { inotify_init1(IN_NONBLOCK | IN_CLOEXEC) };
    if fd < 0 {
        let _ = event_tx.send(FileWatchEvent::WatchError {
            path: None,
            message: io::Error::last_os_error().to_string(),
        });
        drain_without_watching(command_rx);
        return;
    }
    let fd = InotifyFd(fd);
    let mut watches = Watches::default();

    loop {
        while let Ok(command) = command_rx.try_recv() {
            match command {
                WatchCommand::SetDirectories(dirs) => watches.rebuild(fd.0, dirs, &event_tx),
                WatchCommand::Stop => return,
            }
        }

        watches.retry_missing(fd.0, &event_tx);
        drain_inotify_events(fd.0, &event_tx, &mut watches);

        match command_rx.recv_timeout(WAIT_TIMEOUT) {
            Ok(WatchCommand::SetDirectories(dirs)) => {
                watches.rebuild(fd.0, dirs, &event_tx);
            }
            Ok(WatchCommand::Stop) | Err(mpsc::RecvTimeoutError::Disconnected) => return,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
    }

    fn drain_without_watching(command_rx: Receiver<WatchCommand>) {
        loop {
            match command_rx.recv() {
                Ok(WatchCommand::SetDirectories(_dirs)) => {}
                Ok(WatchCommand::Stop) | Err(_) => return,
            }
        }
    }

    fn drain_inotify_events(fd: c_int, event_tx: &Sender<FileWatchEvent>, watches: &mut Watches) {
        let mut buffer = vec![0u8; EVENT_BUFFER_SIZE];
        loop {
            let len = unsafe { read(fd, buffer.as_mut_ptr().cast(), buffer.len()) };
            if len < 0 {
                match io::Error::last_os_error().raw_os_error() {
                    Some(EAGAIN) => return,
                    Some(EINTR) => continue,
                    _ => return,
                }
            }
            if len == 0 {
                return;
            }
            dispatch_events(&buffer[..len as usize], event_tx, watches);
        }
    }

    fn dispatch_events(bytes: &[u8], event_tx: &Sender<FileWatchEvent>, watches: &mut Watches) {
        let header_size = size_of::<InotifyEventRaw>();
        let mut offset = 0usize;
        while offset + header_size <= bytes.len() {
            let event = unsafe {
                std::ptr::read_unaligned(bytes.as_ptr().add(offset).cast::<InotifyEventRaw>())
            };
            let next = offset
                .saturating_add(header_size)
                .saturating_add(event.len as usize);
            if next > bytes.len() {
                return;
            }

            if event.mask & IN_Q_OVERFLOW != 0 {
                for dir in watches.watched_dirs() {
                    let _ = event_tx.send(FileWatchEvent::DirectoryChanged(dir));
                }
            } else if is_relevant_event(event.mask)
                && let Some(dir) = watches.by_wd.get(&event.wd).cloned()
            {
                let _ = event_tx.send(FileWatchEvent::DirectoryChanged(dir));
            }

            if event.mask & (IN_IGNORED | IN_DELETE_SELF | IN_MOVE_SELF | IN_UNMOUNT) != 0 {
                watches.by_wd.remove(&event.wd);
            }

            offset = next;
        }
    }
}

fn path_to_c_string(path: &Path) -> Option<CString> {
    CString::new(path.as_os_str().as_bytes()).ok()
}

fn is_relevant_event(mask: u32) -> bool {
    mask & (IN_MODIFY
        | IN_ATTRIB
        | IN_CLOSE_WRITE
        | IN_MOVED_FROM
        | IN_MOVED_TO
        | IN_CREATE
        | IN_DELETE
        | IN_DELETE_SELF
        | IN_MOVE_SELF
        | IN_UNMOUNT)
        != 0
}

#[cfg(test)]
mod tests {
    use super::{FileWatchEvent, FileWatchService};
    use std::collections::BTreeSet;
    use std::fs;
    use std::path::Path;
    use std::thread;
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

    #[test]
    fn watcher_reports_directory_changes() {
        let dir = unique_temp_dir();
        fs::create_dir_all(&dir).expect("create temp watch dir");
        let probe = dir.join("probe.txt");

        let mut service = FileWatchService::new();
        service.set_watched_directories(BTreeSet::from([dir.clone()]));

        let deadline = Instant::now() + Duration::from_secs(2);
        let mut attempt = 0usize;
        loop {
            fs::write(&probe, format!("attempt {attempt}")).expect("write probe file");
            if service.drain_events().into_iter().any(|event| {
                matches!(event, FileWatchEvent::DirectoryChanged(changed) if changed == dir)
            }) {
                fs::remove_dir_all(&dir).ok();
                return;
            }
            assert!(
                Instant::now() < deadline,
                "watcher did not report temp directory change"
            );
            attempt += 1;
            thread::sleep(Duration::from_millis(50));
        }
    }

    #[test]
    fn watcher_retries_a_directory_recreated_at_the_same_path() {
        let dir = unique_temp_dir();
        fs::create_dir_all(&dir).expect("create temp watch dir");
        let mut service = FileWatchService::new();
        service.set_watched_directories(BTreeSet::from([dir.clone()]));

        let initial = dir.join("initial.txt");
        wait_for_change(&mut service, &dir, || {
            fs::write(&initial, "initial").expect("write initial watched file");
        });
        fs::remove_dir_all(&dir).expect("remove watched directory");

        let missing_deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if service.drain_events().into_iter().any(|event| {
                matches!(
                    event,
                    FileWatchEvent::WatchError { path: Some(path), .. } if path == dir
                )
            }) {
                break;
            }
            assert!(
                Instant::now() < missing_deadline,
                "watcher did not notice that its directory watch was removed"
            );
            thread::sleep(Duration::from_millis(50));
        }

        fs::create_dir_all(&dir).expect("recreate watched directory");
        let recreated = dir.join("recreated.txt");
        wait_for_change(&mut service, &dir, || {
            fs::write(&recreated, "recreated").expect("write recreated watched file");
        });
        fs::remove_dir_all(&dir).ok();
    }

    fn wait_for_change(service: &mut FileWatchService, dir: &Path, mut trigger: impl FnMut()) {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            trigger();
            if service.drain_events().into_iter().any(|event| {
                matches!(event, FileWatchEvent::DirectoryChanged(changed) if changed == dir)
            }) {
                return;
            }
            assert!(Instant::now() < deadline, "watcher did not report change");
            thread::sleep(Duration::from_millis(50));
        }
    }

    fn unique_temp_dir() -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "scratchpad-file-watch-test-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time should be after unix epoch")
                .as_nanos()
        ))
    }
}
