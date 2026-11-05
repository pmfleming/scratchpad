use super::TextDocument;
use super::helpers::insert_edit;

#[test]
fn history_token_invalidates_replay_clear_restore_and_replacement() {
    let mut document = TextDocument::new("base".into());
    let original = document.history_token();
    insert_edit(&mut document, 0, "a");
    let edited = document.history_token();
    assert_ne!(edited, original);
    document.undo_last_operation().unwrap();
    let undone = document.history_token();
    assert_ne!(undone, edited);
    document.redo_last_operation().unwrap();
    assert_ne!(document.history_token(), edited);
    let before = document.history_token();
    let history = document.exported_history();
    document.restore_exported_history(history);
    assert_ne!(document.history_token(), before);
    let before = document.history_token();
    document.clear_operation_history();
    assert_ne!(document.history_token(), before);
    assert_ne!(document.clone().history_token(), document.history_token());
    let before = document.history_token();
    document.replace_text(document.extract_text());
    assert_ne!(document.history_token(), before);
}

#[test]
fn history_offsets_above_u32_round_trip_without_large_text_allocation() {
    use crate::app::domain::buffer::piece_tree::PieceBuffer;
    use crate::app::domain::buffer::{ByteSpan, PieceHistoryEdit};
    let start = u64::from(u32::MAX) + 17;
    let edit = PieceHistoryEdit::Inserted {
        start_char: start,
        span: ByteSpan {
            buffer: PieceBuffer::Add,
            start_byte: 0,
            byte_len: 2,
        },
    };
    let persisted = edit.to_persisted(|_| "λ".into());
    let encoded = serde_json::to_vec(&persisted).unwrap();
    let decoded: crate::app::domain::buffer::PersistedHistoryEdit =
        serde_json::from_slice(&encoded).unwrap();
    assert_eq!(decoded, persisted);
    let restored = decoded.into_piece(|text| {
        assert_eq!(text, "λ");
        ByteSpan {
            buffer: PieceBuffer::Add,
            start_byte: 0,
            byte_len: text.len() as u64,
        }
    });
    assert_eq!(restored.start_char(), start);
    assert_eq!(restored, edit);
}
