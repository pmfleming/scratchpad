//! Metadata-only timeline reads. Edit identity remains stable when coalescing moves it.
use super::SessionStore;
use crate::app::domain::buffer::PieceSource;
use futures_lite::future::block_on;
use std::io;

pub(crate) const TIMELINE_PAGE_SIZE: usize = 100;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct TimelineCursor {
    pub sequence: u64,
    pub document: String,
    pub node: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum TimelineId {
    Edit { document: String, node: u64 },
    Activity(u64),
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum TimelineKind {
    Edit(PieceSource),
    Activity(super::activity::ActivityKind),
}

#[derive(Clone, Debug)]
pub(crate) struct TimelineRow {
    pub cursor: TimelineCursor,
    pub timestamp: u64,
    pub name: String,
    pub path: Option<String>,
    pub summary: String,
    pub kind: TimelineKind,
}

impl TimelineRow {
    pub fn id(&self) -> TimelineId {
        match self.kind {
            TimelineKind::Edit(_) => TimelineId::Edit {
                document: self.cursor.document.clone(),
                node: self.cursor.node,
            },
            TimelineKind::Activity(_) => TimelineId::Activity(self.cursor.sequence),
        }
    }
    pub fn node(&self) -> Option<u64> {
        matches!(self.kind, TimelineKind::Edit(_)).then_some(self.cursor.node)
    }
    pub fn label(&self) -> &str {
        self.path.as_deref().unwrap_or(&self.name)
    }
}

impl super::activity::ActivityEvent {
    pub fn timeline_row(&self) -> TimelineRow {
        TimelineRow {
            cursor: TimelineCursor {
                sequence: self.sequence,
                document: self.document.clone(),
                node: 0,
            },
            timestamp: self.timestamp,
            name: self.name.clone(),
            path: self.path.clone(),
            summary: self.kind.label().into(),
            kind: TimelineKind::Activity(self.kind),
        }
    }
}

pub(crate) struct TimelinePage {
    pub rows: Vec<TimelineRow>,
    pub older: Option<TimelineCursor>,
}

impl SessionStore {
    pub(crate) fn timeline_page(
        &self,
        before: Option<TimelineCursor>,
        loaded: Vec<String>,
        search: String,
    ) -> io::Result<TimelinePage> {
        self.worker()?
            .call(move |db| block_on(db.timeline_page(before, loaded, search)))
    }
}
