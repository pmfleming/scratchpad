use super::history_rows::HistoryRows;
use super::{
    ContentInventory, Database, SessionPersistProfile, error, invalid, quarantine::damaged,
};
use crate::app::services::session_store::{
    capture::{CapturedSessionBuffer, CapturedSessionTab, SessionPersistRequest},
    model::{SessionManifest, SessionTab},
    validation,
};
use std::collections::{HashMap, HashSet};
use std::io;
use std::time::Instant;
use turso::params;

struct RecoveryBuffer {
    metadata: Vec<u8>,
    name: String,
    path: Option<String>,
    path_key: Option<String>,
    observed_digest: Option<String>,
    observed_missing: bool,
    required: bool,
    // None means cold or quarantined: preserve the committed history.
    history: Option<HistoryRows>,
    history_fingerprints: Option<Vec<super::history::HistoryFingerprint>>,
}

#[derive(Default)]
struct Prepared {
    state: Vec<u8>,
    tabs: Vec<Vec<u8>>,
    snapshots: Vec<CapturedSessionBuffer>,
    buffers: HashMap<String, RecoveryBuffer>,
    removed_documents: Vec<String>,
    full_cleanup: bool,
    closed: Vec<super::closed::ClosedRow>,
    closed_changed: bool,
    old_closed: Vec<super::closed::ClosedRow>,
    chunk_checkpoints: HashMap<String, super::chunking::ChunkCheckpoint>,
}
impl Prepared {
    fn new(
        request: SessionPersistRequest,
        old_closed: Vec<super::closed::ClosedRow>,
    ) -> io::Result<Self> {
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
            removed_documents: request.removed_documents,
            ..Self::default()
        };
        for tab in request.tabs {
            let (payload, _) = prepared.push_tab(tab)?;
            prepared.tabs.push(payload);
        }
        let live: HashSet<_> = prepared.buffers.keys().cloned().collect();
        if live
            .iter()
            .any(|id| prepared.removed_documents.contains(id))
        {
            return Err(invalid("cannot remove a live document"));
        }
        let pending: HashSet<_> = request
            .closed_tabs
            .iter()
            .map(|c| c.token.as_str())
            .collect();
        for old in &old_closed {
            if !pending.contains(old.token.as_str())
                && !old.tab.buffers.iter().any(|b| {
                    live.contains(&b.temp_id) || prepared.removed_documents.contains(&b.temp_id)
                })
            {
                prepared.closed.push(super::closed::ClosedRow {
                    token: old.token.clone(),
                    closed_at: old.closed_at,
                    payload: old.payload.clone(),
                    tab: old.tab.clone(),
                });
            }
        }
        for closed in request.closed_tabs {
            if closed
                .tab
                .session_tab
                .buffers
                .iter()
                .any(|b| live.contains(&b.temp_id))
            {
                continue;
            }
            let (payload, tab) = prepared.push_tab(closed.tab)?;
            prepared.closed.push(super::closed::ClosedRow {
                token: closed.token,
                closed_at: closed.closed_at,
                payload,
                tab,
            });
        }
        prepared.closed_changed = prepared.closed != old_closed;
        prepared.old_closed = old_closed;
        Ok(prepared)
    }
    fn push_tab(&mut self, mut tab: CapturedSessionTab) -> io::Result<(Vec<u8>, SessionTab)> {
        validation::validate_tab(&tab.session_tab)?;
        for buffer in &mut tab.session_tab.buffers {
            let entries = buffer
                .history_capture
                .take()
                .map(HistoryRows::Document)
                .unwrap_or_else(|| HistoryRows::Exported(std::mem::take(&mut buffer.text_history)));
            let history = (!tab.preserve_existing || tab.discard_history).then_some(entries);
            if let Some(entries) = &history {
                entries.validate()?;
            }
            let required = validation::needs_snapshot(buffer);
            let policy = RecoveryBuffer {
                metadata: serde_json::to_vec(buffer).map_err(invalid)?,
                name: buffer.name.clone(),
                path: buffer
                    .path
                    .as_ref()
                    .map(|p| p.to_string_lossy().into_owned()),
                path_key: buffer.path.as_ref().map(|p| {
                    crate::app::CanonicalPathKey::from_path(p)
                        .as_str()
                        .to_owned()
                }),
                observed_digest: buffer.observed_disk_digest.clone(),
                observed_missing: buffer.observed_disk_missing,
                required,
                history_fingerprints: history
                    .as_ref()
                    .map(super::history::fingerprints)
                    .transpose()?,
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
        let payload = serde_json::to_vec(&super::documents::StoredTab::capture(&tab.session_tab))
            .map_err(invalid)?;
        // Supplied snapshots are authoritative, independently of capture policy.
        for snapshot in tab.buffer_snapshots {
            if !self.buffers.contains_key(&snapshot.temp_id) {
                return Err(invalid("snapshot has no buffer metadata"));
            }
            self.snapshots.push(snapshot);
        }
        Ok((payload, tab.session_tab))
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
        let old_closed = self
            .prepare_closed(&mut request)
            .await
            .inspect_err(|cause| {
                // Malformed committed archives must never be replaced by a fresh set.
                self.blocked = Some(cause.to_string());
            })?;
        self.freeze_quarantined(&mut request);
        if request
            .removed_documents
            .iter()
            .any(|id| self.quarantined.contains_key(id))
        {
            return Err(invalid("cannot remove a quarantined document"));
        }
        let mut prepared = Prepared::new(request, old_closed)?;
        self.prepare_changes(&mut prepared).await?;
        let mut profile = prepared.profile();
        profile.manifest_serialize_ns = start.elapsed().as_nanos();
        if self.has_changes(&prepared) {
            // An attempt owns the inventory until COMMIT. Failure drops it, forcing
            // a conservative reload; no clone or post-commit invariant unwrap.
            let inventory = self
                .inventory
                .take()
                .ok_or_else(|| invalid("missing prepared inventory"))?;
            self.conn
                .execute("BEGIN IMMEDIATE", ())
                .await
                .map_err(error)?;
            let result = self
                .write_prepared(&mut prepared, &inventory, &mut profile)
                .await;
            let commit_start = Instant::now();
            if let Err(cause) = self.finish_transaction(result).await {
                return Err(self.protect_failed_write(cause).await);
            }
            profile.manifest_write_ns += commit_start.elapsed().as_nanos();
            let connection_changes = self.integer("SELECT total_changes()").await.ok();
            self.accept_prepared(prepared, inventory, connection_changes);
        }
        self.latest_request = generation;
        profile.total_ns = start.elapsed().as_nanos();
        Ok(profile)
    }

    async fn prepare_changes(&mut self, prepared: &mut Prepared) -> io::Result<()> {
        self.refresh_inventory().await?;
        prepared.full_cleanup = self
            .inventory
            .as_ref()
            .is_some_and(|inventory| inventory.full_cleanup_pending);
        self.prepare_history_changes(prepared).await?;
        prepared.snapshots.retain(|buffer| {
            self.revisions.get(&buffer.temp_id) != Some(&buffer.snapshot.revision_key())
        });
        Ok(())
    }

    async fn refresh_inventory(&mut self) -> io::Result<()> {
        // The worker owns this connection. Detect writes outside the save path
        // (including fault injection) before trusting its committed-ID inventory.
        let connection_changes = self.integer("SELECT total_changes()").await?;
        if self
            .inventory
            .as_ref()
            .is_some_and(|inventory| inventory.connection_changes != Some(connection_changes))
        {
            self.inventory = None;
            self.chunk_cache.clear();
        }
        if self.inventory.is_none() {
            self.inventory = Some(ContentInventory {
                buffers: self.string_set("SELECT id FROM documents").await?,
                snapshots: self.string_set("SELECT id FROM snapshots").await?,
                full_cleanup_pending: true,
                connection_changes: Some(connection_changes),
            });
        }
        Ok(())
    }

    async fn prepare_history_changes(&mut self, prepared: &mut Prepared) -> io::Result<()> {
        for (id, buffer) in &mut prepared.buffers {
            if !self.document_cache.contains_key(id)
                && let Some(payload) = self
                    .query_collect(
                        "SELECT payload FROM documents WHERE id=?",
                        [id.as_str()],
                        |row| row.get::<Vec<u8>>(0).map_err(error),
                    )
                    .await?
                    .pop()
            {
                self.document_cache.insert(id.clone(), payload);
            }
            if self.quarantined.contains_key(id) {
                buffer.history = None;
                buffer.history_fingerprints = None;
            }
            if buffer.history.is_some() && !self.history_cache.contains_key(id) {
                let entries = match self.read_history_fingerprints(id).await {
                    Ok(entries) => entries,
                    Err(cause) => return Err(self.protect_failed_write(cause).await),
                };
                self.history_cache.insert(id.clone(), entries);
            }
        }
        Ok(())
    }

    fn has_changes(&self, prepared: &Prepared) -> bool {
        prepared.full_cleanup
            || !prepared.removed_documents.is_empty()
            || prepared.closed_changed
            || !prepared.snapshots.is_empty()
            || self.state_cache != prepared.state
            || self.tab_cache != prepared.tabs
            || prepared.buffers.iter().any(|(id, buffer)| {
                self.document_cache.get(id) != Some(&buffer.metadata)
                    || buffer
                        .history_fingerprints
                        .as_ref()
                        .is_some_and(|entries| self.history_cache.get(id) != Some(entries))
            })
    }
    fn accept_prepared(
        &mut self,
        prepared: Prepared,
        mut inventory: ContentInventory,
        connection_changes: Option<i64>,
    ) {
        // Only after COMMIT. A failed attempt must be fully retryable.
        inventory.snapshots.extend(
            prepared
                .snapshots
                .iter()
                .map(|buffer| buffer.temp_id.clone()),
        );
        inventory.buffers.extend(prepared.buffers.keys().cloned());
        for id in &prepared.removed_documents {
            inventory.buffers.remove(id);
            inventory.snapshots.remove(id);
            self.revisions.remove(id);
            self.chunk_cache.remove(id);
            self.history_cache.remove(id);
            self.document_cache.remove(id);
        }
        inventory.full_cleanup_pending = false;
        inventory.connection_changes = connection_changes;
        self.inventory = Some(inventory);
        // Retention is durable; comparison caches only need the current capture.
        self.chunk_cache
            .retain(|id, _| prepared.buffers.contains_key(id));
        self.revisions
            .retain(|id, _| prepared.buffers.contains_key(id));
        self.history_cache
            .retain(|id, _| prepared.buffers.contains_key(id));
        self.document_cache
            .retain(|id, _| prepared.buffers.contains_key(id));
        self.chunk_cache.extend(prepared.chunk_checkpoints);
        self.revisions.extend(
            prepared
                .snapshots
                .into_iter()
                .map(|b| (b.temp_id, b.snapshot.revision_key())),
        );
        self.document_cache.extend(
            prepared
                .buffers
                .iter()
                .map(|(id, b)| (id.clone(), b.metadata.clone())),
        );
        self.history_cache.extend(
            prepared
                .buffers
                .into_iter()
                .filter_map(|(id, b)| b.history_fingerprints.map(|h| (id, h))),
        );
        self.state_cache = prepared.state;
        self.tab_cache = prepared.tabs;
    }

    async fn write_prepared(
        &self,
        prepared: &mut Prepared,
        inventory: &ContentInventory,
        profile: &mut SessionPersistProfile,
    ) -> io::Result<()> {
        let mut removed_digests = HashSet::new();
        let start = Instant::now();
        self.write_buffer_history(prepared, inventory).await?;
        self.write_prepared_snapshots(prepared, &mut removed_digests, profile)
            .await?;
        profile.snapshot_write_ns = start.elapsed().as_nanos();
        let start = Instant::now();
        self.write_prepared_metadata(prepared).await?;
        profile.manifest_write_ns = start.elapsed().as_nanos();
        let start = Instant::now();
        self.delete_stale_content(prepared, inventory, removed_digests)
            .await?;
        profile.stale_cleanup_ns = start.elapsed().as_nanos();
        Ok(())
    }

    async fn write_buffer_history(
        &self,
        prepared: &Prepared,
        inventory: &ContentInventory,
    ) -> io::Result<()> {
        for (id, buffer) in &prepared.buffers {
            if self.document_cache.get(id) != Some(&buffer.metadata) {
                let sql = if inventory.buffers.contains(id) {
                    "UPDATE documents SET name=?2,path=?3,path_key=?4,payload=?5,observed_digest=?6,observed_missing=?7 WHERE id=?1"
                } else {
                    "INSERT INTO documents(id,name,path,path_key,payload,observed_digest,observed_missing) VALUES(?1,?2,?3,?4,?5,?6,?7)"
                };
                self.execute_cached(
                    sql,
                    params![
                        id.as_str(),
                        buffer.name.as_str(),
                        buffer.path.clone(),
                        buffer.path_key.clone(),
                        buffer.metadata.as_slice(),
                        buffer.observed_digest.clone(),
                        i64::from(buffer.observed_missing)
                    ],
                )
                .await?;
            }
            if let (Some(entries), Some(fingerprints)) =
                (&buffer.history, &buffer.history_fingerprints)
            {
                self.write_history(id, entries, fingerprints).await?;
            } else if !inventory.buffers.contains(id) {
                self.execute_cached(
                    "INSERT INTO document_history VALUES(?,0,0,0,0)",
                    [id.as_str()],
                )
                .await?;
            }
        }
        Ok(())
    }

    async fn write_prepared_snapshots(
        &self,
        prepared: &mut Prepared,
        removed_digests: &mut HashSet<Vec<u8>>,
        profile: &mut SessionPersistProfile,
    ) -> io::Result<()> {
        for buffer in &prepared.snapshots {
            let checkpoint = self
                .write_content(&buffer.temp_id, &buffer.snapshot, removed_digests, profile)
                .await?;
            prepared
                .chunk_checkpoints
                .insert(buffer.temp_id.clone(), checkpoint);
        }
        Ok(())
    }

    async fn write_prepared_metadata(&self, prepared: &Prepared) -> io::Result<()> {
        let mut upsert = self.conn.prepare_cached("INSERT INTO tabs VALUES(?, ?) ON CONFLICT(position) DO UPDATE SET payload=excluded.payload").await.map_err(error)?;
        for (position, bytes) in prepared.tabs.iter().enumerate() {
            if self.tab_cache.get(position) != Some(bytes) {
                upsert
                    .execute(params![position as i64, bytes.as_slice()])
                    .await
                    .map_err(error)?;
                self.execute_cached(
                    "DELETE FROM tab_documents WHERE position=?",
                    [position as i64],
                )
                .await?;
                let stored: super::documents::StoredTab = super::decode(bytes)?;
                for doc in stored.buffers {
                    self.execute_cached(
                        "INSERT INTO tab_documents VALUES(?,?)",
                        params![position as i64, doc.temp_id],
                    )
                    .await?;
                }
            }
        }
        self.execute_cached(
            "DELETE FROM tabs WHERE position>=?",
            [prepared.tabs.len() as i64],
        )
        .await?;
        self.execute_cached("INSERT INTO session_state VALUES(1, ?) ON CONFLICT(id) DO UPDATE SET payload=excluded.payload", [prepared.state.as_slice()]).await?;
        if prepared.closed_changed {
            self.write_closed(&prepared.closed, &prepared.old_closed)
                .await?;
        }
        Ok(())
    }

    async fn delete_stale_content(
        &self,
        prepared: &Prepared,
        inventory: &ContentInventory,
        mut removed_digests: HashSet<Vec<u8>>,
    ) -> io::Result<()> {
        let written: HashSet<_> = prepared
            .snapshots
            .iter()
            .map(|buffer| &buffer.temp_id)
            .collect();
        if let Some((id, _)) = prepared.buffers.iter().find(|(id, buffer)| {
            buffer.required
                && !inventory.snapshots.contains(*id)
                && !written.contains(id)
                && !self.quarantined.contains_key(*id)
        }) {
            return Err(damaged(id, "refusing to commit missing recovery content"));
        }
        for id in &prepared.removed_documents {
            self.collect_digests(
                "SELECT digest FROM parts WHERE temp_id=?",
                [id.as_str()],
                &mut removed_digests,
            )
            .await?;
            self.execute_cached("DELETE FROM documents WHERE id=?", [id.as_str()])
                .await?;
        }
        // Quarantined and shared parts remain GC roots even after live tabs close.
        if prepared.full_cleanup {
            self.execute_cached(
                "DELETE FROM blobs WHERE digest NOT IN (SELECT digest FROM parts)",
                (),
            )
            .await?;
        } else {
            for digest in removed_digests {
                self.execute_cached(
                    "DELETE FROM blobs WHERE digest=? AND NOT EXISTS(SELECT 1 FROM parts WHERE digest=?)",
                    params![digest.as_slice(), digest.as_slice()],
                ).await?;
            }
        }
        Ok(())
    }
}
