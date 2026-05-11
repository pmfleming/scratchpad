use super::ScratchpadApp;
use crate::app::domain::BufferId;
use crate::app::services::file_controller::FileController;
use crate::app::services::file_watch::FileWatchEvent;
use eframe::egui;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const FILE_WATCH_DEBOUNCE: Duration = Duration::from_millis(150);

impl ScratchpadApp {
    pub(crate) fn poll_file_watcher(&mut self, ctx: &egui::Context) {
        self.sync_watched_file_directories();
        self.collect_file_watch_events();
        self.apply_due_file_watch_rescans();

        if !self.io.pending_file_watch_rescans.is_empty() {
            ctx.request_repaint_after(FILE_WATCH_DEBOUNCE);
        }
    }

    fn sync_watched_file_directories(&mut self) {
        let dirs = self
            .open_file_parent_directories()
            .into_iter()
            .collect::<BTreeSet<_>>();
        self.io.file_watch_service.set_watched_directories(dirs);
    }

    fn collect_file_watch_events(&mut self) {
        let due_at = Instant::now() + FILE_WATCH_DEBOUNCE;
        for event in self.io.file_watch_service.drain_events() {
            match event {
                FileWatchEvent::DirectoryChanged(dir) => {
                    self.io.pending_file_watch_rescans.insert(dir, due_at);
                }
            }
        }
    }

    fn apply_due_file_watch_rescans(&mut self) {
        let now = Instant::now();
        let due_dirs = self
            .io
            .pending_file_watch_rescans
            .iter()
            .filter_map(|(dir, due_at)| (*due_at <= now).then_some(dir.clone()))
            .collect::<Vec<_>>();

        for dir in &due_dirs {
            self.io.pending_file_watch_rescans.remove(dir);
        }

        if due_dirs.is_empty() {
            return;
        }

        let buffer_ids = self.buffer_ids_in_directories(&due_dirs);
        for buffer_id in buffer_ids {
            FileController::refresh_buffer_disk_state_by_id(self, buffer_id);
        }
    }

    fn open_file_parent_directories(&self) -> Vec<PathBuf> {
        self.open_file_paths()
            .into_iter()
            .filter_map(|path| watched_parent_dir(&path))
            .collect()
    }

    fn buffer_ids_in_directories(&self, dirs: &[PathBuf]) -> Vec<BufferId> {
        let mut buffer_ids = Vec::new();
        for tab in self.tabs() {
            for buffer in tab.buffers() {
                let Some(path) = &buffer.path else {
                    continue;
                };
                if watched_parent_dir(path).as_ref().is_some_and(|parent| {
                    dirs.iter().any(|dir| crate::app::paths_match(parent, dir))
                }) {
                    buffer_ids.push(buffer.id);
                }
            }
        }
        buffer_ids.sort_unstable();
        buffer_ids.dedup();
        buffer_ids
    }

    fn open_file_paths(&self) -> Vec<PathBuf> {
        self.tabs()
            .iter()
            .flat_map(|tab| tab.buffers())
            .filter_map(|buffer| buffer.path.clone())
            .collect()
    }
}

fn watched_parent_dir(path: &Path) -> Option<PathBuf> {
    let parent = path.parent()?;
    Some(std::fs::canonicalize(parent).unwrap_or_else(|_| parent.to_path_buf()))
}
