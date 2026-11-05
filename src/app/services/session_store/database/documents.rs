//! Documents own metadata and content independently of workspace layouts.
use super::{Database, error};
use crate::app::services::session_store::model::{
    SessionBuffer, SessionPaneNode, SessionTab, SessionView,
};
use std::io;

impl Database {
    /// Save As can replace a closed file. Its former document stays retained,
    /// but must no longer win path lookup or reconcile against the replacement.
    pub(super) async fn release_retained_path(
        &self,
        id: &str,
        key: &str,
    ) -> io::Result<Vec<String>> {
        let document_pieces = self.query_collect("SELECT id FROM documents WHERE path_key=?1 AND id<>?2 AND id NOT IN (SELECT id FROM quarantine) AND id NOT IN (SELECT id FROM document_pieces WHERE part=0)",turso::params![key,id],|r|r.get::<String>(0).map_err(error)).await?;
        self.execute_cached("UPDATE documents SET path=NULL,path_key=NULL,name=name||' (recovered)',search_text=lower(name||' (recovered)'),is_dirty=1,disk_content_digest=NULL,observed_digest=NULL,observed_missing=0,disk_len=NULL,disk_modified_millis=NULL WHERE path_key=?1 AND id<>?2 AND id NOT IN (SELECT id FROM quarantine)",turso::params![key,id]).await?;
        for previous in &document_pieces {
            let snapshot = crate::app::domain::TextDocument::new(String::new()).snapshot();
            self.write_content(
                previous,
                &snapshot,
                &mut std::collections::HashSet::default(),
                &mut super::SessionPersistProfile::default(),
            )
            .await?;
        }
        Ok(document_pieces)
    }
    pub(in crate::app::services::session_store) async fn load_retained_document(
        &mut self,
        id: &str,
    ) -> io::Result<(SessionTab, std::collections::HashMap<String, String>)> {
        self.ensure_healthy()?;
        let buffer = self.read_document(id).await?;
        let mut tab = single_tab(buffer);
        let contents = self.read_tab_contents(std::slice::from_ref(&tab)).await?;
        self.read_histories(std::slice::from_mut(&mut tab)).await?;
        Ok((tab, contents))
    }
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
        if let Some(payload) = self.query_collect("SELECT c.layout_id FROM closed_tabs c JOIN layout_documents r ON c.layout_id=r.layout_id WHERE r.document_id=? ORDER BY c.sequence DESC LIMIT 1", [id.as_str()], |row| super::document_rows::unsigned(row.get(0).map_err(error)?)).await?.pop() {
            return self.read_stored_tab(payload).await.map(Some);
        }
        Ok(Some(single_tab(self.read_document(&id).await?)))
    }

    pub(super) async fn read_document(&self, id: &str) -> io::Result<SessionBuffer> {
        self.read_document_row(id).await?.into_buffer()
    }
}

fn single_tab(buffer: SessionBuffer) -> SessionTab {
    let view = SessionView {
        id: crate::app::domain::view::next_view_id(),
        buffer_id: buffer.id,
        selection: None,
        show_line_numbers: false,
        zoom_factor: 1.0,
    };
    SessionTab {
        layout_id: crate::app::domain::tab::next_layout_id(),
        buffers: vec![buffer],
        active_view_id: view.id,
        root_pane: SessionPaneNode::Leaf { view_id: view.id },
        views: vec![view],
    }
}

impl Database {
    pub(in crate::app::services::session_store) async fn recovery_metadata(
        &mut self,
    ) -> io::Result<(
        super::StoredSession,
        Vec<SessionBuffer>,
        Vec<(String, i64, SessionTab)>,
    )> {
        let stored = self.load(super::ReadScope::All).await?.ok_or_else(|| {
            io::Error::new(io::ErrorKind::NotFound, "no committed session to export")
        })?;
        let live: std::collections::HashSet<_> = stored
            .manifest
            .tabs
            .iter()
            .flat_map(|t| &t.buffers)
            .map(|b| b.temp_id.clone())
            .collect();
        let ids = self
            .query_collect("SELECT id FROM documents ORDER BY rowid", (), |row| {
                row.get::<String>(0).map_err(error)
            })
            .await?;
        let mut retained = Vec::new();
        for id in ids {
            if !live.contains(&id) {
                let (mut tab, _) = self.load_retained_document(&id).await?;
                super::super::validation::validate_tab(&tab)?;
                retained.append(&mut tab.buffers);
            }
        }
        let closed = self
            .closed_rows()
            .await?
            .into_iter()
            .map(|row| (row.token, row.closed_at, row.tab))
            .collect();
        Ok((stored, retained, closed))
    }
}

impl Database {
    pub(in crate::app::services::session_store) async fn hydrate_reset_request(
        &self,
        mut request: crate::app::services::session_store::SessionPersistRequest,
    ) -> io::Result<crate::app::services::session_store::SessionPersistRequest> {
        for tab in request
            .tabs
            .iter_mut()
            .chain(request.closed_tabs.iter_mut().map(|closed| &mut closed.tab))
        {
            if !tab.preserve_existing {
                continue;
            }
            self.read_histories(std::slice::from_mut(&mut tab.session_tab))
                .await?;
            for buffer in &mut tab.session_tab.buffers {
                let document = buffer.history_document.take().ok_or_else(|| {
                    io::Error::other(format!(
                        "Cannot copy unreadable document {} during reset",
                        buffer.temp_id
                    ))
                })?;
                tab.buffer_snapshots
                    .push(super::super::capture::CapturedSessionBuffer {
                        temp_id: buffer.temp_id.clone(),
                        snapshot: document.snapshot(),
                    });
                buffer.history_capture = Some(document.history_snapshot());
            }
            tab.preserve_existing = false;
        }
        Ok(request)
    }
}
