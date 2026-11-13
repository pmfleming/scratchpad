#[must_use]
pub fn summarize_open_results(
    opened_count: usize,
    duplicate_count: usize,
    failure_count: usize,
    artifact_count: usize,
    last_artifact_warning: Option<String>,
) -> Option<String> {
    if opened_count == 1 && duplicate_count == 0 && failure_count == 0 {
        return last_artifact_warning;
    }

    let opened = match (opened_count, artifact_count) {
        (0, _) => None,
        (opened, 0) => Some(format!("Opened {}", file_count_label(opened))),
        (opened, artifacts) => Some(format!(
            "Opened {} ({} with control characters)",
            file_count_label(opened),
            file_count_label(artifacts)
        )),
    };
    let counted = |count: usize, outcome: &str| {
        (count > 0).then(|| format!("{} {outcome}", file_count_label(count)))
    };
    let parts = [
        opened,
        counted(duplicate_count, "already open"),
        counted(failure_count, "failed to open"),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>();
    (!parts.is_empty()).then(|| parts.join("; "))
}

#[must_use]
pub fn file_count_label(count: usize) -> String {
    pluralize(count, "file")
}

#[must_use]
pub fn pluralize(count: usize, singular: &str) -> String {
    if count == 1 {
        format!("1 {singular}")
    } else {
        format!("{count} {singular}s")
    }
}

#[cfg(test)]
mod tests {
    use super::{pluralize, summarize_open_results};

    #[test]
    fn pluralize_uses_singular_only_for_one() {
        assert_eq!(pluralize(0, "file"), "0 files");
        assert_eq!(pluralize(1, "file"), "1 file");
        assert_eq!(pluralize(2, "file"), "2 files");
    }

    #[test]
    fn open_summary_lists_each_nonzero_outcome() {
        let warning = || Some("warning".to_owned());
        for (counts, warning, expected) in [
            ((1, 0, 0, 1), warning(), Some("warning")),
            ((1, 0, 0, 0), None, None),
            ((0, 0, 0, 0), warning(), None),
            ((2, 0, 0, 0), None, Some("Opened 2 files")),
            (
                (3, 0, 0, 1),
                None,
                Some("Opened 3 files (1 file with control characters)"),
            ),
            (
                (1, 2, 1, 0),
                warning(),
                Some("Opened 1 file; 2 files already open; 1 file failed to open"),
            ),
            ((0, 1, 0, 0), None, Some("1 file already open")),
        ] {
            let (opened, duplicates, failures, artifacts) = counts;
            assert_eq!(
                summarize_open_results(opened, duplicates, failures, artifacts, warning).as_deref(),
                expected,
                "{counts:?}"
            );
        }
    }
}
