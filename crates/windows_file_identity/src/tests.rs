use super::same_file;
use std::{fs, io};

#[test]
fn identity_distinguishes_files_with_identical_contents() {
    let root = tempfile::tempdir().unwrap();
    let left = root.path().join("left.txt");
    let right = root.path().join("right.txt");
    fs::write(&left, "same contents").unwrap();
    fs::write(&right, "same contents").unwrap();
    assert!(same_file(&left, &left).unwrap());
    assert!(!same_file(&left, &right).unwrap());
}

#[test]
fn hard_link_identity_is_rechecked_after_replacement() {
    let root = tempfile::tempdir().unwrap();
    let original = root.path().join("original");
    let alias = root.path().join("alias");
    fs::write(&original, "original").unwrap();
    fs::hard_link(&original, &alias).unwrap();
    assert!(same_file(&original, &alias).unwrap());
    fs::remove_file(&original).unwrap();
    fs::write(&original, "replacement").unwrap();
    assert!(!same_file(&original, &alias).unwrap());
}

#[test]
fn directory_and_unicode_paths_support_identity_queries() {
    let root = tempfile::tempdir().unwrap();
    let directory = root.path().join("notes λ 🦀");
    fs::create_dir(&directory).unwrap();
    assert!(same_file(&directory, &directory).unwrap());
    assert!(!same_file(root.path(), &directory).unwrap());
    let file = directory.join("日本語.txt");
    fs::write(&file, "text").unwrap();
    assert!(same_file(&file, &file).unwrap());
}

#[test]
fn inaccessible_identity_is_an_error_not_a_match() {
    let root = tempfile::tempdir().unwrap();
    let existing = root.path().join("existing");
    let missing = root.path().join("missing");
    fs::write(&existing, "text").unwrap();
    for (left, right) in [(&existing, &missing), (&missing, &existing)] {
        assert_eq!(
            same_file(left, right).unwrap_err().kind(),
            io::ErrorKind::NotFound
        );
    }
    assert!(same_file(&existing, &root.path().join("invalid\0name")).is_err());
}
