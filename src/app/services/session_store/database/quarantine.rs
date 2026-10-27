//! Durable, buffer-scoped protection. Archive metadata before allowing other saves.
use super::{Database, decode, error, invalid};
use crate::app::services::session_store::{
    capture::SessionPersistRequest, model::SessionTab, validation,
};
use std::{collections::HashMap, fmt, io};
use turso::params;

pub(super) const SCHEMA: &str = "CREATE TABLE quarantine(id TEXT PRIMARY KEY, payload BLOB NOT NULL, reason TEXT NOT NULL) STRICT;";

#[derive(Debug)]
struct DamagedBuffer {
    id: String,
    cause: String,
}
impl fmt::Display for DamagedBuffer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "recovery buffer {}: {}", self.id, self.cause)
    }
}
impl std::error::Error for DamagedBuffer {}

pub(super) fn damaged(id: &str, cause: impl fmt::Display) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        DamagedBuffer {
            id: id.into(),
            cause: cause.to_string(),
        },
    )
}
pub(super) fn damaged_id(cause: &io::Error) -> Option<&str> {
    cause
        .get_ref()?
        .downcast_ref::<DamagedBuffer>()
        .map(|damage| damage.id.as_str())
}

impl Database {
    pub(super) async fn load_quarantine(&mut self) -> io::Result<()> {
        let mut rows = self
            .conn
            .query("SELECT id, payload FROM quarantine", ())
            .await
            .map_err(error)?;
        let mut protected = HashMap::new();
        while let Some(row) = rows.next().await.map_err(error)? {
            let id: String = row.get(0).map_err(error)?;
            let tab: SessionTab = decode(&row.get::<Vec<u8>>(1).map_err(error)?)?;
            validation::validate_tab(&tab)?;
            let buffer = tab
                .buffers
                .into_iter()
                .find(|buffer| buffer.temp_id == id)
                .ok_or_else(|| invalid("quarantine archive has no matching buffer"))?;
            protected.insert(id, buffer);
        }
        self.quarantined = protected;
        Ok(())
    }

    pub(super) async fn protect_damaged(
        &mut self,
        cause: &io::Error,
        tabs: &[SessionTab],
    ) -> io::Result<()> {
        let Some(id) = damaged_id(cause) else {
            return Ok(());
        };
        if self.quarantined.contains_key(id) {
            return Ok(());
        }
        let archived;
        let tabs = if tabs
            .iter()
            .any(|tab| tab.buffers.iter().any(|buffer| buffer.temp_id == id))
        {
            tabs
        } else {
            archived = self
                .closed_rows()
                .await?
                .into_iter()
                .map(|row| decode::<SessionTab>(&row.payload))
                .collect::<io::Result<Vec<_>>>()?;
            &archived
        };
        let (tab, buffer) = tabs
            .iter()
            .find_map(|tab| {
                tab.buffers
                    .iter()
                    .find(|buffer| buffer.temp_id == id)
                    .map(|buffer| (tab, buffer))
            })
            .ok_or_else(|| invalid("damaged content has no manifest reference"))?;
        let payload = serde_json::to_vec(tab).map_err(invalid)?;
        if !self.conn.is_autocommit().map_err(error)? {
            return Err(invalid(
                "cannot durably quarantine inside an unfinished transaction",
            ));
        }
        // FULL synchronous autocommit: if archiving fails, the caller blocks the
        // entire store. Existing archive bytes are never updated or deleted.
        self.execute_cached(
            "INSERT INTO quarantine VALUES(?, ?, ?)",
            params![id, payload, cause.to_string()],
        )
        .await?;
        self.quarantined.insert(id.to_owned(), buffer.clone());
        self.revisions.remove(id);
        self.chunk_cache.remove(id);
        // Damage may have removed snapshots/parts outside our normal write path.
        // Rebuild inventory and collect orphaned blobs on the next successful save.
        self.inventory = None;
        Ok(())
    }

    pub(super) async fn protect_failed_write(&mut self, cause: io::Error) -> io::Error {
        // Called only after rollback. Missing pre-existing recovery data can be
        // archived now, allowing the next save to protect healthy edits.
        if self.blocked.is_none()
            && damaged_id(&cause).is_some()
            && let Err(archive_error) = self.protect_current_buffer(&cause).await
        {
            self.blocked = Some(archive_error.to_string());
            return archive_error;
        }
        self.quarantine_error(cause)
    }

    pub(super) fn quarantine_error(&self, cause: io::Error) -> io::Error {
        if self.blocked.is_none()
            && let Some(damage) = cause
                .get_ref()
                .and_then(|source| source.downcast_ref::<DamagedBuffer>())
            && self.quarantined.contains_key(&damage.id)
        {
            return damaged(
                &damage.id,
                format!(
                    "{}; quarantined, healthy buffers can still be saved",
                    damage.cause
                ),
            );
        }
        cause
    }

    pub(super) fn freeze_quarantined(&self, request: &mut SessionPersistRequest) {
        if self.quarantined.is_empty() {
            return;
        }
        for tab in request
            .tabs
            .iter_mut()
            .chain(request.closed_tabs.iter_mut().map(|closed| &mut closed.tab))
        {
            for buffer in &mut tab.session_tab.buffers {
                if let Some(original) = self.quarantined.get(&buffer.temp_id) {
                    let live_id = buffer.id;
                    *buffer = original.clone();
                    // Layout IDs belong to the live tab; the archive stays exact.
                    buffer.id = live_id;
                }
            }
            tab.buffer_snapshots
                .retain(|snapshot| !self.quarantined.contains_key(&snapshot.temp_id));
        }
    }
}
