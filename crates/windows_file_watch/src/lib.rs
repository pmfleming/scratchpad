use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};

#[derive(Clone, Debug)]
pub enum FileWatchEvent {
    DirectoryChanged(PathBuf),
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

#[cfg(windows)]
fn spawn_watch_thread(command_rx: Receiver<WatchCommand>, event_tx: Sender<FileWatchEvent>) {
    std::thread::Builder::new()
        .name("scratchpad-file-watch".to_owned())
        .spawn(move || windows_watch_loop(command_rx, event_tx))
        .expect("file watch thread should start");
}

#[cfg(not(windows))]
fn spawn_watch_thread(command_rx: Receiver<WatchCommand>, _event_tx: Sender<FileWatchEvent>) {
    std::thread::Builder::new()
        .name("scratchpad-file-watch".to_owned())
        .spawn(move || while !matches!(command_rx.recv(), Ok(WatchCommand::Stop) | Err(_)) {})
        .expect("file watch thread should start");
}

#[cfg(windows)]
mod windows {
    use std::ffi::c_void;

    pub(super) type Bool = i32;
    pub(super) type Dword = u32;
    pub(super) type Handle = *mut c_void;

    pub(super) const FALSE: Bool = 0;
    pub(super) const INVALID_HANDLE_VALUE: Handle = !0isize as Handle;
    pub(super) const WAIT_OBJECT_0: Dword = 0x0000_0000;
    pub(super) const WAIT_TIMEOUT: Dword = 0x0000_0102;
    pub(super) const WAIT_FAILED: Dword = 0xFFFF_FFFF;
    pub(super) const FILE_NOTIFY_CHANGE_FILE_NAME: Dword = 0x0000_0001;
    pub(super) const FILE_NOTIFY_CHANGE_SIZE: Dword = 0x0000_0008;
    pub(super) const FILE_NOTIFY_CHANGE_LAST_WRITE: Dword = 0x0000_0010;
    pub(super) const FILE_NOTIFY_CHANGE_CREATION: Dword = 0x0000_0040;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        pub(super) fn FindFirstChangeNotificationW(
            lpPathName: *const u16,
            bWatchSubtree: Bool,
            dwNotifyFilter: Dword,
        ) -> Handle;
        pub(super) fn FindNextChangeNotification(hChangeHandle: Handle) -> Bool;
        pub(super) fn FindCloseChangeNotification(hChangeHandle: Handle) -> Bool;
        pub(super) fn WaitForMultipleObjects(
            nCount: Dword,
            lpHandles: *const Handle,
            bWaitAll: Bool,
            dwMilliseconds: Dword,
        ) -> Dword;
    }
}

#[cfg(windows)]
fn windows_watch_loop(command_rx: Receiver<WatchCommand>, event_tx: Sender<FileWatchEvent>) {
    use std::os::windows::ffi::OsStrExt;
    use std::time::Duration;
    use windows::{
        FALSE, FILE_NOTIFY_CHANGE_CREATION, FILE_NOTIFY_CHANGE_FILE_NAME,
        FILE_NOTIFY_CHANGE_LAST_WRITE, FILE_NOTIFY_CHANGE_SIZE, FindCloseChangeNotification,
        FindFirstChangeNotificationW, FindNextChangeNotification, Handle, INVALID_HANDLE_VALUE,
        WAIT_FAILED, WAIT_OBJECT_0, WAIT_TIMEOUT, WaitForMultipleObjects,
    };

    struct WatchHandle {
        dir: PathBuf,
        handle: Handle,
    }

    impl Drop for WatchHandle {
        fn drop(&mut self) {
            unsafe {
                FindCloseChangeNotification(self.handle);
            }
        }
    }

    fn open_watch(dir: PathBuf) -> Option<WatchHandle> {
        let mut wide = dir.as_os_str().encode_wide().collect::<Vec<_>>();
        wide.push(0);
        let filter = FILE_NOTIFY_CHANGE_FILE_NAME
            | FILE_NOTIFY_CHANGE_SIZE
            | FILE_NOTIFY_CHANGE_LAST_WRITE
            | FILE_NOTIFY_CHANGE_CREATION;
        let handle = unsafe { FindFirstChangeNotificationW(wide.as_ptr(), FALSE, filter) };
        if handle == INVALID_HANDLE_VALUE {
            return None;
        }
        Some(WatchHandle { dir, handle })
    }

    fn rebuild_watches(dirs: Vec<PathBuf>) -> Vec<WatchHandle> {
        // This local watcher is deliberately dumb and narrow: Scratchpad is a
        // Windows-only app today, so we use Windows directory change handles
        // directly instead of adding a general cross-platform watcher
        // dependency. The handles only tell us "something changed nearby";
        // the editor's existing disk-state checks still decide reload,
        // conflict, and missing-file behavior. If Scratchpad grows beyond
        // Windows, this boundary is the place to consider wiring in `notify`.
        dirs.into_iter().filter_map(open_watch).collect()
    }

    let mut watches = Vec::<WatchHandle>::new();
    let wait_timeout_ms = 250;

    loop {
        while let Ok(command) = command_rx.try_recv() {
            match command {
                WatchCommand::SetDirectories(dirs) => watches = rebuild_watches(dirs),
                WatchCommand::Stop => return,
            }
        }

        if watches.is_empty() {
            match command_rx.recv_timeout(Duration::from_millis(wait_timeout_ms.into())) {
                Ok(WatchCommand::SetDirectories(dirs)) => watches = rebuild_watches(dirs),
                Ok(WatchCommand::Stop) | Err(mpsc::RecvTimeoutError::Disconnected) => return,
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
            continue;
        }

        let Some(index) = wait_for_changed_watch(&watches, wait_timeout_ms) else {
            continue;
        };
        let Some(watch) = watches.get(index) else {
            continue;
        };
        let _ = event_tx.send(FileWatchEvent::DirectoryChanged(watch.dir.clone()));
        unsafe {
            FindNextChangeNotification(watch.handle);
        }
    }

    fn wait_for_changed_watch(watches: &[WatchHandle], timeout_ms: u32) -> Option<usize> {
        const MAXIMUM_WAIT_OBJECTS: usize = 64;
        let chunk_timeout = if watches.len() <= MAXIMUM_WAIT_OBJECTS {
            timeout_ms
        } else {
            50
        };

        for (chunk_index, chunk) in watches.chunks(MAXIMUM_WAIT_OBJECTS).enumerate() {
            let handles = chunk.iter().map(|watch| watch.handle).collect::<Vec<_>>();
            let wait_result = unsafe {
                WaitForMultipleObjects(
                    handles.len() as u32,
                    handles.as_ptr(),
                    FALSE,
                    chunk_timeout,
                )
            };
            if wait_result == WAIT_TIMEOUT || wait_result == WAIT_FAILED {
                continue;
            }
            let local_index = wait_result.saturating_sub(WAIT_OBJECT_0) as usize;
            return Some(chunk_index * MAXIMUM_WAIT_OBJECTS + local_index);
        }

        None
    }
}
