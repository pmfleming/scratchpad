use super::{Database, SessionPersistProfile, error, invalid, quarantine::damaged};
use crate::app::domain::PersistedHistoryEntry;
use crate::app::services::session_store::{
    capture::{CapturedSessionBuffer, CapturedSessionTab, SessionPersistRequest},
    model::SessionManifest,
    validation,
};
use std::collections::HashMap;
use std::io;
use std::time::Instant;
use turso::params;

struct RecoveryBuffer {
    required: bool,
    retain: bool,
    // None means cold or quarantined: preserve the committed history.
    history: Option<Vec<PersistedHistoryEntry>>,
}

#[derive(Default)]
struct Prepared {
    state: Vec<u8>,
    tabs: Vec<Vec<u8>>,
    snapshots: Vec<CapturedSessionBuffer>,
    buffers: HashMap<String, RecoveryBuffer>,
}
impl Prepared {
    fn new(request: SessionPersistRequest) -> io::Result<Self> {
        let tab_count = request.tabs.len();
        let manifest = SessionManifest {
            version: super::super::SESSION_VERSION,
            tab_count,
            active_tab_index: request.active_tab_index.min(tab_count.saturating_sub(1)),
            active_surface: request.active_surface,
            font_size: request.font_size,
            word_wrap: request.word_wrap,
            tabs: Vec::new(),
        };
        let mut prepared = Self {
            state: serde_json::to_vec(&manifest).map_err(invalid)?,
            ..Self::default()
        };
        for tab in request.tabs {
            prepared.push_tab(tab)?;
        }
        Ok(prepared)
    }
    fn push_tab(&mut self, mut tab: CapturedSessionTab) -> io::Result<()> {
        validation::validate_tab(&tab.session_tab)?;
        for buffer in &mut tab.session_tab.buffers {
            let entries = std::mem::take(&mut buffer.text_history);
            let history = (!tab.preserve_existing).then_some(entries);
            if let Some(entries) = &history {
                super::history::validate(entries)?;
            }
            let required = validation::needs_snapshot(buffer);
            let policy = RecoveryBuffer {
                required,
                retain: tab.preserve_existing || required || buffer.disk_len.is_none(),
                history,
            };
            if self
                .buffers
                .insert(buffer.temp_id.clone(), policy)
                .is_some()
            {
                return Err(invalid("duplicate recovery content identity across tabs"));
            }
        }
        self.tabs
            .push(serde_json::to_vec(&tab.session_tab).map_err(invalid)?);
        // Supplied snapshots are authoritative, independently of capture policy.
        for snapshot in tab.buffer_snapshots {
            self.buffers
                .get_mut(&snapshot.temp_id)
                .ok_or_else(|| invalid("snapshot has no buffer metadata"))?
                .retain = true;
            self.snapshots.push(snapshot);
        }
        Ok(())
    }
    fn retains(&self, id: &str) -> bool {
        self.buffers.get(id).is_some_and(|buffer| buffer.retain)
    }
    fn profile(&self) -> SessionPersistProfile {
        SessionPersistProfile {
            manifest_size_bytes: (self.state.len() + self.tabs.iter().map(Vec::len).sum::<usize>())
                as u64,
            tab_count: self.tabs.len(),
            buffer_count: self.buffers.len(),
            ..SessionPersistProfile::default()
        }
    }
}

impl Database {
    pub async fn persist(
        &mut self,
        mut request: SessionPersistRequest,
        generation: u64,
    ) -> io::Result<SessionPersistProfile> {
        self.ensure_healthy()?;
        if !self.verified {
            self.load(super::ReadScope::Shells).await?;
        }
        if generation < self.latest_request {
            return Ok(SessionPersistProfile::default());
        }
        let start = Instant::now();
        self.freeze_quarantined(&mut request);
        let mut prepared = Prepared::new(request)?;
        self.prepare_changes(&mut prepared).await?;
        let mut profile = prepared.profile();
        profile.manifest_serialize_ns = start.elapsed().as_nanos();
        if self.has_changes(&prepared) {
            self.conn
                .execute("BEGIN IMMEDIATE", ())
                .await
                .map_err(error)?;
            let result = self.write_prepared(&prepared, &mut profile).await;
            let commit_start = Instant::now();
            if let Err(cause) = self.finish_transaction(result).await {
                return Err(self.protect_failed_write(cause).await);
            }
            profile.manifest_write_ns += commit_start.elapsed().as_nanos();
        }
        self.accept_prepared(prepared);
        self.latest_request = generation;
        profile.total_ns = start.elapsed().as_nanos();
        Ok(profile)
    }

    async fn prepare_changes(&mut self, prepared: &mut Prepared) -> io::Result<()> {
        for (id, buffer) in &mut prepared.buffers {
            if self.quarantined.contains_key(id) {
                buffer.history = None;
            }
            if buffer.history.is_some() && !self.history_cache.contains_key(id) {
                let entries = match self.read_history(id).await {
                    Ok(entries) => entries,
                    Err(cause) => return Err(self.protect_failed_write(cause).await),
                };
                self.history_cache.insert(id.clone(), entries);
            }
        }
        prepared.snapshots.retain(|buffer| {
            self.revisions.get(&buffer.temp_id) != Some(&buffer.snapshot.revision_key())
        });
        Ok(())
    }
    fn has_changes(&self, prepared: &Prepared) -> bool {
        !prepared.snapshots.is_empty()
            || self.state_cache != prepared.state
            || self.tab_cache != prepared.tabs
            || self.revisions.keys().any(|id| !prepared.retains(id))
            || prepared.buffers.iter().any(|(id, buffer)| {
                buffer
                    .history
                    .as_ref()
                    .is_some_and(|entries| self.history_cache.get(id) != Some(entries))
            })
    }
    fn accept_prepared(&mut self, prepared: Prepared) {
        // Only after COMMIT. A failed attempt must be fully retryable.
        self.revisions.retain(|id, _| prepared.retains(id));
        self.revisions.extend(
            prepared
                .snapshots
                .into_iter()
                .map(|buffer| (buffer.temp_id, buffer.snapshot.revision_key())),
        );
        self.history_cache
            .retain(|id, _| prepared.buffers.contains_key(id));
        self.history_cache.extend(
            prepared
                .buffers
                .into_iter()
                .filter_map(|(id, buffer)| buffer.history.map(|entries| (id, entries))),
        );
        self.state_cache = prepared.state;
        self.tab_cache = prepared.tabs;
    }

    async fn write_prepared(
        &self,
        prepared: &Prepared,
        profile: &mut SessionPersistProfile,
    ) -> io::Result<()> {
        let start = Instant::now();
        for (id, buffer) in &prepared.buffers {
            self.execute_cached(
                "INSERT INTO buffer_ids VALUES(?) ON CONFLICT(id) DO NOTHING",
                [id.as_str()],
            )
            .await?;
            if let Some(entries) = &buffer.history {
                self.write_history(id, entries).await?;
            }
        }
        for buffer in &prepared.snapshots {
            self.write_content(&buffer.temp_id, &buffer.snapshot)
                .await?;
        }
        profile.snapshot_write_ns = start.elapsed().as_nanos();
        let start = Instant::now();
        let mut upsert = self.conn.prepare_cached("INSERT INTO tabs VALUES(?, ?) ON CONFLICT(position) DO UPDATE SET payload=excluded.payload").await.map_err(error)?;
        for (position, bytes) in prepared.tabs.iter().enumerate() {
            if self.tab_cache.get(position) != Some(bytes) {
                upsert
                    .execute(params![position as i64, bytes.as_slice()])
                    .await
                    .map_err(error)?;
            }
        }
        self.execute_cached(
            "DELETE FROM tabs WHERE position>=?",
            [prepared.tabs.len() as i64],
        )
        .await?;
        self.execute_cached("INSERT INTO session_state VALUES(1, ?) ON CONFLICT(id) DO UPDATE SET payload=excluded.payload", [prepared.state.as_slice()]).await?;
        profile.manifest_write_ns = start.elapsed().as_nanos();
        let start = Instant::now();
        self.delete_stale_content(prepared).await?;
        profile.stale_cleanup_ns = start.elapsed().as_nanos();
        Ok(())
    }

    async fn delete_stale_content(&self, prepared: &Prepared) -> io::Result<()> {
        let snapshots = self.string_set("SELECT id FROM snapshots").await?;
        if let Some((id, _)) = prepared.buffers.iter().find(|(id, buffer)| {
            buffer.required && !snapshots.contains(*id) && !self.quarantined.contains_key(*id)
        }) {
            return Err(damaged(id, "refusing to commit missing recovery content"));
        }
        for id in snapshots
            .iter()
            .filter(|id| !prepared.retains(id) && !self.quarantined.contains_key(*id))
        {
            self.execute_cached("DELETE FROM snapshots WHERE id=?", [id.as_str()])
                .await?;
        }
        for id in self
            .string_set("SELECT id FROM buffer_ids")
            .await?
            .iter()
            .filter(|id| !prepared.buffers.contains_key(*id) && !self.quarantined.contains_key(*id))
        {
            self.execute_cached("DELETE FROM buffer_ids WHERE id=?", [id.as_str()])
                .await?;
        }
        // Quarantined parts remain GC roots even after their live tabs close.
        self.execute_cached(
            "DELETE FROM blobs WHERE digest NOT IN (SELECT digest FROM parts)",
            (),
        )
        .await
    }
}
