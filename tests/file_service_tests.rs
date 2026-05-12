use scratchpad::app::domain::{EncodingSource, LineEndingStyle, TextDocument, TextFormatMetadata};
use scratchpad::app::services::file_service::FileService;

#[test]
fn read_write_utf8_round_trips_text() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("utf8.txt");
    let text = "alpha\nβeta\n😀";

    FileService::write_file_with_bom(&path, text, "UTF-8", false).unwrap();
    let loaded = FileService::read_file(&path).unwrap();

    assert_eq!(loaded.document.extract_text(), text);
    assert_eq!(loaded.format.encoding_name, "UTF-8");
}

#[test]
fn read_large_utf8_file_round_trips_across_decode_chunks() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("large-utf8.txt");
    let mut text = "header café 東京\n".repeat(12_000);
    text.push_str("tail 😀");
    std::fs::write(&path, &text).unwrap();

    let loaded = FileService::read_file(&path).unwrap();

    assert_eq!(loaded.document.extract_text(), text);
    assert_eq!(loaded.document.snapshot().line_count(), 12_001);
}

#[test]
fn read_write_utf16le_round_trips_text_with_bom() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("utf16le.txt");
    let text = "alpha\nβeta";

    FileService::write_file_with_bom(&path, text, "UTF-16LE", true).unwrap();
    let bytes = std::fs::read(&path).unwrap();
    assert_eq!(&bytes[..2], &[0xff, 0xfe]);

    let loaded = FileService::read_file(&path).unwrap();

    assert_eq!(loaded.document.extract_text(), text);
    assert_eq!(loaded.format.encoding_name, "UTF-16LE");
    assert!(loaded.format.has_bom);
}

#[test]
fn detects_binary_file() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("binary.bin");
    std::fs::write(&path, b"abc\0def").unwrap();

    let error = match FileService::read_file(&path) {
        Ok(_) => panic!("binary file unexpectedly loaded"),
        Err(error) => error,
    };

    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    assert!(error.to_string().contains("Binary files are not supported"));
}

#[test]
fn explicit_reopen_with_encoding_uses_selected_encoding() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("ansi.txt");
    std::fs::write(&path, [0x63, 0x61, 0x66, 0xe9]).unwrap();

    let loaded = FileService::read_file_with_encoding(&path, "windows-1252").unwrap();

    assert_eq!(loaded.document.extract_text(), "café");
    assert_eq!(
        loaded.format.encoding_source,
        EncodingSource::ExplicitUserChoice
    );
}

#[test]
fn saving_unencodable_text_fails_for_legacy_encoding() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("ansi.txt");
    let format = TextFormatMetadata::detected(
        "plain",
        "windows-1252".to_owned(),
        false,
        EncodingSource::Heuristic,
        false,
    );

    let error = FileService::write_file_with_format(&path, "plain 😀", &format).unwrap_err();

    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
}

#[test]
fn snapshot_save_applies_configured_line_ending_policy() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("crlf.txt");
    let document = TextDocument::new("alpha\r\nbeta\ngamma\rdelta".to_owned());
    let mut format = TextFormatMetadata::detected(
        "alpha\r\nbeta",
        "UTF-8".to_owned(),
        false,
        EncodingSource::Heuristic,
        false,
    );
    format.preferred_line_ending = LineEndingStyle::Crlf;

    FileService::write_snapshot_with_format(&path, &document.snapshot(), &format).unwrap();

    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "alpha\r\nbeta\r\ngamma\r\ndelta"
    );
}
