use super::*;

#[test]
fn missing_targets_are_anchored_and_have_explicit_uncertainty() {
    let root = tempfile::tempdir().unwrap();
    let key = CanonicalPathKey::from_path_at(Path::new("missing"), root.path());
    assert_eq!(
        key.resolution(),
        PathResolution::Unresolved(io::ErrorKind::NotFound)
    );
    assert!(key.requested_path().is_absolute());
    assert_eq!(
        key.compare(&CanonicalPathKey::from_path(&root.path().join("missing"))),
        PathMatch::SameLocation
    );
    let dotted = CanonicalPathKey::from_path(&root.path().join("./missing"));
    assert_ne!(key, dotted);
    assert_eq!(key.compare(&dotted), PathMatch::Uncertain);
    let other =
        CanonicalPathKey::from_path_at(Path::new("missing"), &root.path().join("elsewhere"));
    assert_eq!(key.compare(&other), PathMatch::Uncertain);
    assert_eq!(
        CanonicalPathKey::from_path_at(Path::new("missing"), Path::new("relative")).compare(&key),
        PathMatch::Uncertain
    );
}

#[test]
fn hard_links_match_but_replacements_do_not_reuse_stale_identity() {
    let root = tempfile::tempdir().unwrap();
    let a = root.path().join("a");
    let b = root.path().join("b");
    std::fs::write(&a, "one").unwrap();
    std::fs::hard_link(&a, &b).unwrap();
    let key = CanonicalPathKey::from_path(&a);
    let alias = CanonicalPathKey::from_path(&b);
    assert_eq!(key.compare(&alias), PathMatch::SameFile);
    std::fs::remove_file(&a).unwrap();
    std::fs::write(&a, "replacement").unwrap();
    assert_eq!(key.compare(&alias), PathMatch::Different);
    assert_eq!(
        key.compare(&CanonicalPathKey::from_path(&a)),
        PathMatch::SameLocation
    );
}

#[test]
fn missing_case_variants_are_not_folded() {
    let root = tempfile::tempdir().unwrap();
    let a = CanonicalPathKey::from_path(&root.path().join("Missing"));
    let b = CanonicalPathKey::from_path(&root.path().join("missing"));
    assert_ne!(a, b);
    assert_eq!(a.compare(&b), PathMatch::Uncertain);
}

#[cfg(unix)]
#[test]
fn native_bytes_do_not_collide() {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};
    let root = tempfile::tempdir().unwrap();
    let a = root.path().join(OsString::from_vec(vec![0xff]));
    let b = root.path().join(OsString::from_vec(vec![0xfe]));
    std::fs::write(&a, "a").unwrap();
    std::fs::write(&b, "b").unwrap();
    assert_eq!(a.to_string_lossy(), b.to_string_lossy());
    assert_ne!(
        CanonicalPathKey::from_path(&a),
        CanonicalPathKey::from_path(&b)
    );
    assert!(!paths_match(&a, &b));
}

#[cfg(unix)]
#[test]
fn symlink_changes_and_parent_components_are_rechecked() {
    use std::os::unix::fs::symlink;
    let root = tempfile::tempdir().unwrap();
    let a = root.path().join("a");
    let b = root.path().join("b");
    let link = root.path().join("link");
    std::fs::create_dir_all(a.join("child")).unwrap();
    std::fs::create_dir(&b).unwrap();
    std::fs::write(a.join("file"), "a").unwrap();
    std::fs::write(root.path().join("file"), "other").unwrap();
    symlink(a.join("child"), &link).unwrap();
    let key = CanonicalPathKey::from_path(&link.join("../file"));
    assert!(key.matches(&CanonicalPathKey::from_path(&a.join("file"))));
    assert!(!key.matches(&CanonicalPathKey::from_path(&root.path().join("file"))));
    std::fs::remove_file(&link).unwrap();
    symlink(&b, &link).unwrap();
    assert!(!key.matches(&CanonicalPathKey::from_path(&a.join("file"))));
    assert!(key.matches(&CanonicalPathKey::from_path(&root.path().join("file"))));
}

#[test]
fn canonicalization_errors_are_not_reported_as_missing() {
    let root = tempfile::tempdir().unwrap();
    let file = root.path().join("file");
    std::fs::write(&file, "text").unwrap();
    let key = CanonicalPathKey::from_path(&file.join("child"));
    assert!(
        matches!(key.resolution(), PathResolution::Unresolved(kind) if kind != io::ErrorKind::NotFound)
    );
}
