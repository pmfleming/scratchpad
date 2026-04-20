#![forbid(unsafe_code)]

use scratchpad::app::domain::LineEndingStyle;
use scratchpad::app::domain::TextDocument;
use scratchpad::app::services::file_service::FileService;
use std::fs;
use tempfile::tempdir;

#[test]
fn read_write_utf8() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("utf8.txt");
    let content = "Hello, world! 🌍";

    FileService::write_file_with_bom(&path, content, "UTF-8", false).unwrap();
    let read = FileService::read_file(&path).unwrap();

    assert_eq!(read.content, content);
    assert_eq!(read.format.encoding_name, "UTF-8");
    assert!(!read.format.has_bom);
    assert_eq!(read.format.line_endings, LineEndingStyle::None);
}

#[test]
fn read_write_utf16le() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("utf16le.txt");
    let content = "Hello, UTF-16! 🚀";

    FileService::write_file_with_bom(&path, content, "UTF-16LE", true).unwrap();

    let read = FileService::read_file(&path).unwrap();
    FileService::write_file_with_bom(
        &path,
        &(read.content.clone() + "!"),
        &read.format.encoding_name,
        read.format.has_bom,
    )
    .unwrap();
    let bytes = fs::read(&path).unwrap();

    assert_eq!(read.content, content);
    assert_eq!(read.format.encoding_name, "UTF-16LE");
    assert!(read.format.has_bom);
    assert_eq!(&bytes[..2], &[0xFF, 0xFE]);
}

#[test]
fn read_write_shift_jis() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("sjis.txt");
    let content = "こんにちは";

    FileService::write_file_with_bom(&path, content, "Shift_JIS", false).unwrap();
    let read = FileService::read_file(&path).unwrap();

    assert_eq!(read.content, content);
    assert_eq!(read.format.encoding_name, "Shift_JIS");
    assert!(!read.format.has_bom);
}

#[test]
fn read_write_windows_1252() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("cp1252.txt");
    let content = "caf\u{00E9} \u{2014} na\u{00EF}ve";

    FileService::write_file_with_bom(&path, content, "windows-1252", false).unwrap();
    let read = FileService::read_file(&path).unwrap();
    let bytes = fs::read(&path).unwrap();

    assert_eq!(read.content, content);
    assert_eq!(read.format.encoding_name, "windows-1252");
    assert_eq!(
        bytes,
        vec![
            0x63, 0x61, 0x66, 0xE9, 0x20, 0x97, 0x20, 0x6E, 0x61, 0xEF, 0x76, 0x65
        ]
    );
}

#[test]
fn preserves_encoding_when_round_tripping_windows_1252() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("roundtrip-cp1252.txt");
    let original = vec![0x63, 0x61, 0x66, 0xE9];
    fs::write(&path, &original).unwrap();

    let mut read = FileService::read_file(&path).unwrap();
    read.content.push('!');
    FileService::write_file_with_format(&path, &read.content, &read.format).unwrap();

    assert_eq!(fs::read(&path).unwrap(), vec![0x63, 0x61, 0x66, 0xE9, 0x21]);
}

#[test]
fn detect_binary_file() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("binary.bin");
    let content = vec![0u8, 1, 2, 3, 0, 4, 5];

    fs::write(&path, content).unwrap();
    let result = FileService::read_file(&path);

    assert!(result.is_err());
    assert_eq!(
        result.unwrap_err().to_string(),
        "Binary files are not supported"
    );
}

#[test]
fn detects_artifacts_without_treating_crlf_as_control_chars() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("artifacts.txt");
    let content = "plain\r\ntext\r\n\u{1b}[31mcolor\u{1b}[0m\rprogress\u{0008}";

    FileService::write_file_with_bom(&path, content, "UTF-8", false).unwrap();
    let read = FileService::read_file(&path).unwrap();

    assert!(read.artifact_summary.has_ansi_sequences);
    assert!(read.artifact_summary.has_carriage_returns);
    assert!(read.artifact_summary.has_backspaces);
    assert_eq!(read.format.line_endings, LineEndingStyle::Mixed);
}

#[test]
fn detects_cr_only_line_endings_without_reporting_artifacts() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("classic-mac.txt");
    let content = "alpha\rbeta\r";

    FileService::write_file_with_bom(&path, content, "UTF-8", false).unwrap();
    let read = FileService::read_file(&path).unwrap();

    assert_eq!(read.format.line_endings, LineEndingStyle::Cr);
    assert!(!read.artifact_summary.has_control_chars());
}

#[test]
fn preserves_loaded_crlf_style_when_editing_and_saving() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("windows-lines.txt");
    fs::write(&path, b"alpha\r\nbeta\r\n").unwrap();

    let read = FileService::read_file(&path).unwrap();
    let mut document = TextDocument::with_preferred_line_ending(
        read.content.clone(),
        read.format.preferred_line_ending_style(),
    );
    let insert_at = document.piece_tree().len_chars();

    document.insert_direct(insert_at, "gamma\r\n");
    let text = document.extract_text();
    FileService::write_file_with_format(&path, &text, &read.format).unwrap();

    assert_eq!(fs::read(&path).unwrap(), b"alpha\r\nbeta\r\ngamma\r\n");
}

#[test]
fn explicit_reopen_with_encoding_uses_selected_encoding() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("latin1ish.txt");
    fs::write(&path, vec![0x63, 0x61, 0x66, 0xE9]).unwrap();

    let read = FileService::read_file_with_encoding(&path, "windows-1252").unwrap();

    assert_eq!(read.content, "caf\u{00E9}");
    assert_eq!(read.format.encoding_name, "windows-1252");
    assert_eq!(
        read.format.encoding_source,
        scratchpad::app::domain::EncodingSource::ExplicitUserChoice
    );
}

#[test]
fn saving_unencodable_text_fails_for_legacy_encoding() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("cp1252-save-fail.txt");
    let format = scratchpad::app::domain::TextFormatMetadata::detected(
        "caf\u{00E9}",
        "windows-1252".to_owned(),
        false,
        scratchpad::app::domain::EncodingSource::ExplicitUserChoice,
        false,
    );

    let error = FileService::write_file_with_format(&path, "emoji \u{1F600}", &format)
        .expect_err("save should fail for unencodable text");

    assert!(
        error
            .to_string()
            .contains("not representable in windows-1252")
    );
}
