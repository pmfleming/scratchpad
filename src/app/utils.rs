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

    let mut parts = Vec::new();

    if opened_count > 0 {
        if artifact_count > 0 {
            parts.push(format!(
                "Opened {} ({} with control characters)",
                file_count_label(opened_count),
                file_count_label(artifact_count)
            ));
        } else {
            parts.push(format!("Opened {}", file_count_label(opened_count)));
        }
    }

    if duplicate_count > 0 {
        parts.push(format!(
            "{} already open",
            file_count_label(duplicate_count)
        ));
    }

    if failure_count > 0 {
        parts.push(format!(
            "{} failed to open",
            file_count_label(failure_count)
        ));
    }

    if parts.is_empty() {
        None
    } else {
        Some(parts.join("; "))
    }
}

pub fn file_count_label(count: usize) -> String {
    if count == 1 {
        "1 file".to_owned()
    } else {
        format!("{count} files")
    }
}
