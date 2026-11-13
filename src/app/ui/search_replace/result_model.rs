use super::state::SearchStripState;
use crate::app::app_state::SearchResultEntry;

pub(super) fn match_count_label(match_count: usize) -> String {
    if match_count == 1 {
        "1 match".to_owned()
    } else {
        format!("{match_count} matches")
    }
}

pub(super) fn match_preview(entry: &SearchResultEntry) -> &str {
    if entry.preview.is_empty() {
        "Match"
    } else {
        entry.preview.as_str()
    }
}

pub(super) fn empty_message(state: &SearchStripState) -> Option<&str> {
    if !state.result_groups.is_empty()
        && state.progress.status
            == crate::app::app_state::SearchStatus::Error(
                crate::app::app_state::search_state::SEARCH_RESULT_LIMIT_MESSAGE.into(),
            )
    {
        return None;
    }
    empty_message_from_parts(
        state.query.is_empty(),
        state.progress.status.message(),
        state.progress.searching,
        !state.result_groups.is_empty(),
    )
}

fn empty_message_from_parts(
    query_empty: bool,
    status_message: Option<&str>,
    searching: bool,
    has_result_groups: bool,
) -> Option<&str> {
    if query_empty {
        Some("Type to search across the selected scope.")
    } else if let Some(message) = status_message {
        Some(message)
    } else if !has_result_groups {
        if searching {
            Some("Searching\u{2026}")
        } else {
            Some("No matches found.")
        }
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::{empty_message_from_parts, match_count_label, match_preview};
    use crate::app::app_state::SearchResultEntry;

    fn entry(preview: &str) -> SearchResultEntry {
        SearchResultEntry {
            match_index: 4,
            buffer_id: 7,
            buffer_label: "notes.txt".to_owned(),
            line_number: 3,
            column_number: 8,
            preview: preview.to_owned(),
            active: false,
        }
    }

    #[test]
    fn match_count_label_pluralizes_only_non_one_counts() {
        assert_eq!(match_count_label(0), "0 matches");
        assert_eq!(match_count_label(1), "1 match");
        assert_eq!(match_count_label(2), "2 matches");
    }

    #[test]
    fn match_preview_uses_placeholder_for_empty_preview() {
        assert_eq!(match_preview(&entry("")), "Match");
        assert_eq!(
            match_preview(&entry("needle in haystack")),
            "needle in haystack"
        );
    }

    #[test]
    fn empty_message_prioritizes_query_status_then_result_presence() {
        assert_eq!(
            empty_message_from_parts(true, None, false, false),
            Some("Type to search across the selected scope.")
        );
        assert_eq!(
            empty_message_from_parts(false, Some("Invalid query"), false, false),
            Some("Invalid query")
        );
        assert_eq!(
            empty_message_from_parts(false, None, true, false),
            Some("Searching\u{2026}")
        );
        assert_eq!(
            empty_message_from_parts(false, None, false, false),
            Some("No matches found.")
        );
        assert_eq!(empty_message_from_parts(false, None, false, true), None);
    }
}
