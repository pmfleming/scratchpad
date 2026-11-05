//! Closed workspaces are bounded recovery roots, not a second copy of their text.
use super::{Database, error, invalid};
use crate::app::services::session_store::{capture::SessionPersistRequest, model::SessionTab};
use std::{collections::HashSet, io};
use turso::params;

pub(super) const SCHEMA: &str = "CREATE TABLE closed_tabs(sequence INTEGER PRIMARY KEY, token TEXT NOT NULL UNIQUE, closed_at INTEGER NOT NULL, payload BLOB NOT NULL) STRICT;
CREATE TABLE closed_documents(token TEXT NOT NULL REFERENCES closed_tabs(token) ON DELETE CASCADE, document_id TEXT NOT NULL REFERENCES documents(id), PRIMARY KEY(token,document_id)) STRICT;";

pub(in crate::app::services::session_store) struct ClosedRow {
    pub token: String,
    pub closed_at: i64,
    pub payload: Vec<u8>,
    pub tab: SessionTab,
}

impl PartialEq for ClosedRow {
    fn eq(&self, other: &Self) -> bool {
        self.token == other.token
            && self.closed_at == other.closed_at
            && self.payload == other.payload
    }
}
impl Eq for ClosedRow {}

impl Database {
    pub(in crate::app::services::session_store) async fn recent_history_files(
        &self,
    ) -> io::Result<Vec<super::super::retained::RecentHistoryFile>> {
        self.query_collect("SELECT d.id,coalesce(d.path,d.name),coalesce(h.edited_at,0),coalesce(h.edit_seq,0),EXISTS(SELECT 1 FROM history e WHERE e.temp_id=d.id) FROM documents d LEFT JOIN document_history h ON h.temp_id=d.id ORDER BY h.edited_at DESC,h.edit_seq DESC",(),|row|Ok(super::super::retained::RecentHistoryFile {temp_id:row.get(0).map_err(error)?,label:row.get(1).map_err(error)?,edited_at:row.get::<i64>(2).map_err(error)? as u64,edit_seq:row.get::<i64>(3).map_err(error)? as u64,has_history:row.get::<i64>(4).map_err(error)? !=0})).await
    }

    pub(in crate::app::services::session_store) async fn closed_rows(
        &self,
    ) -> io::Result<Vec<ClosedRow>> {
        let rows = self
            .query_collect(
                "SELECT token, closed_at, payload FROM closed_tabs ORDER BY sequence",
                (),
                |row| {
                    Ok((
                        row.get::<String>(0).map_err(error)?,
                        row.get::<i64>(1).map_err(error)?,
                        row.get::<Vec<u8>>(2).map_err(error)?,
                    ))
                },
            )
            .await?;
        let mut result = Vec::with_capacity(rows.len());
        for (token, closed_at, payload) in rows {
            let tab = self.read_stored_tab(&payload).await?;
            result.push(ClosedRow {
                token,
                closed_at,
                payload,
                tab,
            });
        }
        Ok(result)
    }

    pub(super) async fn prepare_closed(
        &mut self,
        _request: &mut SessionPersistRequest,
    ) -> io::Result<Vec<ClosedRow>> {
        self.closed_rows().await
    }

    pub(super) async fn write_closed(
        &self,
        rows: &[ClosedRow],
        old: &[ClosedRow],
    ) -> io::Result<()> {
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
                    "UPDATE closed_tabs SET closed_at=?, payload=? WHERE token=?",
                    params![row.closed_at, row.payload.as_slice(), row.token.as_str()],
                )
                .await?;
            }
        }
        for row in &rows[retained..] {
            self.execute_cached(
                "INSERT INTO closed_tabs(token, closed_at, payload) VALUES(?, ?, ?)",
                params![row.token.as_str(), row.closed_at, row.payload.as_slice()],
            )
            .await?;
        }
        for row in rows {
            if !old.iter().any(|previous| previous == row) {
                self.execute_cached(
                    "DELETE FROM closed_documents WHERE token=?",
                    [row.token.as_str()],
                )
                .await?;
                for buffer in &row.tab.buffers {
                    self.execute_cached(
                        "INSERT INTO closed_documents VALUES(?,?)",
                        params![row.token.as_str(), buffer.temp_id.as_str()],
                    )
                    .await?;
                }
            }
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
                "SELECT payload FROM closed_tabs WHERE token=?",
                [token],
                |row| row.get::<Vec<u8>>(0).map_err(error),
            )
            .await?
            .pop()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "closed tab expired"))?;
        let mut tab = self.read_stored_tab(&payload).await?;
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
