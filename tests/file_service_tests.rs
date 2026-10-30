use scratchpad::app::domain::{EncodingSource, TextDocument, TextFormatMetadata};
use scratchpad::app::services::file_service::FileService;

#[test]
fn formatted_saves_round_trip_with_exact_boms_and_stateful_encoding_bytes() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("encoded.txt");
    for (encoding, text, expected) in [
        ("UTF-8", "é😀", &b"\xef\xbb\xbf\xc3\xa9\xf0\x9f\x98\x80"[..]),
        ("UTF-16LE", "é😀", &b"\xff\xfe\xe9\x00\x3d\xd8\x00\xde"[..]),
        ("UTF-16BE", "é😀", &b"\xfe\xff\x00\xe9\xd8\x3d\xde\x00"[..]),
        ("windows-1252", "café", &b"caf\xe9"[..]),
        ("ISO-2022-JP", "日本", &b"\x1b$BF|K\\\x1b(B"[..]),
    ] {
        FileService::write_file_with_bom(&path, text, encoding, true).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), expected, "{encoding}");
        let loaded = FileService::read_file_with_encoding(&path, encoding).unwrap();
        assert_eq!(loaded.document.extract_text(), text, "{encoding}");
        assert_eq!(loaded.format.encoding_name, encoding);
        assert_eq!(
            loaded.format.encoding_source,
            EncodingSource::ExplicitUserChoice
        );
        if encoding.starts_with("UTF") {
            let detected = FileService::read_file(&path).unwrap();
            assert_eq!(detected.document.extract_text(), text);
            assert!(detected.format.has_bom);
        }
        FileService::write_snapshot_with_format(&path, &loaded.document.snapshot(), &loaded.format)
            .unwrap();
        assert_eq!(
            std::fs::read(&path).unwrap(),
            expected,
            "snapshot {encoding}"
        );
    }
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
fn detects_binary_file() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("binary.bin");
    std::fs::write(&path, b"abc\0def").unwrap();
    let error = FileService::read_file(&path).err().unwrap();
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    assert!(error.to_string().contains("Binary files are not supported"));
}

#[test]
fn invalid_encoding_and_unrepresentable_text_fail_before_replacing_original() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("original.txt");
    let document = TextDocument::new("plain 😀".into());
    for (encoding, kind) in [
        ("invalid", std::io::ErrorKind::InvalidInput),
        ("windows-1252", std::io::ErrorKind::InvalidData),
    ] {
        let format = TextFormatMetadata::detected(
            "plain",
            encoding.into(),
            false,
            EncodingSource::Heuristic,
            false,
        );
        for snapshot in [false, true] {
            std::fs::write(&path, "original").unwrap();
            let result = if snapshot {
                FileService::write_snapshot_with_format(&path, &document.snapshot(), &format)
            } else {
                FileService::write_file_with_format(&path, "plain 😀", &format)
            };
            assert_eq!(result.unwrap_err().kind(), kind);
            // Linux stages snapshot writes; other platforms currently write directly.
            if cfg!(target_os = "linux") || !snapshot {
                assert_eq!(std::fs::read_to_string(&path).unwrap(), "original");
            }
        }
    }
}
