//! Rows for the Status panel: newest first, filtered, with repeats folded together.
use crate::app::app_state::{StatusDomain, StatusFilter, StatusMessage, StatusSeverity};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct StatusRow {
    /// The newest message this row stands for.
    pub(super) id: u64,
    pub(super) severity: StatusSeverity,
    pub(super) domain: StatusDomain,
    pub(super) text: String,
    pub(super) detail: Option<String>,
    pub(super) time: u64,
    /// How many identical messages in a row this stands for.
    pub(super) repeats: usize,
}

impl StatusRow {
    pub(super) fn from_status(status: &StatusMessage) -> Self {
        Self {
            id: status.id,
            severity: status.severity,
            domain: status.domain,
            text: status.text.clone(),
            detail: status.detail.clone(),
            time: status.time,
            repeats: 1,
        }
    }

    fn repeats_message(&self, status: &StatusMessage) -> bool {
        self.severity == status.severity
            && self.domain == status.domain
            && self.text == status.text
            && self.detail == status.detail
    }

    /// The message and its detail, as copied to the clipboard.
    pub(super) fn copy_text(&self) -> String {
        match &self.detail {
            Some(detail) => format!("{}\n{detail}", self.text),
            None => self.text.clone(),
        }
    }

    /// "Error · Session", plus how often it repeated.
    pub(super) fn subtitle(&self) -> String {
        let base = format!(
            "{} · {}",
            severity_name(self.severity),
            domain_label(self.domain)
        );
        if self.repeats > 1 {
            format!("{base} · {} times", self.repeats)
        } else {
            base
        }
    }
}

/// Newest first; consecutive identical messages become one row.
pub(super) fn rows<'a>(
    messages: impl DoubleEndedIterator<Item = &'a StatusMessage>,
    filter: StatusFilter,
) -> Vec<StatusRow> {
    let mut rows: Vec<StatusRow> = Vec::new();
    for status in messages.rev().filter(|s| filter.matches(s.severity)) {
        match rows.last_mut() {
            Some(row) if row.repeats_message(status) => row.repeats += 1,
            _ => rows.push(StatusRow::from_status(status)),
        }
    }
    rows
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Counts {
    pub(super) errors: usize,
    pub(super) warnings: usize,
    pub(super) info: usize,
}

impl Counts {
    pub(super) fn of<'a>(messages: impl Iterator<Item = &'a StatusMessage>) -> Self {
        let mut counts = Self::default();
        for status in messages {
            match status.severity {
                StatusSeverity::Error => counts.errors += 1,
                StatusSeverity::Warning => counts.warnings += 1,
                StatusSeverity::Info => counts.info += 1,
            }
        }
        counts
    }

    pub(super) fn get(self, filter: StatusFilter) -> usize {
        match filter {
            StatusFilter::All => self.errors + self.warnings + self.info,
            StatusFilter::Errors => self.errors,
            StatusFilter::Warnings => self.warnings,
            StatusFilter::Info => self.info,
        }
    }
}

pub(super) fn severity_name(severity: StatusSeverity) -> &'static str {
    match severity {
        StatusSeverity::Info => "Info",
        StatusSeverity::Warning => "Warning",
        StatusSeverity::Error => "Error",
    }
}

pub(super) fn domain_label(domain: StatusDomain) -> &'static str {
    match domain {
        StatusDomain::File => "File",
        StatusDomain::Disk => "Disk",
        StatusDomain::Search => "Search",
        StatusDomain::Settings => "Settings",
        StatusDomain::Session => "Session",
        StatusDomain::Encoding => "Encoding",
        StatusDomain::History => "History",
        StatusDomain::Layout => "Layout",
    }
}

/// What the list says when the filter leaves nothing to show.
pub(super) fn empty_message(filter: StatusFilter) -> &'static str {
    match filter {
        StatusFilter::All => "No status messages",
        StatusFilter::Errors => "No errors",
        StatusFilter::Warnings => "No warnings",
        StatusFilter::Info => "No info messages",
    }
}

/// Time of day for today's messages, with the date for older ones.
pub(super) fn clock(time: u64) -> String {
    clock_at(time, chrono::Local::now())
}

pub(super) fn clock_at(time: u64, now: chrono::DateTime<chrono::Local>) -> String {
    chrono::DateTime::from_timestamp(time as i64, 0).map_or_else(String::new, |date| {
        let date = date.with_timezone(&chrono::Local);
        date.format(if date.date_naive() == now.date_naive() {
            "%H:%M:%S"
        } else {
            "%b %-d %H:%M"
        })
        .to_string()
    })
}

pub(super) fn full_time(time: u64) -> String {
    chrono::DateTime::from_timestamp(time as i64, 0).map_or_else(String::new, |date| {
        date.with_timezone(&chrono::Local)
            .format("%Y-%m-%d %H:%M:%S")
            .to_string()
    })
}
