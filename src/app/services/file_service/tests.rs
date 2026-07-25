use super::FileService;
use crate::app::domain::{EncodingSource, LineEndingStyle, TextDocument, TextFormatMetadata};
use std::io;

#[test]
fn first_visible_window_stops_on_a_utf8_boundary() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("large.txt");
    std::fs::write(&path, "alpha café 東京 omega").unwrap();

    let window = FileService::read_first_visible_window(&path, 9).unwrap();

    assert_eq!(window.text, "alpha caf");
    assert_eq!(window.loaded_bytes, 9);
    assert_eq!(window.file_size_bytes, 24);
    assert!(!window.complete);
    assert!(!window.has_decoding_warnings);
}

#[test]
fn cold_file_shell_retains_path_and_disk_length_without_text_ingest() {
    let path = std::path::Path::new("large.txt");
    let disk_state = crate::app::domain::DiskFileState {
        modified_millis: Some(7),
        len: 1024 * 1024 * 1024,
    };

    let shell = FileService::build_cold_file_shell(path, Some(disk_state.clone()));

    assert_eq!(shell.path.as_deref(), Some(path));
    assert_eq!(shell.disk_state, Some(disk_state));
    assert_eq!(shell.document().piece_tree().len_bytes(), 0);
}

#[test]
fn snapshot_save_applies_crlf_policy_for_utf8() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("crlf.txt");
    let document = TextDocument::new("alpha\r\nbeta\ngamma\rdelta".to_owned());
    let format = TextFormatMetadata::detected(
        "alpha\r\nbeta",
        "UTF-8".to_owned(),
        false,
        EncodingSource::Heuristic,
        false,
    );

    FileService::write_snapshot_with_format(&path, &document.snapshot(), &format).unwrap();

    assert_eq!(
        std::fs::read(&path).unwrap(),
        b"alpha\r\nbeta\r\ngamma\r\ndelta"
    );
}

#[test]
fn snapshot_save_handles_crlf_split_across_spans() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("split-crlf.txt");
    let mut document = TextDocument::new("alpha\r".to_owned());
    document.insert_direct(6, "\nbeta\ngamma");
    let format = TextFormatMetadata::detected(
        "alpha\r\nbeta",
        "UTF-8".to_owned(),
        false,
        EncodingSource::Heuristic,
        false,
    );

    FileService::write_snapshot_with_format(&path, &document.snapshot(), &format).unwrap();

    assert_eq!(std::fs::read(&path).unwrap(), b"alpha\r\nbeta\r\ngamma");
}

#[test]
fn string_save_applies_line_ending_policy() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("string-write.txt");
    let format = TextFormatMetadata::detected(
        "alpha\r\nbeta",
        "UTF-8".to_owned(),
        false,
        EncodingSource::Heuristic,
        false,
    );

    FileService::write_file_with_format(&path, "alpha\r\nbeta\ngamma", &format).unwrap();

    assert_eq!(std::fs::read(&path).unwrap(), b"alpha\r\nbeta\r\ngamma");
}

#[test]
fn snapshot_save_preserves_mixed_line_endings() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("mixed.txt");
    let text = "alpha\r\nbeta\ngamma\rdelta";
    let document = TextDocument::new(text.to_owned());
    let format = TextFormatMetadata::detected(
        text,
        "UTF-8".to_owned(),
        false,
        EncodingSource::Heuristic,
        false,
    );

    FileService::write_snapshot_with_format(&path, &document.snapshot(), &format).unwrap();

    assert_eq!(std::fs::read(&path).unwrap(), text.as_bytes());
}

#[test]
fn snapshot_save_applies_lf_policy_for_utf16le() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("utf16le.txt");
    let document = TextDocument::new("alpha\r\nbeta\ngamma\rdelta".to_owned());
    let mut format = TextFormatMetadata::detected(
        "alpha\nbeta",
        "UTF-16LE".to_owned(),
        true,
        EncodingSource::Bom,
        false,
    );
    format.preferred_line_ending = LineEndingStyle::Lf;

    FileService::write_snapshot_with_format(&path, &document.snapshot(), &format).unwrap();

    let bytes = std::fs::read(&path).unwrap();
    assert_eq!(&bytes[..2], &[0xFF, 0xFE]);
    let decoded = String::from_utf16(
        &bytes[2..]
            .chunks_exact(2)
            .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
            .collect::<Vec<_>>(),
    )
    .unwrap();
    assert_eq!(decoded, "alpha\nbeta\ngamma\ndelta");
}

#[test]
fn snapshot_save_fails_for_unrepresentable_windows_1252_text() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("ansi.txt");
    let document = TextDocument::new("plain 😀".to_owned());
    let format = TextFormatMetadata::detected(
        "plain",
        "windows-1252".to_owned(),
        false,
        EncodingSource::Heuristic,
        false,
    );

    let error =
        FileService::write_snapshot_with_format(&path, &document.snapshot(), &format).unwrap_err();

    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    assert!(error.to_string().contains("not representable"));
}
