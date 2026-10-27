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

#[derive(PartialEq, Eq)]
pub(in crate::app::services::session_store) struct ClosedRow {
    pub token: String,
    pub closed_at: i64,
    pub payload: Vec<u8>,
}

impl Database {
    pub(in crate::app::services::session_store) async fn closed_rows(
        &self,
    ) -> io::Result<Vec<ClosedRow>> {
        self.query_collect(
            "SELECT token, closed_at, payload FROM closed_tabs ORDER BY sequence",
            (),
            |row| {
                let payload: Vec<u8> = row.get(2).map_err(error)?;
                let tab: SessionTab = decode(&payload)?;
                super::super::validation::validate_tab(&tab)?;
                Ok(ClosedRow {
                    token: row.get(0).map_err(error)?,
                    closed_at: row.get(1).map_err(error)?,
                    payload,
                })
            },
        )
        .await
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
                        recovery_points: Vec::new(),
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
