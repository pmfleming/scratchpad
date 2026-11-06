//! Production-path fixture for Scratchpad Performance Lens; no timing budgets here.
use super::super::{CapturedClosedTab, SessionPersistRequest, SessionStore};
use crate::app::domain::{BufferState, WorkspaceTab};
use crate::app::ui::editor_content::native_editor::{
    CharCursor, CursorRange, EditOperation, OperationRecord,
};
use std::{io, path::PathBuf, time::Instant};

pub struct SessionMaintenanceFixture {
    store: SessionStore,
    tabs: Vec<WorkspaceTab>,
    token: String,
    aged_chunks: Vec<std::sync::Arc<crate::app::services::chunk_store::Chunk>>,
}

impl SessionMaintenanceFixture {
    pub fn new(root: PathBuf, text: String, archives: usize) -> io::Result<Self> {
        // Never let a profiling fixture replace an existing user session.
        std::fs::create_dir(&root)?;
        let store = SessionStore::new(root);
        let tabs = vec![WorkspaceTab::new(BufferState::new(
            "live".into(),
            text,
            None,
        ))];
        let mut request = SessionPersistRequest::capture(&tabs, 0, 16.0, false);
        let mut token = String::new();
        for index in 0..archives {
            let tab = WorkspaceTab::new(BufferState::new(
                format!("archive-{index}"),
                format!("{index}: λ\n").repeat(4096),
                None,
            ));
            let archive = CapturedClosedTab::capture(&tab, None);
            token.clone_from(&archive.token);
            request.closed_tabs.push(archive);
        }
        store.persist_request(request)?;
        Ok(Self {
            store,
            tabs,
            token,
            aged_chunks: Vec::new(),
        })
    }

    /// Build a populated store with retained chunk bodies, real edit history and archives.
    pub fn seed_aged(&mut self, bytes: usize, revisions: usize) -> io::Result<()> {
        self.aged_chunks = self.store.profile_seed_text(bytes, 0x8719_aab2)?;
        for index in 0..revisions {
            let buffer = self.tabs[0].active_buffer_mut();
            let at = buffer.document().piece_tree().len_chars();
            let text = format!("revision-{index}\n");
            let document = buffer.document_mut();
            document.insert_direct(at, &text);
            document.push_edit_operation_with_source(
                OperationRecord {
                    previous_cursor: CursorRange::one(CharCursor::new(at)),
                    next_cursor: CursorRange::one(CharCursor::new(at + text.len())),
                    edits: vec![EditOperation {
                        start_char: at,
                        deleted_text: String::new(),
                        inserted_text: text,
                        deleted_spans: Vec::new(),
                    }],
                },
                crate::app::domain::PieceSource::Edit,
            );
            self.store.persist(&self.tabs, 0, 16.0, false)?;
        }
        self.store.flush_text_chunks()
    }

    #[must_use]
    pub fn database_bytes(&self) -> u64 {
        ["session.db", "session.db-wal"]
            .iter()
            .filter_map(|name| {
                std::fs::metadata(self.store.root().join(name))
                    .ok()
                    .map(|m| m.len())
            })
            .sum()
    }

    /// Repeated, independently evicted reads while production saves run.
    /// A read without observed physical I/O is reported as unverified, never cold.
    pub fn cold_reads_during_saves(&mut self) -> io::Result<serde_json::Value> {
        use crate::profile::process_metrics::{ProcessMeasurement, process_snapshot};
        use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
        let done = AtomicBool::new(false);
        let saves = AtomicU64::new(0);
        let store = self.store.clone();
        let tabs = self.tabs.clone();
        let mut timings = Vec::new();
        let mut raw = Vec::new();
        // Flush dirty pages only in this disposable fixture, before timed reads.
        for name in ["session.db", "session.db-wal"] {
            if let Ok(file) = std::fs::File::open(self.store.root().join(name)) {
                file.sync_all()?;
            }
        }
        std::thread::scope(|scope| -> io::Result<()> {
            let writer = scope.spawn(|| -> io::Result<()> {
                while !done.load(Ordering::Acquire) {
                    store.persist(&tabs, 0, 16.0, false)?;
                    saves.fetch_add(1, Ordering::Release);
                }
                Ok(())
            });
            let reading = (|| -> io::Result<()> {
                for index in 0..128 {
                    self.store.chunk_store_handle()?.set_limits(0, 0);
                    let mut evicted = cfg!(target_os = "linux");
                    for name in ["session.db", "session.db-wal"] {
                        let path = self.store.root().join(name);
                        if path.exists() {
                            evicted &= std::process::Command::new("fadvise")
                                .args(["--advice", "dontneed"])
                                .arg(&path)
                                .output()
                                .is_ok_and(|o| o.status.success());
                        }
                    }
                    let chunk = &self.aged_chunks[(index * 97) % self.aged_chunks.len()];
                    let before = process_snapshot();
                    let started = Instant::now();
                    let text = chunk.load(false, false)?;
                    std::hint::black_box(text.len());
                    let elapsed = started.elapsed().as_nanos();
                    let io = ProcessMeasurement::between(before, process_snapshot());
                    let counters = serde_json::to_value(io).map_err(io::Error::other)?;
                    let verified =
                        evicted && counters["read_bytes"].as_u64().is_some_and(|b| b > 0);
                    if verified {
                        timings.push(elapsed);
                    }
                    raw.push(
                        serde_json::json!({"elapsed_ns":elapsed,"eviction_succeeded":evicted,
                        "physical_read_verified":verified,"read_bytes":counters["read_bytes"]}),
                    );
                }
                Ok(())
            })();
            done.store(true, Ordering::Release);
            let writing = writer
                .join()
                .map_err(|_| io::Error::other("save thread panicked"))?;
            reading?;
            writing?;
            Ok(())
        })?;
        timings.sort_unstable();
        let p99 = (!timings.is_empty()).then(|| timings[(timings.len() * 99).div_ceil(100) - 1]);
        Ok(
            serde_json::json!({"cold_read_p99_ns":p99,"physical_read_verified_count":timings.len(),
            "read_sample_count":raw.len(),"concurrent_saves":saves.load(Ordering::Acquire),
            "physical_io_scope":"process read_bytes during each read; includes concurrent saves",
            "cold_read_samples":raw}),
        )
    }

    pub fn aged_operation(&mut self, operation: &str) -> io::Result<()> {
        match operation {
            "aged_store_save" => {
                self.store.persist(&self.tabs, 0, 16.0, false)?;
            }
            "aged_store_restore" => {
                std::hint::black_box(self.store.load_startup_visible()?);
            }
            "aged_store_close" => {
                let closed = WorkspaceTab::new(BufferState::new(
                    "closed-aged".into(),
                    "archive".into(),
                    None,
                ));
                let mut request = SessionPersistRequest::capture(&self.tabs, 0, 16.0, false);
                request
                    .closed_tabs
                    .push(CapturedClosedTab::capture(&closed, None));
                self.store.persist_request(request)?;
            }
            "aged_store_forget" => {
                let buffer = self.tabs[0].active_buffer_mut();
                buffer.document_mut().clear_operation_history();
                let document = buffer.temp_id.clone();
                let epoch = self.store.history_epoch() + 1;
                self.store.invalidate_history();
                let mut request = SessionPersistRequest::capture(&self.tabs, 0, 16.0, false);
                request.forgotten.push((document, epoch));
                self.store.persist_request(request)?;
            }
            _ => return Err(io::Error::other("unknown aged operation")),
        }
        Ok(())
    }

    /// Prepare a recorded edit; fixture setup is excluded from measured samples.
    pub fn prepare(&mut self, scenario: &str) -> io::Result<()> {
        if scenario.starts_with("save_after_") || scenario == "incremental_save" {
            let buffer = self.tabs[0].active_buffer_mut();
            if scenario == "save_after_branch" {
                buffer
                    .navigate_history(1)
                    .map_err(|error| io::Error::other(format!("{error:?}")))?;
            }
            let text = format!("x{}", buffer.text());
            buffer.document_mut().replace_exact_with_history(&text);
            buffer.is_dirty = true;
        }
        Ok(())
    }

    /// Returns action time, subsequent save time, and incremental bytes hashed.
    pub fn sample(&mut self, scenario: &str) -> io::Result<(u128, u128, u64)> {
        let start = Instant::now();
        match scenario {
            "stored_usage" => {
                let usage = self.store.storage_usage()?;
                if let Some(error) = usage.stored_error {
                    return Err(io::Error::other(error));
                }
                std::hint::black_box(usage);
            }
            "closed_token_lookup" => {
                std::hint::black_box(self.store.restore_closed_tab(&self.token)?);
            }
            "closed_archive_append" => {
                let archive = WorkspaceTab::new(BufferState::new(
                    "appended".into(),
                    "small archive".into(),
                    None,
                ));
                let mut request = SessionPersistRequest::capture(&self.tabs, 0, 16.0, false);
                request
                    .closed_tabs
                    .push(CapturedClosedTab::capture(&archive, None));
                self.store.persist_request(request)?;
            }
            "save_after_navigation" => {
                self.tabs[0]
                    .active_buffer_mut()
                    .navigate_history(1)
                    .map_err(|error| io::Error::other(format!("{error:?}")))?;
            }
            "save_after_branch" | "incremental_save" => {}
            _ => return Err(io::Error::other("unknown session profile scenario")),
        }
        let action = start.elapsed().as_nanos();
        if scenario.starts_with("save_after_") || scenario == "incremental_save" {
            let start = Instant::now();
            let profile = self.store.persist_profiled(&self.tabs, 0, 16.0, false)?;
            Ok((
                action,
                start.elapsed().as_nanos(),
                profile.snapshot_bytes_hashed,
            ))
        } else {
            Ok((action, 0, 0))
        }
    }

    pub fn verify(&self) -> io::Result<()> {
        let restored = self
            .store
            .load()?
            .ok_or_else(|| io::Error::other("missing profile session"))?;
        if restored.tabs[0].active_buffer().text() != self.tabs[0].active_buffer().text() {
            return Err(io::Error::other("profile session text mismatch"));
        }
        Ok(())
    }

    /// Capture before the worker starts, then time mutation and the frame's
    /// storage-health query while that exact captured revision is saved.
    pub fn typing_sample(&mut self, saving: bool) -> io::Result<(Vec<u128>, u128)> {
        let request = SessionPersistRequest::capture(&self.tabs, 0, 16.0, false);
        let store = &self.store;
        let buffer = self.tabs[0].active_buffer_mut();
        let barrier = std::sync::Barrier::new(2);
        std::thread::scope(|scope| {
            let job = saving.then(|| {
                scope.spawn(|| {
                    barrier.wait();
                    let timer = Instant::now();
                    store.persist_request(request)?;
                    Ok::<_, io::Error>(timer.elapsed().as_nanos())
                })
            });
            if saving {
                barrier.wait();
            }
            let mut times = Vec::new();
            for _ in 0..128 {
                let timer = Instant::now();
                std::hint::black_box(store.writes_blocked());
                let at = buffer.document().piece_tree().len_chars();
                let document = buffer.document_mut();
                document.insert_direct(at, "x");
                document.push_edit_operation_with_source(
                    OperationRecord {
                        previous_cursor: CursorRange::one(CharCursor::new(at)),
                        next_cursor: CursorRange::one(CharCursor::new(at + 1)),
                        edits: vec![EditOperation {
                            start_char: at,
                            deleted_text: String::new(),
                            inserted_text: "x".into(),
                            deleted_spans: Vec::new(),
                        }],
                    },
                    crate::app::domain::PieceSource::Edit,
                );
                buffer.is_dirty = true;
                times.push(timer.elapsed().as_nanos());
            }
            let save_ns = match job {
                Some(job) => job
                    .join()
                    .map_err(|_| io::Error::other("profile save panicked"))??,
                None => 0,
            };
            Ok((times, save_ns))
        })
    }
}

impl SessionStore {
    /// Production probe boundary: hold one writer job while measuring UI frames.
    /// This injects a scheduling stall, not a claim about physical device latency.
    pub fn profile_pause_writer(
        &self,
        duration: std::time::Duration,
    ) -> io::Result<std::thread::JoinHandle<io::Result<()>>> {
        let worker = self.worker()?.clone();
        let (started, ready) = std::sync::mpsc::sync_channel(1);
        let task = std::thread::Builder::new()
            .name("profile-writer-pause".into())
            .spawn(move || {
                worker.call(move |_| {
                    started.send(()).map_err(io::Error::other)?;
                    std::thread::sleep(duration);
                    Ok(())
                })
            })?;
        ready
            .recv_timeout(std::time::Duration::from_secs(5))
            .map_err(io::Error::other)?;
        Ok(task)
    }
}

impl super::super::SessionStore {
    /// Production writer stress fixture. Retained handles protect live-only rows
    /// without a giant RAM text or an unrelated manifest in the measured editor.
    pub fn profile_seed_text(
        &self,
        bytes: usize,
        seed: u64,
    ) -> io::Result<Vec<std::sync::Arc<crate::app::services::chunk_store::Chunk>>> {
        let store = self.chunk_store_handle()?;
        let mut chunks = Vec::new();
        let mut state = seed.max(1);
        let mut remaining = bytes;
        while remaining > 0 {
            let take = remaining.min(256 * 1024);
            let mut text = String::with_capacity(take);
            for i in 0..take {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                text.push(if i % 80 == 79 {
                    '\n'
                } else {
                    (b' ' + (state % 95) as u8) as char
                });
            }
            chunks.push(store.seal_bulk(std::sync::Arc::new(text), u64::MAX)?);
            remaining -= take;
        }
        self.flush_text_chunks()?;
        Ok(chunks)
    }
}

#[cfg(test)]
mod aged_profile_tests {
    #[test]
    fn aged_operations_preserve_text_and_run_against_persisted_history() {
        let root = tempfile::tempdir().unwrap();
        let mut fixture = super::SessionMaintenanceFixture::new(
            root.path().join("aged"),
            crate::profile::fixtures::text(1024 * 1024, 77),
            2,
        )
        .unwrap();
        fixture.seed_aged(4 * 1024 * 1024, 3).unwrap();
        assert!(fixture.database_bytes() >= 4 * 1024 * 1024);
        for operation in [
            "aged_store_save",
            "aged_store_restore",
            "aged_store_close",
            "aged_store_forget",
        ] {
            fixture.aged_operation(operation).unwrap();
            fixture.verify().unwrap();
        }
    }
}
