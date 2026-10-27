//! Closed workspaces are bounded recovery roots, not a second copy of their text.
use super::{Database, decode, error, invalid};
use crate::app::services::session_store::{
    capture::{CapturedClosedTab, CapturedSessionTab, SessionPersistRequest},
    model::SessionTab,
};
use std::{collections::HashSet, io};
use turso::params;

pub(crate) const CLOSED_TAB_LIMIT: usize = 32;
pub(super) const SCHEMA: &str = "CREATE TABLE closed_tabs(sequence INTEGER PRIMARY KEY, token TEXT NOT NULL UNIQUE, closed_at INTEGER NOT NULL, payload BLOB NOT NULL) STRICT;";

#[derive(Clone, PartialEq, Eq)]
pub(in crate::app::services::session_store) struct ClosedRow {
    pub token: String,
    pub closed_at: i64,
    pub payload: Vec<u8>,
}

impl Database {
    pub(in crate::app::services::session_store) async fn closed_rows(
        &self,
    ) -> io::Result<Vec<ClosedRow>> {
        let mut rows = self
            .conn
            .query(
                "SELECT token, closed_at, payload FROM closed_tabs ORDER BY sequence",
                (),
            )
            .await
            .map_err(error)?;
        let mut result = Vec::new();
        while let Some(row) = rows.next().await.map_err(error)? {
            let payload: Vec<u8> = row.get(2).map_err(error)?;
            let tab: SessionTab = decode(&payload)?;
            super::super::validation::validate_tab(&tab)?;
            result.push(ClosedRow {
                token: row.get(0).map_err(error)?,
                closed_at: row.get(1).map_err(error)?,
                payload,
            });
        }
        Ok(result)
    }

    pub(super) async fn prepare_closed(
        &self,
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
            let tab: SessionTab = decode(&row.payload)?;
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
                        session_tab: tab,
                        buffer_snapshots: Vec::new(),
                        preserve_existing: true,
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
        if retained.len() > CLOSED_TAB_LIMIT {
            retained.drain(..retained.len() - CLOSED_TAB_LIMIT);
        }
        request.closed_tabs = retained;
        Ok(old)
    }

    pub(super) async fn write_closed(&self, rows: &[ClosedRow]) -> io::Result<()> {
        self.execute_cached("DELETE FROM closed_tabs", ()).await?;
        for row in rows {
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
        let row = self
            .closed_rows()
            .await?
            .into_iter()
            .find(|row| row.token == token)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "closed tab expired"))?;
        let mut tab: SessionTab = decode(&row.payload)?;
        let contents = match self.read_tab_contents(std::slice::from_ref(&tab)).await {
            Ok(contents) => contents,
            Err(cause) => {
                self.protect_damaged(&cause, std::slice::from_ref(&tab))
                    .await?;
                return Err(cause);
            }
        };
        self.read_histories(std::slice::from_mut(&mut tab)).await?;
        // Prefer archived bytes over a now-different file. Recovered file buffers
        // are dirty intentionally; no reopen operation writes original files.
        for buffer in &mut tab.buffers {
            if contents.contains_key(&buffer.temp_id) {
                buffer.is_dirty = true;
            } else if buffer.path.is_none() {
                return Err(invalid("closed tab content is missing"));
            }
        }
        Ok((tab, contents))
    }
}
