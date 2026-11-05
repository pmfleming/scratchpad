use super::super::piece_tree::PieceBuffer;

#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum PieceSource {
    #[default]
    Load,
    Edit,
    Paste,
    Cut,
    SearchReplace,
    External,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ByteSpan {
    pub buffer: PieceBuffer,
    pub start_byte: u64,
    pub byte_len: u64,
    pub char_len: u64,
}

impl ByteSpan {
    pub fn byte_end(self) -> u64 {
        self.start_byte.saturating_add(self.byte_len)
    }
}

pub(crate) fn empty_byte_span() -> ByteSpan {
    ByteSpan {
        buffer: PieceBuffer::Add,
        start_byte: 0,
        byte_len: 0,
        char_len: 0,
    }
}
