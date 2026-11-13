//! What changed between the disk file and the tab, read off the UI thread.
use crate::app::domain::DocumentSnapshot;
use crate::app::services::file_service::FileService;
use std::path::PathBuf;

/// Disk files above this are not compared.
const COMPARE_LIMIT: u64 = 32 * 1024 * 1024;
/// Lines shown on each side of the summary hunk.
const HUNK_LINES: usize = 3;

pub(crate) struct DiskDiff {
    /// One-based line where the versions first differ.
    pub(crate) line: usize,
    pub(crate) removed: Vec<String>,
    pub(crate) added: Vec<String>,
    /// More changed lines exist than the hunk shows.
    pub(crate) more: bool,
    pub(crate) disk: String,
    pub(crate) tab: String,
}

pub(crate) fn load(
    path: &PathBuf,
    encoding: &str,
    snapshot: &DocumentSnapshot,
) -> std::io::Result<DiskDiff> {
    if std::fs::metadata(path)?.len() > COMPARE_LIMIT {
        return Err(std::io::Error::other("Too large to compare"));
    }
    let disk = FileService::read_file_with_encoding(path, encoding)?
        .document
        .try_extract_text()?;
    let tab = snapshot.extract_text();
    Ok(compare(disk, tab))
}

pub(crate) fn compare(disk: String, tab: String) -> DiskDiff {
    let old: Vec<&str> = disk.lines().collect();
    let new: Vec<&str> = tab.lines().collect();
    let prefix = old.iter().zip(&new).take_while(|(a, b)| a == b).count();
    let suffix = old[prefix..]
        .iter()
        .rev()
        .zip(new[prefix..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let removed = &old[prefix..old.len() - suffix];
    let added = &new[prefix..new.len() - suffix];
    let take = |lines: &[&str]| -> Vec<String> {
        lines
            .iter()
            .take(HUNK_LINES)
            .map(|line| (*line).to_owned())
            .collect()
    };
    DiskDiff {
        line: prefix + 1,
        more: removed.len() > HUNK_LINES || added.len() > HUNK_LINES,
        removed: take(removed),
        added: take(added),
        disk,
        tab,
    }
}

#[cfg(test)]
mod tests {
    use super::compare;

    #[test]
    fn the_hunk_starts_at_the_first_changed_line_and_skips_the_shared_end() {
        let diff = compare("a\nb\nc\nd".into(), "a\nB\nB2\nc\nd".into());
        assert_eq!(diff.line, 2);
        assert_eq!(diff.removed, ["b"]);
        assert_eq!(diff.added, ["B", "B2"]);
        assert!(!diff.more);
    }

    #[test]
    fn identical_text_has_an_empty_hunk() {
        let diff = compare("same\n".into(), "same\n".into());
        assert!(diff.removed.is_empty() && diff.added.is_empty());
    }
}
