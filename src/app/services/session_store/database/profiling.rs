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
        Ok(Self { store, tabs, token })
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
