use super::FileService;
use crate::app::domain::{EncodingSource, LineEndingStyle, TextDocument, TextFormatMetadata};
use std::io;

#[test]
fn unreadable_snapshot_save_preserves_the_existing_destination() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("source");
    let target = directory.path().join("target");
    std::fs::write(&source, "aλ\r\n".repeat(128)).unwrap();
    std::fs::write(&target, "keep this exact destination").unwrap();
    let (document, _, _) = TextDocument::from_utf8_file(&source, 0, 0).unwrap();
    document.piece_tree().truncate_backing_for_test().unwrap();
    assert!(FileService::write_snapshot_utf8(&target, &document.snapshot()).is_err());
    assert_eq!(
        std::fs::read_to_string(&target).unwrap(),
        "keep this exact destination"
    );
}

#[test]
fn large_utf8_file_uses_lazy_file_backed_piece_tree_chunks() {
    const MB: usize = 1024 * 1024;
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("file-backed.txt");
    let text = "alpha café 東京\n".repeat((17 * MB) / "alpha café 東京\n".len() + 1);
    std::fs::write(&path, text.as_bytes()).unwrap();

    let content = FileService::read_file(&path).unwrap();
    let tree = content.document.piece_tree();

    assert!(tree.is_file_backed());
    assert_eq!(tree.loaded_file_chunk_count(), 0);
    assert_eq!(tree.borrow_range(0..5).as_deref(), Some("alpha"));
    assert_eq!(tree.loaded_file_chunk_count(), 1);
    let tail = tree.len_chars().saturating_sub(5)..tree.len_chars();
    assert_eq!(tree.extract_range(tail).chars().count(), 5);
    assert_eq!(tree.loaded_file_chunk_count(), 2);

    let mut document = content.document;
    document.insert_direct(5, " lazy");
    assert_eq!(document.piece_tree().extract_range(0..10), "alpha lazy");

    let saved = directory.path().join("file-backed-saved.txt");
    FileService::write_snapshot_utf8(&saved, &document.snapshot()).unwrap();
    assert_eq!(
        std::fs::metadata(&saved).unwrap().len(),
        text.len() as u64 + 5
    );
    assert!(
        std::fs::read_to_string(saved)
            .unwrap()
            .starts_with("alpha lazy")
    );
}

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
fn formatted_writers_agree_across_encodings_newlines_and_piece_boundaries() {
    let text = "café\r\nx\ry\nz\r";
    for encoding in [
        encoding_rs::UTF_8,
        encoding_rs::UTF_16LE,
        encoding_rs::UTF_16BE,
        encoding_rs::WINDOWS_1252,
    ] {
        for (style, normalized) in [
            (LineEndingStyle::Lf, "café\nx\ny\nz\n"),
            (LineEndingStyle::Cr, "café\rx\ry\rz\r"),
            (LineEndingStyle::Crlf, "café\r\nx\r\ny\r\nz\r\n"),
            (LineEndingStyle::Mixed, text),
        ] {
            for has_bom in [false, true] {
                let mut format = TextFormatMetadata::detected(
                    text,
                    encoding.name().into(),
                    has_bom,
                    EncodingSource::ExplicitUserChoice,
                    false,
                );
                format.line_endings = style;
                format.preferred_line_ending = style;
                let expected = encoded_bytes(normalized, encoding, has_bom);
                assert_piece_boundaries(text, &format, &expected);
            }
        }
    }
}

fn encoded_bytes(text: &str, encoding: &'static encoding_rs::Encoding, bom: bool) -> Vec<u8> {
    let (mut bytes, payload) = match encoding.name() {
        "UTF-16LE" => (
            vec![0xff, 0xfe],
            text.encode_utf16().flat_map(u16::to_le_bytes).collect(),
        ),
        "UTF-16BE" => (
            vec![0xfe, 0xff],
            text.encode_utf16().flat_map(u16::to_be_bytes).collect(),
        ),
        "UTF-8" => (vec![0xef, 0xbb, 0xbf], text.as_bytes().to_vec()),
        _ => (Vec::new(), encoding.encode(text).0.into_owned()),
    };
    if !bom {
        bytes.clear();
    }
    bytes.extend(payload);
    bytes
}

#[test]
fn encoded_streams_flush_state_and_propagate_write_errors() {
    for (encoding, text) in [
        ("UTF-8", "😀"),
        ("UTF-16LE", "😀"),
        ("UTF-16BE", "😀"),
        ("windows-1252", "café"),
        ("ISO-2022-JP", "日本"),
    ] {
        for text in [String::new(), text.repeat(9000)] {
            let format = TextFormatMetadata::detected(
                &text,
                encoding.into(),
                true,
                EncodingSource::ExplicitUserChoice,
                false,
            );
            let document = TextDocument::new(text.clone());
            let mut bytes = Vec::new();
            super::write::write_snapshot_to_writer(&mut bytes, &document.snapshot(), &format)
                .unwrap();
            assert_eq!(
                bytes,
                encoded_bytes(
                    &text,
                    encoding_rs::Encoding::for_label(encoding.as_bytes()).unwrap(),
                    true
                )
            );
            assert_write_errors(&document, &format, bytes.len());
        }
    }
}

fn assert_piece_boundaries(text: &str, format: &TextFormatMetadata, expected: &[u8]) {
    let mut bytes = Vec::new();
    super::write::write_content_to_writer(&mut bytes, text, format).unwrap();
    assert_eq!(bytes, expected, "{format:?}");
    for split in 0..=text.chars().count() {
        let mut document = TextDocument::new(text.chars().take(split).collect());
        document.insert_direct(split, &text.chars().skip(split).collect::<String>());
        bytes.clear();
        super::write::write_snapshot_to_writer(&mut bytes, &document.snapshot(), format).unwrap();
        assert_eq!(bytes, expected, "{format:?} split={split}");
    }
}

fn assert_write_errors(document: &TextDocument, format: &TextFormatMetadata, byte_len: usize) {
    let Some(last_byte) = byte_len.checked_sub(1) else {
        return;
    };
    for limit in [0, last_byte] {
        let mut space = vec![0; limit];
        let error = super::write::write_snapshot_to_writer(
            &mut space.as_mut_slice(),
            &document.snapshot(),
            format,
        )
        .unwrap_err();
        assert_eq!(
            error.kind(),
            io::ErrorKind::WriteZero,
            "{format:?} limit={limit}"
        );
    }
}
