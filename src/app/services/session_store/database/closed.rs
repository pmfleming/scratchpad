//! Closed workspaces are bounded recovery roots, not a second copy of their text.
use super::{Database, decode, error, invalid};
use crate::app::services::session_store::{
    capture::{CapturedClosedTab, CapturedSessionTab, SessionPersistRequest},
    model::SessionTab,
};
use std::{collections::HashSet, io};
use turso::params;

pub(super) const SCHEMA: &str = "CREATE TABLE closed_tabs(sequence INTEGER PRIMARY KEY, token TEXT NOT NULL UNIQUE, closed_at INTEGER NOT NULL, payload BLOB NOT NULL) STRICT;";

pub(in crate::app::services::session_store) struct ClosedRow {
    pub token: String,
    pub closed_at: i64,
    pub payload: Vec<u8>,
    pub tab: SessionTab,
}

impl ClosedRow {
    pub(super) fn new(token: String, closed_at: i64, payload: Vec<u8>) -> io::Result<Self> {
        let tab: SessionTab = decode(&payload)?;
        super::super::validation::validate_tab(&tab)?;
        Ok(Self {
            token,
            closed_at,
            payload,
            tab,
        })
    }
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
        self.query_collect(
            "SELECT c.token, json_extract(b.value,'$.temp_id'), coalesce(json_extract(b.value,'$.path'),json_extract(b.value,'$.name')), coalesce(h.edited_at,0), coalesce(h.edit_seq,0) FROM closed_tabs c, json_each(CAST(c.payload AS TEXT),'$.buffers') b LEFT JOIN document_history h ON h.temp_id=json_extract(b.value,'$.temp_id') ORDER BY h.edited_at DESC,h.edit_seq DESC",
            (), |row| Ok(super::super::retained::RecentHistoryFile {
                token: row.get(0).map_err(error)?,
                temp_id: row.get(1).map_err(error)?,
                label: row.get(2).map_err(error)?,
                edited_at: row.get::<i64>(3).map_err(error)? as u64,
                edit_seq: row.get::<i64>(4).map_err(error)? as u64,
            }),
        ).await
    }
    pub(in crate::app::services::session_store) async fn closed_rows(
        &self,
    ) -> io::Result<Vec<ClosedRow>> {
        self.query_collect(
            "SELECT token, closed_at, payload FROM closed_tabs ORDER BY sequence",
            (),
            |row| {
                ClosedRow::new(
                    row.get(0).map_err(error)?,
                    row.get(1).map_err(error)?,
                    row.get(2).map_err(error)?,
                )
            },
        )
        .await
    }

    pub(super) async fn prepare_closed(
        &mut self,
        request: &mut SessionPersistRequest,
    ) -> io::Result<Vec<ClosedRow>> {
        let old = self.closed_rows().await?;
        let live: HashSet<_> = request
            .tabs
            .iter()
            .flat_map(|tab| &tab.session_tab.buffers)
            .map(|buffer| buffer.temp_id.as_str())
            .collect();
        let pending: HashSet<_> = request
            .closed_tabs
            .iter()
            .map(|tab| tab.token.as_str())
            .collect();
        let mut retained = Vec::new();
        for row in &old {
            let tab = &row.tab;
            if !pending.contains(row.token.as_str())
                && !tab
                    .buffers
                    .iter()
                    .any(|buffer| live.contains(buffer.temp_id.as_str()))
            {
                retained.push(CapturedClosedTab {
                    token: row.token.clone(),
                    closed_at: row.closed_at,
                    tab: CapturedSessionTab {
                        session_tab: tab.clone(),
                        buffer_snapshots: Vec::new(),
                        preserve_existing: true,
                        discard_history: false,
                    },
                });
            }
        }
        retained.append(&mut request.closed_tabs);
        retained.retain(|closed| {
            !closed
                .tab
                .session_tab
                .buffers
                .iter()
                .any(|buffer| live.contains(buffer.temp_id.as_str()))
        });

        request.closed_tabs = retained;
        Ok(old)
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
        let row = self
            .query_collect(
                "SELECT token, closed_at, payload FROM closed_tabs WHERE token=?",
                [token],
                |row| {
                    ClosedRow::new(
                        row.get(0).map_err(error)?,
                        row.get(1).map_err(error)?,
                        row.get(2).map_err(error)?,
                    )
                },
            )
            .await?
            .pop()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "closed tab expired"))?;
        let mut tab = row.tab;
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
