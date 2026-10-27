use super::{Restorer, SessionStore, WorkspaceTab, model::SessionTab};
use futures_lite::future::block_on;
use std::io;

#[derive(Clone, Debug)]
pub struct ClosedTabInfo {
    pub token: String,
    pub closed_at_unix_seconds: i64,
    pub name: String,
}

impl SessionStore {
    pub fn buffer_versions(&self, temp_id: &str) -> io::Result<Vec<super::BufferVersionInfo>> {
        let id = temp_id.to_owned();
        self.worker()?
            .call(move |db| block_on(db.version_history(&id)))
    }

    pub fn restore_buffer_version(&self, version_id: i64) -> io::Result<String> {
        self.worker()?
            .call(move |db| block_on(db.read_version(version_id)))
    }

    pub fn closed_tabs(&self) -> io::Result<Vec<ClosedTabInfo>> {
        self.worker()?.call(|db| {
            block_on(async {
                db.closed_rows()
                    .await?
                    .into_iter()
                    .rev()
                    .map(|row| {
                        let tab: SessionTab =
                            serde_json::from_slice(&row.payload).map_err(io::Error::other)?;
                        Ok(ClosedTabInfo {
                            token: row.token,
                            closed_at_unix_seconds: row.closed_at,
                            name: tab
                                .buffers
                                .iter()
                                .map(|buffer| buffer.name.as_str())
                                .collect::<Vec<_>>()
                                .join(" + "),
                        })
                    })
                    .collect()
            })
        })
    }

    /// Non-destructive read. The archive is consumed only when a subsequent
    /// successful session save includes its reopened buffers as live tabs.
    pub fn restore_closed_tab(&self, token: &str) -> io::Result<WorkspaceTab> {
        let token = token.to_owned();
        let (tab, contents) = self
            .worker()?
            .call(move |db| block_on(db.read_closed_tab(&token)))?;
        Ok(Restorer::new(contents).restore_tab_with_summary(tab).0)
    }
}
