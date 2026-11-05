//! Closed workspaces are bounded recovery roots, not a second copy of their text.
use super::{Database, error, invalid};
use crate::app::services::session_store::{capture::SessionPersistRequest, model::SessionTab};
use std::{collections::HashSet, io};
use turso::params;

pub(super) const SCHEMA: &str = "CREATE TABLE closed_tabs(sequence INTEGER PRIMARY KEY, token TEXT NOT NULL UNIQUE, closed_at INTEGER NOT NULL, layout_id INTEGER NOT NULL UNIQUE REFERENCES layouts(id)) STRICT;";

pub(in crate::app::services::session_store) struct ClosedRow {
    pub token: String,
    pub closed_at: i64,
    pub layout: super::layouts::LayoutRow,
    pub tab: SessionTab,
}

impl PartialEq for ClosedRow {
    fn eq(&self, other: &Self) -> bool {
        self.token == other.token
            && self.closed_at == other.closed_at
            && self.layout == other.layout
    }
}
impl Eq for ClosedRow {}

impl Database {
    pub(in crate::app::services::session_store) async fn recent_history_files(
        &self,
    ) -> io::Result<Vec<super::super::retained::RecentHistoryFile>> {
        self.query_collect("SELECT d.id,coalesce(d.path,d.name),coalesce(h.edited_at,0),coalesce(h.edit_seq,0),EXISTS(SELECT 1 FROM history e WHERE e.temp_id=d.id) FROM documents d LEFT JOIN document_history h ON h.temp_id=d.id ORDER BY h.edit_seq DESC",(),|row|Ok(super::super::retained::RecentHistoryFile {temp_id:row.get(0).map_err(error)?,label:row.get(1).map_err(error)?,edited_at:row.get::<i64>(2).map_err(error)? as u64,edit_seq:row.get::<i64>(3).map_err(error)? as u64,has_history:row.get::<i64>(4).map_err(error)? !=0})).await
    }

    pub(in crate::app::services::session_store) async fn closed_rows(
        &self,
    ) -> io::Result<Vec<ClosedRow>> {
        let rows = self
            .query_collect(
                "SELECT token,closed_at,layout_id FROM closed_tabs ORDER BY sequence",
                (),
                |r| {
                    Ok((
                        r.get::<String>(0).map_err(error)?,
                        r.get::<i64>(1).map_err(error)?,
                        super::document_rows::unsigned(r.get(2).map_err(error)?)?,
                    ))
                },
            )
            .await?;
        self.resolve_closed(rows).await
    }
    async fn resolve_closed(&self, rows: Vec<(String, i64, u64)>) -> io::Result<Vec<ClosedRow>> {
        let ids: Vec<_> = rows.iter().map(|r| r.2).collect();
        let mut tabs = self.read_layouts(&ids).await?;
        rows.into_iter()
            .map(|(token, closed_at, id)| {
                let tab = tabs
                    .remove(&id)
                    .ok_or_else(|| invalid("closed archive has a missing or shared layout"))?;
                Ok(ClosedRow {
                    token,
                    closed_at,
                    layout: super::layouts::LayoutRow::capture(&tab)?,
                    tab,
                })
            })
            .collect()
    }

    pub(super) async fn prepare_closed(
        &mut self,
        request: &mut SessionPersistRequest,
    ) -> io::Result<Vec<ClosedRow>> {
        // Only archives consumed by live/removed documents or explicitly updated
        // closes participate in this diff. Unrelated layouts remain untouched.
        let ids: Vec<_> = request
            .tabs
            .iter()
            .flat_map(|tab| tab.session_tab.buffers.iter().map(|b| b.temp_id.clone()))
            .chain(request.removed_documents.iter().cloned())
            .collect();
        let tokens: Vec<_> = request
            .closed_tabs
            .iter()
            .map(|c| c.token.clone())
            .collect();
        let rows = self.query_collect("SELECT c.token,c.closed_at,c.layout_id FROM closed_tabs c WHERE c.layout_id IN (SELECT layout_id FROM layout_documents WHERE document_id IN (SELECT value FROM json_each(?1))) OR c.token IN (SELECT value FROM json_each(?2)) ORDER BY c.sequence",params![serde_json::to_string(&ids).map_err(invalid)?,serde_json::to_string(&tokens).map_err(invalid)?],|row|Ok((row.get::<String>(0).map_err(error)?,row.get::<i64>(1).map_err(error)?,super::document_rows::unsigned(row.get(2).map_err(error)?)?))).await?;
        self.resolve_closed(rows).await
    }

    pub(super) async fn write_closed(
        &self,
        rows: &[ClosedRow],
        old: &[ClosedRow],
    ) -> io::Result<()> {
        for row in rows {
            self.write_layout(&row.layout).await?;
        }
        let positions: std::collections::HashMap<_, _> = old
            .iter()
            .enumerate()
            .map(|(index, row)| (row.token.as_str(), index))
            .collect();
        // Keep existing sequence numbers for the ordered retained prefix. New
        // or reordered archives append after it; ordinary eviction never rewrites
        // the surviving archives. All deletes precede inserts (token is UNIQUE).
        let mut next = 0;
        let retained = rows
            .iter()
            .take_while(|row| {
                if let Some(&index) = positions.get(row.token.as_str())
                    && index >= next
                {
                    next = index + 1;
                    true
                } else {
                    false
                }
            })
            .count();
        let keep: HashSet<_> = rows[..retained]
            .iter()
            .map(|row| row.token.as_str())
            .collect();
        for row in old {
            if !keep.contains(row.token.as_str()) {
                self.execute_cached(
                    "DELETE FROM closed_tabs WHERE token=?",
                    [row.token.as_str()],
                )
                .await?;
            }
        }
        for row in &rows[..retained] {
            if row != &old[positions[row.token.as_str()]] {
                self.execute_cached(
                    "UPDATE closed_tabs SET closed_at=?, layout_id=? WHERE token=?",
                    params![
                        row.closed_at,
                        super::document_rows::signed(row.layout.id)?,
                        row.token.as_str()
                    ],
                )
                .await?;
            }
        }
        for row in &rows[retained..] {
            self.execute_cached(
                "INSERT INTO closed_tabs(token, closed_at, layout_id) VALUES(?, ?, ?)",
                params![
                    row.token.as_str(),
                    row.closed_at,
                    super::document_rows::signed(row.layout.id)?
                ],
            )
            .await?;
        }
        Ok(())
    }

    pub(in crate::app::services::session_store) async fn read_closed_tab(
        &mut self,
        token: &str,
    ) -> io::Result<(SessionTab, std::collections::HashMap<String, String>)> {
        self.ensure_healthy()?;
        let result = self.read_closed_tab_inner(token).await;
        if let Err(cause) = &result
            && cause.kind() != io::ErrorKind::NotFound
            && super::quarantine::damaged_id(cause).is_none()
        {
            self.blocked = Some(cause.to_string());
        }
        result
    }

    async fn read_closed_tab_inner(
        &mut self,
        token: &str,
    ) -> io::Result<(SessionTab, std::collections::HashMap<String, String>)> {
        let payload = self
            .query_collect(
                "SELECT layout_id FROM closed_tabs WHERE token=?",
                [token],
                |row| super::document_rows::unsigned(row.get(0).map_err(error)?),
            )
            .await?
            .pop()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "closed tab expired"))?;
        let mut tab = self.read_stored_tab(payload).await?;
        let contents = match self.read_tab_contents(std::slice::from_ref(&tab)).await {
            Ok(contents) => contents,
            Err(cause) => {
                self.protect_damaged(&cause, std::slice::from_ref(&tab))
                    .await?;
                return Err(cause);
            }
        };
        if let Err(cause) = self.read_histories(std::slice::from_mut(&mut tab)).await {
            self.protect_damaged(&cause, std::slice::from_ref(&tab))
                .await?;
            return Err(cause);
        }
        for buffer in &tab.buffers {
            if buffer.path.is_none() && !contents.contains_key(&buffer.temp_id) {
                return Err(invalid("closed tab content is missing"));
            }
        }
        Ok((tab, contents))
    }
}
