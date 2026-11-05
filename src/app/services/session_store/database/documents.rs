//! Documents own metadata and content independently of workspace layouts.
use super::{Database, decode, error, invalid};
use crate::app::services::session_store::{
    model::{SessionBuffer, SessionPaneNode, SessionTab, SessionView},
    validation,
};
use serde::{Deserialize, Serialize};
use std::io;

#[derive(Serialize, Deserialize)]
pub(super) struct DocumentRef {
    pub id: u64,
    pub temp_id: String,
}

#[derive(Serialize, Deserialize)]
pub(super) struct StoredTab {
    pub buffers: Vec<DocumentRef>,
    active_view_id: u64,
    views: Vec<SessionView>,
    root_pane: SessionPaneNode,
}

impl StoredTab {
    pub fn capture(tab: &SessionTab) -> Self {
        Self {
            buffers: tab
                .buffers
                .iter()
                .map(|b| DocumentRef {
                    id: b.id,
                    temp_id: b.temp_id.clone(),
                })
                .collect(),
            active_view_id: tab.active_view_id,
            views: tab.views.clone(),
            root_pane: tab.root_pane.clone(),
        }
    }
}

impl Database {
    pub(super) async fn read_document(&self, id: &str) -> io::Result<SessionBuffer> {
        let mut buffer: SessionBuffer = self
            .query_collect("SELECT payload FROM documents WHERE id=?", [id], |row| {
                decode(&row.get::<Vec<u8>>(0).map_err(error)?)
            })
            .await?
            .pop()
            .ok_or_else(|| invalid("workspace references a missing document"))?;
        if buffer.temp_id != id || !buffer.text_history.is_empty() {
            return Err(invalid("invalid document metadata identity"));
        }
        buffer.history_capture = None;
        Ok(buffer)
    }

    pub(super) async fn read_stored_tab(&self, payload: &[u8]) -> io::Result<SessionTab> {
        let stored: StoredTab = decode(payload)?;
        let mut buffers = Vec::with_capacity(stored.buffers.len());
        for reference in stored.buffers {
            let mut buffer = self.read_document(&reference.temp_id).await?;
            buffer.id = reference.id;
            buffers.push(buffer);
        }
        let tab = SessionTab {
            buffers,
            active_view_id: stored.active_view_id,
            views: stored.views,
            root_pane: stored.root_pane,
        };
        validation::validate_tab(&tab)?;
        Ok(tab)
    }
}
