//! Metadata-only capture plus one-entry-at-a-time payload materialization.
use crate::app::domain::{PersistedHistoryEntry, buffer::HistorySnapshot};
use std::io;

pub(super) enum HistoryRows {
    Document(HistorySnapshot),
    Exported(Vec<PersistedHistoryEntry>),
}
impl HistoryRows {
    pub(super) fn len(&self) -> usize {
        match self {
            Self::Document(doc) => doc.len(),
            Self::Exported(rows) => rows.len(),
        }
    }
    pub(super) fn entry(&self, index: usize) -> io::Result<PersistedHistoryEntry> {
        match self {
            Self::Document(doc) => doc.entry(index),
            Self::Exported(rows) => Ok(rows[index].clone()),
        }
    }
    pub(super) fn positions(&self) -> (u64, u64, u64, u64) {
        match self {
            Self::Document(doc) => doc.positions(),
            Self::Exported(rows) => {
                let now = rows.iter().find(|e| e.current).map_or(0, |e| e.id);
                let tip = rows.iter().find(|e| e.selected_tip).map_or(0, |e| e.id);
                let last = rows
                    .iter()
                    .filter(|e| e.summary != "Initial contents")
                    .max_by_key(|e| e.global_seq);
                (
                    now,
                    tip,
                    last.map_or(0, |e| e.edited_at),
                    last.map_or(0, |e| e.global_seq),
                )
            }
        }
    }
    pub(super) fn validate(&self) -> io::Result<()> {
        match self {
            Self::Exported(rows) => super::history::validate(rows),
            Self::Document(_) => Ok(()),
        }
    }
}
