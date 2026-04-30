use crate::app::transactions::TransactionLogEntry;
use egui_phosphor::regular::{ARROWS_SPLIT, FILE_TEXT, PENCIL_SIMPLE_LINE};

const TRANSACTION_LOG_TOKEN_MAX_CHARS: usize = 28;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum TransactionFilter {
    All,
    FileChanges,
    TabOperations,
    Modifications,
}

impl TransactionFilter {
    pub(super) const ALL: [Self; 4] = [
        Self::All,
        Self::FileChanges,
        Self::TabOperations,
        Self::Modifications,
    ];

    pub(super) fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::FileChanges => "File changes",
            Self::TabOperations => "Tab operations",
            Self::Modifications => "Modifications",
        }
    }

    fn matches_category(self, category: TransactionCategory) -> bool {
        matches!(self, Self::All)
            || matches!(
                (self, category),
                (Self::FileChanges, TransactionCategory::FileChanges)
                    | (Self::TabOperations, TransactionCategory::TabOperations)
                    | (Self::Modifications, TransactionCategory::Modifications)
            )
    }

    pub(super) fn persisted_value(self) -> u8 {
        match self {
            Self::All => 0,
            Self::FileChanges => 1,
            Self::TabOperations => 2,
            Self::Modifications => 3,
        }
    }

    pub(super) fn from_persisted_value(value: u8) -> Self {
        match value {
            1 => Self::FileChanges,
            2 => Self::TabOperations,
            3 => Self::Modifications,
            _ => Self::All,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum TransactionCategory {
    FileChanges,
    TabOperations,
    Modifications,
}

impl TransactionCategory {
    pub(super) fn icon(self) -> &'static str {
        match self {
            Self::FileChanges => FILE_TEXT,
            Self::TabOperations => ARROWS_SPLIT,
            Self::Modifications => PENCIL_SIMPLE_LINE,
        }
    }

    pub(super) fn from_entry(entry: &TransactionLogEntry) -> Self {
        let label = entry.action_label.to_ascii_lowercase();
        if contains_any(&label, &["tab", "split", "combine", "promote", "view"]) {
            Self::TabOperations
        } else if contains_any(
            &label,
            &["file", "save", "open", "reload", "reopen", "rename"],
        ) {
            Self::FileChanges
        } else {
            Self::Modifications
        }
    }
}

pub(super) struct TransactionLogToken {
    pub(super) display: String,
    pub(super) full: String,
}

pub(super) fn filtered_entries(
    entries: &[TransactionLogEntry],
    filter: TransactionFilter,
) -> Vec<&TransactionLogEntry> {
    entries
        .iter()
        .filter(|entry| filter.matches_category(TransactionCategory::from_entry(entry)))
        .collect()
}

pub(super) fn transaction_log_tokens(entry: &TransactionLogEntry) -> Vec<TransactionLogToken> {
    representative_transaction_token(entry)
        .into_iter()
        .collect()
}

fn representative_transaction_token(entry: &TransactionLogEntry) -> Option<TransactionLogToken> {
    if entry.affected_items.is_empty() {
        return entry
            .details
            .as_deref()
            .filter(|details| !details.trim().is_empty())
            .map(|details| TransactionLogToken {
                display: truncate_transaction_token(details),
                full: details.to_owned(),
            });
    }

    let target = entry
        .affected_items
        .iter()
        .rev()
        .find_map(|item| {
            item.strip_prefix("target: ")
                .or_else(|| item.strip_prefix("Target: "))
        })
        .or_else(|| entry.affected_items.first().map(String::as_str))
        .unwrap_or_default();

    let mut tooltip_lines = entry.affected_items.clone();
    if let Some(details) = entry
        .details
        .as_deref()
        .filter(|details| !details.trim().is_empty())
    {
        tooltip_lines.push(details.to_owned());
    }

    Some(TransactionLogToken {
        display: truncate_transaction_token(target),
        full: tooltip_lines.join("\n"),
    })
}

fn truncate_transaction_token(token: &str) -> String {
    let trimmed = token.trim();
    let char_count = trimmed.chars().count();
    if char_count <= TRANSACTION_LOG_TOKEN_MAX_CHARS {
        return trimmed.to_owned();
    }

    let mut truncated = trimmed
        .chars()
        .take(TRANSACTION_LOG_TOKEN_MAX_CHARS.saturating_sub(1))
        .collect::<String>();
    truncated.push('…');
    truncated
}

pub(super) fn entry_count_label(entry_count: usize) -> String {
    if entry_count == 1 {
        "1 entry".to_owned()
    } else {
        format!("{entry_count} entries")
    }
}

fn contains_any(text: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| text.contains(needle))
}
