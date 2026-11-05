use super::{Restorer, SessionStore, WorkspaceTab};
use futures_lite::future::block_on;
use std::io;

#[derive(Clone, Debug)]
pub(crate) struct RecentHistoryFile {
    pub temp_id: String,
    pub label: String,
    pub edited_at: u64,
    pub edit_seq: u64,
    pub has_history: bool,
}

#[derive(Clone, Debug)]
pub struct ClosedTabInfo {
    pub token: String,
    pub closed_at_unix_seconds: i64,
    pub name: String,
    pub paths: Vec<std::path::PathBuf>,
    pub has_unsaved_changes: bool,
}

impl SessionStore {
    pub(crate) fn recent_history_files(&self) -> io::Result<Vec<RecentHistoryFile>> {
        self.worker()?
            .call(|db| block_on(db.recent_history_files()))
    }
    pub fn closed_tabs(&self) -> io::Result<Vec<ClosedTabInfo>> {
        self.worker()?.call(|db| {
            block_on(async {
                db.closed_rows()
                    .await?
                    .into_iter()
                    .rev()
                    .map(|row| {
                        let tab = row.tab;
                        Ok(ClosedTabInfo {
                            token: row.token,
                            closed_at_unix_seconds: row.closed_at,
                            paths: tab
                                .buffers
                                .iter()
                                .filter_map(|buffer| buffer.path.clone())
                                .collect(),
                            has_unsaved_changes: tab
                                .buffers
                                .iter()
                                .any(|buffer| buffer.is_dirty || buffer.path.is_none()),
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
        let tab = self
            .worker()?
            .call(move |db| block_on(db.read_closed_tab(&token)))?;
        let mut tab = Restorer::restore_tab_with_summary(tab)?.0;
        self.bind_tab(&mut tab)?;
        Ok(tab)
    }
}
