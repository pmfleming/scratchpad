use super::{Database, SessionPersistProfile, error, invalid};
use crate::app::services::session_store::capture::{CapturedSessionBuffer, SessionPersistRequest};
use crate::app::services::session_store::{model::SessionManifest, session_manifest};
use std::collections::{HashMap, HashSet};
use std::io;
use std::time::Instant;
use turso::params;

struct RecoveryPolicy {
    required: bool,
    retain: bool,
}

struct Prepared {
    state: Vec<u8>,
    tabs: Vec<Vec<u8>>,
    snapshots: Vec<CapturedSessionBuffer>,
    buffers: HashMap<String, RecoveryPolicy>,
}
impl Prepared {
    fn new(request: SessionPersistRequest) -> io::Result<Self> {
        let SessionPersistRequest {
            active_tab_index,
            active_surface,
            font_size,
            word_wrap,
            tabs,
            ..
        } = request;
        let tab_count = tabs.len();
        let mut prepared = Self {
            state: Vec::new(),
            tabs: Vec::with_capacity(tab_count),
            snapshots: Vec::new(),
            buffers: HashMap::new(),
        };
        for tab in tabs {
            super::super::legacy::validate_tab(&tab.session_tab)?;
            for buffer in &tab.session_tab.buffers {
                let required = super::super::legacy::needs_snapshot(buffer);
                let policy = RecoveryPolicy {
                    required,
                    retain: tab.preserve_existing || required || buffer.disk_len.is_none(),
                };
                if prepared
                    .buffers
                    .insert(buffer.temp_id.clone(), policy)
                    .is_some()
                {
                    return Err(invalid("duplicate recovery content identity across tabs"));
                }
            }
            prepared
                .tabs
                .push(serde_json::to_vec(&tab.session_tab).map_err(invalid)?);
            // A supplied snapshot is authoritative: it must survive this commit,
            // even if capture's eligibility rules evolve independently.
            for snapshot in tab.buffer_snapshots {
                prepared
                    .buffers
                    .get_mut(&snapshot.temp_id)
                    .ok_or_else(|| invalid("snapshot has no buffer metadata"))?
                    .retain = true;
                prepared.snapshots.push(snapshot);
            }
        }
        let mut manifest: SessionManifest = session_manifest(
            active_tab_index,
            active_surface,
            font_size,
            word_wrap,
            Vec::new(),
        );
        manifest.active_tab_index = active_tab_index.min(tab_count.saturating_sub(1));
        manifest.tab_count = tab_count;
        prepared.state = serde_json::to_vec(&manifest).map_err(invalid)?;
        Ok(prepared)
    }
    fn retains(&self, id: &str) -> bool {
        self.buffers.get(id).is_some_and(|policy| policy.retain)
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
        let mut profile = SessionPersistProfile {
            manifest_serialize_ns: start.elapsed().as_nanos(),
            manifest_size_bytes: (prepared.state.len()
                + prepared.tabs.iter().map(Vec::len).sum::<usize>())
                as u64,
            tab_count: prepared.tabs.len(),
            buffer_count: prepared.buffers.len(),
            ..SessionPersistProfile::default()
        };
        prepared.snapshots.retain(|buffer| {
            self.revisions.get(&buffer.temp_id) != Some(&buffer.snapshot.revision_key())
        });
        let unchanged = prepared.snapshots.is_empty()
            && self.state_cache == prepared.state
            && self.tab_cache == prepared.tabs
            && self.revisions.keys().all(|id| prepared.retains(id));
        if !unchanged {
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
        // Update caches only after COMMIT. A failed attempt must be fully retryable.
        self.revisions.retain(|id, _| prepared.retains(id));
        self.revisions.extend(
            prepared
                .snapshots
                .into_iter()
                .map(|buffer| (buffer.temp_id, buffer.snapshot.revision_key())),
        );
        self.state_cache = prepared.state;
        self.tab_cache = prepared.tabs;
        self.latest_request = generation;
        profile.total_ns = start.elapsed().as_nanos();
        Ok(profile)
    }

    async fn write_prepared(
        &self,
        prepared: &Prepared,
        profile: &mut SessionPersistProfile,
    ) -> io::Result<()> {
        let start = Instant::now();
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
        let mut rows = self
            .conn
            .query("SELECT id FROM snapshots", ())
            .await
            .map_err(error)?;
        let mut ids = HashSet::new();
        while let Some(row) = rows.next().await.map_err(error)? {
            ids.insert(row.get::<String>(0).map_err(error)?);
        }
        drop(rows);
        if let Some((id, _)) = prepared.buffers.iter().find(|(id, policy)| {
            policy.required && !ids.contains(*id) && !self.quarantined.contains_key(*id)
        }) {
            return Err(super::quarantine::damaged(
                id,
                "refusing to commit missing recovery content",
            ));
        }
        for id in ids
            .iter()
            .filter(|id| !prepared.retains(id) && !self.quarantined.contains_key(*id))
        {
            self.execute_cached("DELETE FROM chunks WHERE id=?", [id.as_str()])
                .await?;
            self.execute_cached("DELETE FROM snapshots WHERE id=?", [id.as_str()])
                .await?;
        }
        Ok(())
    }
}
