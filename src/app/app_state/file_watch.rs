use super::ScratchpadApp;
use crate::app::domain::BufferId;
use crate::app::services::file_controller::FileController;
use eframe::egui;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const FILE_WATCH_DEBOUNCE: Duration = Duration::from_millis(150);

impl ScratchpadApp {
    pub(crate) fn poll_file_watcher(&mut self, ctx: &egui::Context) {
        self.sync_watched_file_directories();
        self.state
            .file_watch
            .collect_file_watch_events(Instant::now() + FILE_WATCH_DEBOUNCE);
        self.apply_due_file_watch_rescans();

        if self.state.file_watch.has_pending_file_watch_rescans() {
            ctx.request_repaint_after(FILE_WATCH_DEBOUNCE);
        }
    }

    fn sync_watched_file_directories(&mut self) {
        let now = Instant::now();
        let generation = self.tab_manager.path_index_generation;
        if !self
            .state
            .file_watch
            .directories
            .needs_refresh(generation, now)
        {
            return;
        }
        let paths = self
            .tab_manager
            .tabs
            .iter()
            .flat_map(|tab| tab.buffers())
            .filter_map(|buffer| buffer.path.as_deref().map(|path| (buffer.id, path)));
        self.state
            .file_watch
            .directories
            .refresh(generation, now, paths);
        let dirs = self
            .state
            .file_watch
            .directories
            .members
            .keys()
            .cloned()
            .collect();
        self.state.file_watch.set_watched_file_directories(dirs);
    }

    fn apply_due_file_watch_rescans(&mut self) {
        let due_dirs = self
            .state
            .file_watch
            .take_due_file_watch_rescans(Instant::now());
        if due_dirs.is_empty() {
            return;
        }

        let buffer_ids = self.buffer_ids_in_directories(&due_dirs);
        for buffer_id in buffer_ids {
            FileController::refresh_buffer_disk_state_by_id(self, buffer_id);
        }
    }

    fn buffer_ids_in_directories(&self, dirs: &[PathBuf]) -> Vec<BufferId> {
        let mut ids: Vec<_> = dirs
            .iter()
            .filter_map(|dir| self.state.file_watch.directories.members.get(dir))
            .flatten()
            .copied()
            .collect();
        ids.sort_unstable();
        ids.dedup();
        ids
    }
}

// Periodic revalidation handles missing/recreated directories and retargeted
// symlinks without moving filesystem calls back into every logic pass.
const DIRECTORY_REVALIDATE: Duration = Duration::from_secs(5);

#[derive(Default)]
pub(super) struct WatchDirectories {
    generation: Option<u64>,
    refresh_at: Option<Instant>,
    members: HashMap<PathBuf, Vec<BufferId>>,
}

impl WatchDirectories {
    pub(super) fn invalidate(&mut self) {
        self.generation = None;
    }

    fn needs_refresh(&self, generation: u64, now: Instant) -> bool {
        self.generation != Some(generation) || self.refresh_at.is_some_and(|at| now >= at)
    }

    fn refresh<'a>(
        &mut self,
        generation: u64,
        now: Instant,
        paths: impl Iterator<Item = (BufferId, &'a Path)>,
    ) {
        let mut parents: HashMap<&Path, Vec<BufferId>> = HashMap::new();
        for (id, path) in paths {
            if let Some(parent) = path.parent() {
                parents.entry(parent).or_default().push(id);
            }
        }
        self.members.clear();
        for (parent, ids) in parents {
            let normalized = std::fs::canonicalize(parent).unwrap_or_else(|_| parent.to_path_buf());
            self.members.entry(normalized).or_default().extend(ids);
        }
        self.generation = Some(generation);
        self.refresh_at = (!self.members.is_empty()).then_some(now + DIRECTORY_REVALIDATE);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn directory_cache_tracks_membership_and_revalidates_missing_parents() {
        let root = tempfile::tempdir().unwrap();
        let missing = root.path().join("missing");
        let path = missing.join("file");
        let now = Instant::now();
        let mut cache = WatchDirectories::default();
        cache.refresh(
            1,
            now,
            [(1, path.as_path()), (2, path.as_path())].into_iter(),
        );
        assert_eq!(cache.members[&missing], vec![1, 2]);
        assert!(!cache.needs_refresh(1, now));
        assert!(cache.needs_refresh(2, now));
        assert!(cache.needs_refresh(1, now + DIRECTORY_REVALIDATE));
        std::fs::create_dir(&missing).unwrap();
        cache.refresh(2, now, [(2, path.as_path())].into_iter());
        assert_eq!(
            cache.members[&std::fs::canonicalize(&missing).unwrap()],
            vec![2]
        );
        cache.invalidate();
        assert!(cache.needs_refresh(2, now));
        cache.refresh(3, now, std::iter::empty());
        assert!(cache.members.is_empty());
        assert!(!cache.needs_refresh(3, now + DIRECTORY_REVALIDATE));
    }

    #[cfg(unix)]
    #[test]
    fn directory_cache_revalidates_retargeted_symlinks() {
        let root = tempfile::tempdir().unwrap();
        let a = root.path().join("a");
        let b = root.path().join("b");
        let alias = root.path().join("alias");
        std::fs::create_dir(&a).unwrap();
        std::fs::create_dir(&b).unwrap();
        std::os::unix::fs::symlink(&a, &alias).unwrap();
        let path = alias.join("file");
        let mut cache = WatchDirectories::default();
        let now = Instant::now();
        cache.refresh(1, now, [(7, path.as_path())].into_iter());
        assert_eq!(cache.members[&a], vec![7]);
        std::fs::remove_file(&alias).unwrap();
        std::os::unix::fs::symlink(&b, &alias).unwrap();
        cache.refresh(
            1,
            now + DIRECTORY_REVALIDATE,
            [(7, path.as_path())].into_iter(),
        );
        assert_eq!(cache.members[&b], vec![7]);
        assert!(!cache.members.contains_key(&a));
    }
}

#[cfg(test)]
mod performance_probe {
    use crate::{
        ScratchpadApp,
        app::{
            domain::{BufferState, WorkspaceTab},
            services::session_store::SessionStore,
        },
    };
    use eframe::{App, egui};
    use std::{hint::black_box, time::Instant};
    #[test]
    #[ignore = "manual performance measurement"]
    fn many_tab_logic_frames() {
        let root = tempfile::tempdir().unwrap();
        for file_backed in [false, true] {
            let mut app = ScratchpadApp::with_session_store(SessionStore::new(
                root.path().join(if file_backed { "files" } else { "tabs" }),
            ));
            app.set_session_persist_on_drop(false);
            assert!(app.wait_for_background_io_idle_timeout(std::time::Duration::from_secs(10)));
            app.tab_manager.tabs = (0..1_000)
                .map(|i| {
                    WorkspaceTab::new(BufferState::new(
                        format!("{i}.txt"),
                        "hello".into(),
                        file_backed.then(|| root.path().join(format!("{i}.txt"))),
                    ))
                })
                .collect();
            app.tab_manager.active_tab_index = 0;
            app.tab_manager.rebuild_buffer_tab_index();
            let ctx = egui::Context::default();
            let mut frame = eframe::Frame::_new_kittest();
            for _ in 0..3 {
                app.logic(&ctx, &mut frame);
            }
            let mut samples = Vec::new();
            for _ in 0..7 {
                let start = Instant::now();
                for _ in 0..100 {
                    app.logic(black_box(&ctx), &mut frame);
                }
                samples.push(start.elapsed().as_nanos());
            }
            samples.sort_unstable();
            println!(
                "logic_frames files={file_backed} tabs=1000 frames=100 median_ns={} range={}-{}",
                samples[3], samples[0], samples[6]
            );
        }
    }
}
