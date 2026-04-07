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
    assert_eq!(read.encoding, "UTF-8");
    assert!(!read.has_bom);
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
        &read.encoding,
        read.has_bom,
    )
    .unwrap();
    let bytes = fs::read(&path).unwrap();

    assert_eq!(read.content, content);
    assert_eq!(read.encoding, "UTF-16LE");
    assert!(read.has_bom);
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
    assert_eq!(read.encoding, "Shift_JIS");
    assert!(!read.has_bom);
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
    assert_eq!(read.encoding, "windows-1252");
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
    FileService::write_file_with_bom(&path, &read.content, &read.encoding, read.has_bom).unwrap();

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
}
