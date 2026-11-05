//! A capture retains references; only explicit export materializes undo payloads.
#[cfg(test)]
use crate::app::domain::TextDocument;
use crate::app::domain::{PersistedHistoryEntry, buffer::HistorySnapshot};
use std::io;
pub(super) struct HistoryRows(pub HistorySnapshot);
impl HistoryRows {
    pub(super) fn capture(
        capture: Option<HistorySnapshot>,
        exported: Vec<PersistedHistoryEntry>,
    ) -> io::Result<Self> {
        if let Some(capture) = capture {
            return Ok(Self(capture));
        }
        #[cfg(test)]
        {
            Self::from_exported(exported)
        }
        #[cfg(not(test))]
        {
            let _ = exported;
            Err(super::invalid("live capture is missing reference history"))
        }
    }
    #[cfg(test)]
    fn from_exported(rows: Vec<PersistedHistoryEntry>) -> io::Result<Self> {
        super::history::validate(&rows)?;
        let mut document = TextDocument::new(String::new());
        if !document.restore_exported_history(rows) {
            return Err(super::invalid("invalid exported history"));
        }
        Ok(Self(document.history_snapshot()))
    }
    pub(super) fn len(&self) -> usize {
        self.0.len()
    }
    pub(super) fn positions(&self) -> (u64, u64, u64, u64) {
        self.0.positions()
    }
}
