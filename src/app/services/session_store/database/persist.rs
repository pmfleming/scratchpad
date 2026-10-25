use super::{Database, SessionPersistProfile, error, invalid};
use crate::app::services::session_store::capture::{CapturedSessionBuffer, SessionPersistRequest};
use crate::app::services::session_store::{model::SessionManifest, session_manifest};
use std::collections::HashSet;
use std::io;
use std::time::Instant;
use turso::params;

struct Prepared {
    state: Vec<u8>,
    tabs: Vec<Vec<u8>>,
    snapshots: Vec<CapturedSessionBuffer>,
    active: HashSet<String>,
}
impl Prepared {
    fn new(request: SessionPersistRequest) -> io::Result<Self> {
        let SessionPersistRequest {
            active_tab_index,
            active_surface,
            font_size,
            word_wrap,
            tabs,
        } = request;
        let tab_count = tabs.len();
        let mut prepared = Self {
            state: Vec::new(),
            tabs: Vec::with_capacity(tab_count),
            snapshots: Vec::new(),
            active: HashSet::new(),
        };
        for tab in tabs {
            prepared.active.extend(
                tab.session_tab
                    .buffers
                    .iter()
                    .map(|buffer| buffer.temp_id.clone()),
            );
            prepared
                .tabs
                .push(serde_json::to_vec(&tab.session_tab).map_err(invalid)?);
            prepared.snapshots.extend(tab.buffer_snapshots);
        }
        let mut manifest: SessionManifest = session_manifest(
            active_tab_index,
            active_surface,
            font_size,
            word_wrap,
            Vec::new(),
        );
        manifest.active_tab_index = active_tab_index.min(tab_count.saturating_sub(1));
        prepared.state = serde_json::to_vec(&manifest).map_err(invalid)?;
        Ok(prepared)
    }
}

impl Database {
    pub async fn persist(
        &mut self,
        request: SessionPersistRequest,
        generation: u64,
    ) -> io::Result<SessionPersistProfile> {
        if let Some(cause) = &self.blocked {
            return Err(invalid(format!(
                "session is protected after recovery failure: {cause}"
            )));
        }
        if generation < self.latest_request {
            return Ok(SessionPersistProfile::default());
        }
        let start = Instant::now();
        let mut prepared = Prepared::new(request)?;
        let mut profile = SessionPersistProfile {
            manifest_serialize_ns: start.elapsed().as_nanos(),
            manifest_size_bytes: (prepared.state.len()
                + prepared.tabs.iter().map(Vec::len).sum::<usize>())
                as u64,
            tab_count: prepared.tabs.len(),
            buffer_count: prepared.active.len(),
            ..SessionPersistProfile::default()
        };
        prepared.snapshots.retain(|buffer| {
            self.revisions.get(&buffer.temp_id) != Some(&buffer.snapshot.revision())
        });
        let unchanged = prepared.snapshots.is_empty()
            && self.state_cache == prepared.state
            && self.tab_cache == prepared.tabs;
        if !unchanged {
            self.conn
                .execute("BEGIN IMMEDIATE", ())
                .await
                .map_err(error)?;
            let result = self.write_prepared(&prepared, &mut profile).await;
            self.finish_transaction(result).await?;
        }
        // Update caches only after COMMIT. A failed attempt must be fully retryable.
        self.revisions.retain(|id, _| prepared.active.contains(id));
        self.revisions.extend(
            prepared
                .snapshots
                .into_iter()
                .map(|buffer| (buffer.temp_id, buffer.snapshot.revision())),
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
        self.conn
            .execute(
                "DELETE FROM tabs WHERE position>=?",
                [prepared.tabs.len() as i64],
            )
            .await
            .map_err(error)?;
        self.conn.execute("INSERT INTO session_state VALUES(1, ?) ON CONFLICT(id) DO UPDATE SET payload=excluded.payload", [prepared.state.as_slice()]).await.map_err(error)?;
        profile.manifest_write_ns = start.elapsed().as_nanos();
        let start = Instant::now();
        self.delete_stale_content(&prepared.active).await?;
        profile.stale_cleanup_ns = start.elapsed().as_nanos();
        Ok(())
    }

    async fn delete_stale_content(&self, active: &HashSet<String>) -> io::Result<()> {
        let mut rows = self
            .conn
            .query("SELECT id FROM snapshots", ())
            .await
            .map_err(error)?;
        let mut stale = Vec::new();
        while let Some(row) = rows.next().await.map_err(error)? {
            let id: String = row.get(0).map_err(error)?;
            if !active.contains(&id) {
                stale.push(id);
            }
        }
        drop(rows);
        for id in stale {
            self.conn
                .execute("DELETE FROM chunks WHERE id=?", [id.as_str()])
                .await
                .map_err(error)?;
            self.conn
                .execute("DELETE FROM snapshots WHERE id=?", [id.as_str()])
                .await
                .map_err(error)?;
        }
        Ok(())
    }
}
