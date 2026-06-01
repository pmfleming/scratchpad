use super::state::SearchStripState;
use crate::app::app_state::{SearchResultEntry, SearchResultGroup};

pub(super) type SearchResultGroupKey = (usize, u64, usize);

pub(super) fn active_match_local_row(
    group: &SearchResultGroup,
    active_match_index: Option<usize>,
) -> Option<usize> {
    let active_match_index = active_match_index?;
    let group_range = group.first_match_index..group.first_match_index + group.total_match_count;
    group_range
        .contains(&active_match_index)
        .then(|| active_match_index - group.first_match_index)
}

pub(super) fn search_result_group_key(group: &SearchResultGroup) -> SearchResultGroupKey {
    (group.tab_index, group.buffer_id, group.first_match_index)
}

pub(super) fn file_match_count_label(match_count: usize) -> String {
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
    empty_message_from_parts(
        state.query.is_empty(),
        state.progress.status.message(),
        state.progress.searching,
        !state.result_groups.is_empty(),
    )
}

fn empty_message_from_parts<'a>(
    query_empty: bool,
    status_message: Option<&'a str>,
    searching: bool,
    has_result_groups: bool,
) -> Option<&'a str> {
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
    use super::{
        active_match_local_row, empty_message_from_parts, file_match_count_label, match_preview,
        search_result_group_key,
    };
    use crate::app::app_state::{SearchResultEntry, SearchResultGroup};

    fn group(first_match_index: usize, total_match_count: usize) -> SearchResultGroup {
        SearchResultGroup {
            tab_index: 2,
            buffer_id: 7,
            buffer_label: "notes.txt".to_owned(),
            tab_label: "Tab".to_owned(),
            first_match_index,
            total_match_count,
            active: false,
        }
    }

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
    fn active_match_local_row_requires_match_inside_group() {
        let group = group(10, 4);

        assert_eq!(active_match_local_row(&group, Some(10)), Some(0));
        assert_eq!(active_match_local_row(&group, Some(13)), Some(3));
        assert_eq!(active_match_local_row(&group, Some(9)), None);
        assert_eq!(active_match_local_row(&group, Some(14)), None);
        assert_eq!(active_match_local_row(&group, None), None);
    }

    #[test]
    fn search_result_group_key_uses_stable_identity_fields() {
        assert_eq!(search_result_group_key(&group(10, 4)), (2, 7, 10));
    }

    #[test]
    fn file_match_count_label_pluralizes_only_non_one_counts() {
        assert_eq!(file_match_count_label(0), "0 matches");
        assert_eq!(file_match_count_label(1), "1 match");
        assert_eq!(file_match_count_label(2), "2 matches");
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
