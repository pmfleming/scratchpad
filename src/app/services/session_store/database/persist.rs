use super::history_rows::HistoryRows;
use super::{
    ContentInventory, Database, SessionPersistProfile, error, invalid, quarantine::damaged,
};
use crate::app::services::session_store::{
    capture::{CapturedSessionBuffer, CapturedSessionTab, SessionPersistRequest},
    model::SessionTab,
    validation,
};
use std::collections::{HashMap, HashSet};
use std::io;
use std::time::Instant;
use turso::params;

struct RecoveryBuffer {
    metadata: super::document_rows::DocumentRow,
    required: bool,
    // None means cold or quarantined: preserve the committed history.
    history: Option<HistoryRows>,
    history_fingerprints: Option<Vec<super::history::HistoryFingerprint>>,
    changed_history: Vec<super::history::PreparedHistoryEntry>,
}

#[derive(Default)]
struct Prepared {
    state: super::layouts::StateRow,
    tabs: Vec<super::layouts::LayoutRow>,
    snapshots: Vec<CapturedSessionBuffer>,
    buffers: HashMap<String, RecoveryBuffer>,
    removed_documents: Vec<String>,
    retained_snapshots: Vec<String>,
    activity: Vec<super::super::activity::ActivityEvent>,
    forgotten: Vec<String>,
    full_cleanup: bool,
    closed: Vec<super::closed::ClosedRow>,
    closed_changed: bool,
    old_closed: Vec<super::closed::ClosedRow>,
    contents: HashMap<String, super::content::PreparedContent>,
}
impl Prepared {
    fn new(
        mut request: SessionPersistRequest,
        old_closed: Vec<super::closed::ClosedRow>,
    ) -> io::Result<Self> {
        // Loading previews have no durable identity or content. Keep their
        // positions in captures for close operations, then omit them here.
        let active = request.active_tab_index;
        let mut position = 0;
        let mut removed_before_active = 0;
        request.tabs.retain(|tab| {
            let keep = !tab.loading_preview_only;
            if !keep && position < active {
                removed_before_active += 1;
            }
            position += 1;
            keep
        });
        request.active_tab_index = active.saturating_sub(removed_before_active);
        let tab_count = request.tabs.len();
        let state = super::layouts::StateRow {
            tab_count,
            active_tab_index: request.active_tab_index.min(tab_count.saturating_sub(1)),
            active_surface: super::document_rows::enum_name(request.active_surface)?,
        };
        let mut prepared = Self {
            state,
            removed_documents: request.removed_documents,
            activity: request.activity,
            forgotten: request.forgotten.into_iter().map(|(id, _)| id).collect(),
            ..Self::default()
        };
        prepared
            .activity
            .retain(|event| !prepared.removed_documents.contains(&event.document));
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
                    layout: old.layout.clone(),
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
            let (layout, tab) = prepared.push_tab(closed.tab)?;
            prepared.closed.push(super::closed::ClosedRow {
                token: closed.token,
                closed_at: closed.closed_at,
                layout,
                tab,
            });
        }
        prepared.closed_changed = prepared.closed != old_closed;
        prepared.old_closed = old_closed;
        Ok(prepared)
    }
    fn push_tab(
        &mut self,
        mut tab: CapturedSessionTab,
    ) -> io::Result<(super::layouts::LayoutRow, SessionTab)> {
        validation::validate_tab(&tab.session_tab)?;
        for buffer in &mut tab.session_tab.buffers {
            let history = if tab.preserve_existing {
                buffer.history_capture = None;
                buffer.text_history.clear();
                None
            } else {
                Some(HistoryRows::capture(
                    buffer.history_capture.take(),
                    std::mem::take(&mut buffer.text_history),
                )?)
            };
            let required = validation::needs_snapshot(buffer);
            let policy = RecoveryBuffer {
                metadata: super::document_rows::DocumentRow::capture(buffer)?,
                required,
                changed_history: Vec::new(),
                history_fingerprints: history.as_ref().map(super::history::fingerprints),
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
        let payload = super::layouts::LayoutRow::capture(&tab.session_tab)?;
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
            manifest_size_bytes: (24
                + self
                    .tabs
                    .iter()
                    .map(super::layouts::LayoutRow::estimated_bytes)
                    .sum::<usize>()) as u64,
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
        let hashed_before = self.chunk_store()?.hashed_bytes();
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
        profile.snapshot_bytes_hashed = self
            .chunk_store()?
            .hashed_bytes()
            .saturating_sub(hashed_before);
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
        let snapshots: HashMap<_, _> = prepared
            .snapshots
            .iter()
            .map(|s| (s.temp_id.as_str(), s))
            .collect();
        for (id, buffer) in &prepared.buffers {
            if let Some(history) = &buffer.history {
                let changed = self.history_cache.get(id) != buffer.history_fingerprints.as_ref();
                let text_changed = snapshots.contains_key(id.as_str());
                if !changed
                    && history.len() == 0
                    && self
                        .document_cache
                        .get(id)
                        .is_some_and(|row| row.text_is_empty)
                    && self
                        .inventory
                        .as_ref()
                        .is_some_and(|i| i.snapshots.contains(id))
                    && snapshots
                        .get(id.as_str())
                        .is_none_or(|s| s.snapshot.len_bytes() == 0)
                {
                    // Empty text with no undo references has no backing bytes to
                    // compare. Preserve its existing manifest after reopening.
                    continue;
                }
                if changed
                    || text_changed
                    || !self
                        .inventory
                        .as_ref()
                        .is_some_and(|i| i.snapshots.contains(id))
                {
                    let tree = if history.len() == 0 {
                        snapshots
                            .get(id.as_str())
                            .map_or_else(|| history.0.piece_tree(), |s| s.snapshot.piece_tree())
                    } else {
                        history.0.piece_tree()
                    };
                    let content = self.prepare_content(id, tree).await?;
                    prepared.contents.insert(id.clone(), content);
                }
            }
        }
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
            self.tail_cache.clear();
            self.history_cache.clear();
            self.history_positions.clear();
        }
        if self.inventory.is_none() {
            self.inventory = Some(ContentInventory {
                buffers: self.string_set("SELECT id FROM documents").await?,
                snapshots: self
                    .string_set("SELECT id FROM document_pieces WHERE part=0")
                    .await?,
                full_cleanup_pending: true,
                connection_changes: Some(connection_changes),
            });
        }
        Ok(())
    }

    async fn prepare_history_changes(&mut self, prepared: &mut Prepared) -> io::Result<()> {
        let ids: Vec<_> = prepared
            .buffers
            .keys()
            .filter(|id| !self.document_cache.contains_key(*id))
            .cloned()
            .collect();
        let ids = serde_json::to_string(&ids).map_err(invalid)?;
        for row in self
            .query_collect(
                &format!(
                    "SELECT {} FROM documents WHERE id IN (SELECT value FROM json_each(?))",
                    super::document_rows::COLUMNS
                ),
                [ids.as_str()],
                super::document_rows::DocumentRow::read,
            )
            .await?
        {
            self.document_cache.insert(row.id.clone(), row);
        }
        let layout_ids: Vec<_> = prepared
            .tabs
            .iter()
            .chain(prepared.closed.iter().map(|c| &c.layout))
            .map(|l| l.id)
            .filter(|id| !self.layout_cache.contains_key(id))
            .collect();
        for (id, tab) in self.read_layouts(&layout_ids).await? {
            self.layout_cache
                .insert(id, super::layouts::LayoutRow::capture(&tab)?);
        }
        let history_ids = prepared
            .buffers
            .iter()
            .filter(|(id, b)| {
                b.history.is_some()
                    && !self.quarantined.contains_key(*id)
                    && !self.history_cache.contains_key(*id)
            })
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>();
        if let Err(cause) = self.prime_history_caches(&history_ids).await {
            return Err(self.protect_failed_write(cause).await);
        }
        for (id, buffer) in &mut prepared.buffers {
            if self.quarantined.contains_key(id) {
                buffer.history = None;
                buffer.history_fingerprints = None;
            }
        }
        for (id, buffer) in &mut prepared.buffers {
            if let (Some(history), Some(keys)) = (&buffer.history, &buffer.history_fingerprints) {
                if !self.history_positions.contains_key(id) {
                    self.history_positions
                        .insert(id.clone(), self.read_history_positions(id).await?);
                }
                buffer.changed_history = self.prepare_history(id, history, keys)?;
            }
        }
        Ok(())
    }

    fn has_changes(&self, prepared: &Prepared) -> bool {
        prepared.full_cleanup
            || !prepared.activity.is_empty()
            || !prepared.forgotten.is_empty()
            || !prepared.removed_documents.is_empty()
            || prepared.closed_changed
            || !prepared.snapshots.is_empty()
            || self.state_cache.as_ref() != Some(&prepared.state)
            || self.tab_cache != prepared.tabs.iter().map(|l| l.id).collect::<Vec<_>>()
            || prepared
                .tabs
                .iter()
                .any(|l| self.layout_cache.get(&l.id) != Some(l))
            || prepared.buffers.iter().any(|(id, buffer)| {
                self.document_cache.get(id) != Some(&buffer.metadata)
                    || buffer
                        .history
                        .as_ref()
                        .is_some_and(|h| self.history_positions.get(id) != Some(&h.positions()))
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
        inventory
            .snapshots
            .extend(prepared.contents.keys().cloned());
        inventory.buffers.extend(prepared.buffers.keys().cloned());
        inventory.snapshots.extend(prepared.retained_snapshots);
        for id in &prepared.removed_documents {
            inventory.buffers.remove(id);
            inventory.snapshots.remove(id);
            self.revisions.remove(id);
            self.tail_cache.remove(id);
            self.history_cache.remove(id);
            self.history_positions.remove(id);
            self.document_cache.remove(id);
        }
        inventory.full_cleanup_pending = false;
        inventory.connection_changes = connection_changes;
        self.inventory = Some(inventory);
        // Retention is durable; comparison caches only need the current capture.
        self.tail_cache
            .retain(|id, _| prepared.buffers.contains_key(id));
        self.revisions
            .retain(|id, _| prepared.buffers.contains_key(id));
        self.history_cache
            .retain(|id, _| prepared.buffers.contains_key(id));
        self.document_cache
            .retain(|id, _| prepared.buffers.contains_key(id));
        for (id, content) in prepared.contents {
            if let Some(checkpoint) = content.checkpoint {
                self.tail_cache.insert(id, checkpoint);
            } else {
                self.tail_cache.remove(&id);
            }
        }
        for (id, buffer) in &prepared.buffers {
            if let Some(history) = &buffer.history {
                self.history_positions
                    .insert(id.clone(), history.positions());
            }
        }
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
        self.state_cache = Some(prepared.state);
        self.tab_cache = prepared.tabs.iter().map(|l| l.id).collect();
        let retained: HashSet<_> = prepared
            .tabs
            .iter()
            .map(|l| l.id)
            .chain(prepared.closed.iter().map(|c| c.layout.id))
            .collect();
        self.layout_cache.retain(|id, _| retained.contains(id));
        self.layout_cache.extend(
            prepared
                .tabs
                .into_iter()
                .chain(prepared.closed.into_iter().map(|c| c.layout))
                .map(|l| (l.id, l)),
        );
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
        self.write_activity(&prepared.activity, &prepared.forgotten)
            .await?;
        profile.stale_cleanup_ns = start.elapsed().as_nanos();
        Ok(())
    }

    async fn write_buffer_history(
        &self,
        prepared: &mut Prepared,
        inventory: &ContentInventory,
    ) -> io::Result<()> {
        for (id, buffer) in &prepared.buffers {
            if self.document_cache.get(id) != Some(&buffer.metadata) {
                if let Some(key) = &buffer.metadata.path_key {
                    prepared
                        .retained_snapshots
                        .extend(self.release_retained_path(id, key).await?);
                }
                self.write_document_row(&buffer.metadata, inventory.buffers.contains(id))
                    .await?;
            }
            if let (Some(entries), Some(fingerprints)) =
                (&buffer.history, &buffer.history_fingerprints)
            {
                self.write_history(id, entries, fingerprints, &buffer.changed_history)
                    .await?;
            } else if !inventory.buffers.contains(id) {
                self.execute_cached("INSERT INTO history_nodes VALUES(?,0)", [id.as_str()])
                    .await?;
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
        let _ = removed_digests;
        for (id, content) in &prepared.contents {
            self.write_prepared_content(id, content).await?;
        }
        profile.snapshot_bytes_read = 0;
        Ok(())
    }

    async fn write_prepared_metadata(&self, prepared: &Prepared) -> io::Result<()> {
        for layout in &prepared.tabs {
            self.write_layout(layout).await?;
        }
        let ids: Vec<_> = prepared.tabs.iter().map(|l| l.id).collect();
        if self.tab_cache != ids {
            // Delete positions before inserting: layout_id has a UNIQUE constraint.
            self.execute_cached("DELETE FROM tabs", ()).await?;
            for (position, id) in ids.iter().enumerate() {
                self.execute_cached(
                    "INSERT INTO tabs VALUES(?,?)",
                    params![
                        i64::try_from(position).map_err(invalid)?,
                        super::document_rows::signed(*id)?
                    ],
                )
                .await?;
            }
        }
        if self.state_cache.as_ref() != Some(&prepared.state) {
            self.execute_cached("INSERT INTO session_state VALUES(1,?,?,?) ON CONFLICT(id) DO UPDATE SET tab_count=excluded.tab_count,active_tab_index=excluded.active_tab_index,active_surface=excluded.active_surface", params![i64::try_from(prepared.state.tab_count).map_err(invalid)?,i64::try_from(prepared.state.active_tab_index).map_err(invalid)?,prepared.state.active_surface.as_str()]).await?;
        }
        if prepared.closed_changed {
            self.write_closed(&prepared.closed, &prepared.old_closed)
                .await?;
        }
        self.execute_cached("DELETE FROM layouts WHERE id NOT IN (SELECT layout_id FROM tabs UNION ALL SELECT layout_id FROM closed_tabs)",()).await?;
        Ok(())
    }

    async fn delete_stale_content(
        &self,
        prepared: &Prepared,
        inventory: &ContentInventory,
        mut removed_digests: HashSet<Vec<u8>>,
    ) -> io::Result<()> {
        let written: HashSet<_> = prepared.contents.keys().collect();
        if let Some((id, _)) = prepared.buffers.iter().find(|(id, buffer)| {
            buffer.required
                && !inventory.snapshots.contains(*id)
                && !written.contains(id)
                && !self.quarantined.contains_key(*id)
        }) {
            return Err(damaged(id, "refusing to commit missing recovery content"));
        }
        let _ = &mut removed_digests;
        for id in &prepared.removed_documents {
            self.execute_cached("DELETE FROM documents WHERE id=?", [id.as_str()])
                .await?;
        }
        Ok(())
    }
}
