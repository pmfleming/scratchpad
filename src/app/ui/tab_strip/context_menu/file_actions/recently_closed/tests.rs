use super::*;

fn file(path: &Path, at: Option<i64>) -> RecentlyClosedFile {
    RecentlyClosedFile {
        path: path.to_owned(),
        closed_at_unix_seconds: at,
    }
}

fn archive(token: &str, paths: Vec<PathBuf>, at: i64, unsaved: bool) -> ClosedTabInfo {
    ClosedTabInfo {
        token: token.into(),
        name: "note.txt".into(),
        paths,
        closed_at_unix_seconds: at,
        has_unsaved_changes: unsaved,
    }
}

#[test]
fn merge_sorts_sources_together_deduplicates_paths_and_keeps_archive_versions() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("note.txt");
    std::fs::write(&path, "disk").unwrap();
    let recent = root.path().join("recent.txt");
    let legacy = root.path().join("legacy.txt");
    let entries = merged_entries(
        vec![
            archive("new", vec![path.clone()], 30, true),
            archive("old", vec![path.clone()], 10, false),
        ],
        vec![
            file(&recent, Some(20)),
            file(&path, Some(40)),
            file(&legacy, None),
            file(&recent, Some(15)),
        ],
    );
    assert_eq!(
        entries
            .iter()
            .map(|entry| &entry.target)
            .collect::<Vec<_>>(),
        vec![
            &ReopenTarget::Archive("new".into()),
            &ReopenTarget::File(recent),
            &ReopenTarget::Archive("old".into()),
            &ReopenTarget::File(legacy),
        ]
    );
    assert!(entries[0].unsaved);
    assert_eq!(entries[0].disk_icon(), Some(FILE));
    assert!(!entries[2].unsaved);
    assert_eq!(entries[0].paths[0].1, DiskStatus::OnDisk);
}

#[test]
fn missing_archives_remain_recoverable_but_missing_file_only_entries_do_not() {
    let root = tempfile::tempdir().unwrap();
    let missing = root.path().join("missing.txt");
    let other_missing = root.path().join("other.txt");
    let entries = merged_entries(
        vec![archive("saved", vec![missing], 1, false)],
        vec![file(&other_missing, None)],
    );
    assert!(entries[0].can_reopen());
    assert_eq!(entries[0].disk_icon(), Some(FILE_X));
    assert!(!entries[1].can_reopen());
    assert_eq!(entries[1].disk_icon(), Some(FILE_X));
    assert!(entries[1].tooltip().contains("no retained tab copy"));
}

#[test]
fn split_tab_status_reports_each_file_and_untitled_entries_use_only_unsaved_dot() {
    let root = tempfile::tempdir().unwrap();
    let exists = root.path().join("exists.txt");
    let missing = root.path().join("missing.txt");
    std::fs::write(&exists, "text").unwrap();
    let entries = merged_entries(
        vec![
            archive("split", vec![exists.clone(), missing.clone()], 2, true),
            archive("untitled", vec![], 1, true),
        ],
        vec![],
    );
    let split = &entries[0];
    assert!(split.can_reopen());
    assert!(split.unsaved);
    assert_eq!(split.disk_icon(), Some(FILE_X));
    let tooltip = split.tooltip();
    for detail in [
        "Some files missing",
        "Unsaved content retained",
        "exists.txt — On disk",
        "missing.txt — Missing",
    ] {
        assert!(tooltip.contains(detail), "missing {detail}: {tooltip}");
    }
    assert!(entries[1].can_reopen());
    assert!(entries[1].unsaved);
    assert_eq!(entries[1].disk_icon(), None);
}

#[test]
fn access_errors_are_not_misreported_as_missing_files() {
    assert_eq!(
        DiskStatus::from_metadata(Err(io::ErrorKind::PermissionDenied.into())),
        DiskStatus::Unavailable
    );
    assert_eq!(
        DiskStatus::from_metadata(Err(io::ErrorKind::NotFound.into())),
        DiskStatus::Missing
    );
    let root = tempfile::tempdir().unwrap();
    let entries = merged_entries(vec![], vec![file(root.path(), None)]);
    assert_eq!(entries[0].disk_icon(), Some(WARNING_CIRCLE));
    assert!(!entries[0].can_reopen());
    assert!(entries[0].tooltip().contains("Unavailable"));
}
