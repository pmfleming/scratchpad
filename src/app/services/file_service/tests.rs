use super::{FileService, staged_display_line_count};
use crate::app::domain::{EncodingSource, TextDocument, TextFormatMetadata, display_line_count};

#[test]
fn writing_snapshot_uses_captured_revision_text() {
    let tempdir = tempfile::tempdir().expect("create tempdir");
    let path = tempdir.path().join("snapshot.txt");
    let mut document = TextDocument::new("before".to_owned());
    let snapshot = document.snapshot();

    document.insert_direct(6, " after");

    FileService::write_snapshot_with_format(
        &path,
        &snapshot,
        &crate::app::domain::TextFormatMetadata::utf8_for_new_file("before"),
    )
    .expect("write snapshot");

    assert_eq!(
        std::fs::read_to_string(path).expect("read written file"),
        "before"
    );
}

#[test]
fn writing_fragmented_snapshot_with_utf16_preserves_content() {
    let tempdir = tempfile::tempdir().expect("create tempdir");
    let path = tempdir.path().join("snapshot-utf16.txt");
    let mut document = TextDocument::new("hello world".to_owned());
    document.insert_direct(5, " wide");
    let snapshot = document.snapshot();
    let format = TextFormatMetadata::detected(
        "hello wide world",
        "UTF-16LE".to_owned(),
        true,
        EncodingSource::ExplicitUserChoice,
        false,
    );

    FileService::write_snapshot_with_format(&path, &snapshot, &format)
        .expect("write fragmented snapshot");

    let reloaded = FileService::read_file(&path).expect("read UTF-16 snapshot");
    assert_eq!(reloaded.document.extract_text(), "hello wide world");
}

#[test]
fn opened_small_file_stages_metadata_refresh() {
    let tempdir = tempfile::tempdir().expect("create tempdir");
    let path = tempdir.path().join("small.txt");
    let content = "alpha\nbravo\ncharlie\n";
    std::fs::write(&path, content).expect("write small file");

    let file_content = FileService::read_file(&path).expect("read small file");
    let buffer = FileService::build_buffer_from_file_content(&path, file_content, None);

    assert!(buffer.text_metadata_refresh_needed());
    assert_eq!(buffer.line_count, content.matches('\n').count() + 1);
}

#[test]
fn opened_sample_sized_file_stages_metadata_refresh() {
    let tempdir = tempfile::tempdir().expect("create tempdir");
    let path = tempdir.path().join("sample-sized.txt");
    let content = "alpha\n".repeat((super::STAGED_METADATA_SAMPLE_BYTES / 6) + 1);
    std::fs::write(&path, &content).expect("write sample-sized file");

    let file_content = FileService::read_file(&path).expect("read sample-sized file");
    let buffer = FileService::build_buffer_from_file_content(&path, file_content, None);

    assert!(buffer.text_metadata_refresh_needed());
    assert_eq!(buffer.line_count, content.matches('\n').count() + 1);
}

#[test]
fn staged_line_count_matches_display_line_count_for_cr_and_mixed_endings() {
    let content = "alpha\rbravo\r\ncharlie\ndelta\r";

    assert_eq!(
        staged_display_line_count(content),
        display_line_count(content)
    );
}
