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
    pub(in crate::app::services::session_store) async fn load_retained_path(
        &mut self,
        path_key: &str,
    ) -> io::Result<Option<(SessionTab, std::collections::HashMap<String, String>)>> {
        let Some(mut tab) = self.retained_tab_by_path(path_key).await? else {
            return Ok(None);
        };
        let contents = self.read_tab_contents(std::slice::from_ref(&tab)).await?;
        self.read_histories(std::slice::from_mut(&mut tab)).await?;
        Ok(Some((tab, contents)))
    }

    pub(in crate::app::services::session_store) async fn retained_tab_by_path(
        &self,
        path_key: &str,
    ) -> io::Result<Option<SessionTab>> {
        self.ensure_healthy()?;
        let Some(id) = self
            .query_collect(
                "SELECT id FROM documents WHERE path_key=? ORDER BY rowid DESC LIMIT 1",
                [path_key],
                |row| row.get::<String>(0).map_err(error),
            )
            .await?
            .pop()
        else {
            return Ok(None);
        };
        if let Some(payload) = self.query_collect("SELECT c.payload FROM closed_tabs c JOIN closed_documents r ON c.token=r.token WHERE r.document_id=? ORDER BY c.sequence DESC LIMIT 1", [id.as_str()], |row| row.get::<Vec<u8>>(0).map_err(error)).await?.pop() {
            return self.read_stored_tab(&payload).await.map(Some);
        }
        let buffer = self.read_document(&id).await?;
        let view = SessionView {
            id: crate::app::domain::view::next_view_id(),
            buffer_id: buffer.id,
            selection: None,
            show_line_numbers: false,
            zoom_factor: 1.0,
            show_control_chars: false,
        };
        Ok(Some(SessionTab {
            buffers: vec![buffer],
            active_view_id: view.id,
            root_pane: SessionPaneNode::Leaf { view_id: view.id },
            views: vec![view],
        }))
    }

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
