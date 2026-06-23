#[cfg(target_os = "linux")]
pub(crate) use linux_file_watch::{FileWatchEvent, FileWatchService};

#[cfg(windows)]
pub(crate) use windows_file_watch::{FileWatchEvent, FileWatchService};

#[cfg(not(any(windows, target_os = "linux")))]
mod fallback {
    use std::collections::BTreeSet;
    use std::path::PathBuf;

    #[allow(dead_code)]
    #[derive(Clone, Debug)]
    pub(crate) enum FileWatchEvent {
        DirectoryChanged(PathBuf),
    }

    #[derive(Default)]
    pub(crate) struct FileWatchService;

    impl FileWatchService {
        pub(crate) fn new() -> Self {
            Self
        }

        pub(crate) fn set_watched_directories(&mut self, _dirs: BTreeSet<PathBuf>) {}

        pub(crate) fn drain_events(&mut self) -> Vec<FileWatchEvent> {
            Vec::new()
        }
    }
}

#[cfg(not(any(windows, target_os = "linux")))]
pub(crate) use fallback::{FileWatchEvent, FileWatchService};
