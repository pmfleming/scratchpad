use super::{PieceTreeLite, PieceTreeText, byte_index_for_char_offset};

/// A seekable character cursor that keeps the current backing piece loaded.
/// Sequential forward or reverse movement therefore avoids a tree lookup and
/// a UTF-8 prefix scan for every character.
pub struct PieceTreeCharCursor<'a> {
    tree: &'a PieceTreeLite,
    text: Option<PieceTreeText<'a>>,
    piece_start_char: usize,
    piece_char_len: usize,
    byte_offset: usize,
    position: usize,
}

impl<'a> PieceTreeCharCursor<'a> {
    pub(super) fn new(tree: &'a PieceTreeLite, position: usize) -> Self {
        Self {
            tree,
            text: None,
            piece_start_char: 0,
            piece_char_len: 0,
            byte_offset: 0,
            position: position.min(tree.len_chars()),
        }
    }

    #[must_use]
    pub fn position(&self) -> usize {
        self.position
    }

    pub fn seek(&mut self, position: usize) {
        self.position = position.min(self.tree.len_chars());
        self.text = None;
    }

    #[must_use]
    pub fn peek_next(&mut self) -> Option<char> {
        self.ensure_next_piece()?;
        self.text
            .as_deref()?
            .get(self.byte_offset..)?
            .chars()
            .next()
    }

    #[must_use]
    pub fn peek_previous(&mut self) -> Option<char> {
        self.ensure_previous_piece()?;
        self.text
            .as_deref()?
            .get(..self.byte_offset)?
            .chars()
            .next_back()
    }

    pub fn next_char(&mut self) -> Option<char> {
        let ch = self.peek_next()?;
        self.byte_offset += ch.len_utf8();
        self.position += 1;
        Some(ch)
    }

    pub fn previous_char(&mut self) -> Option<char> {
        let ch = self.peek_previous()?;
        self.byte_offset -= ch.len_utf8();
        self.position -= 1;
        Some(ch)
    }

    fn ensure_next_piece(&mut self) -> Option<()> {
        if self.position >= self.tree.len_chars() {
            return None;
        }
        let piece_end = self.piece_start_char + self.piece_char_len;
        if self.text.is_none()
            || self.position < self.piece_start_char
            || self.position >= piece_end
        {
            self.load_piece(self.position, self.position)?;
        }
        Some(())
    }

    fn ensure_previous_piece(&mut self) -> Option<()> {
        if self.position == 0 {
            return None;
        }
        let piece_end = self.piece_start_char + self.piece_char_len;
        if self.text.is_none()
            || self.position <= self.piece_start_char
            || self.position > piece_end
        {
            self.load_piece(self.position - 1, self.position)?;
        }
        Some(())
    }

    fn load_piece(&mut self, probe: usize, cursor_position: usize) -> Option<()> {
        let address = self.tree.find_leaf_for_char_offset(probe);
        let (piece, offset_in_piece) = self.tree.piece_at_char_offset(address, probe)?;
        let piece_start_char = probe.saturating_sub(offset_in_piece);
        let local_char = cursor_position.saturating_sub(piece_start_char);
        let text = self.tree.piece_text(piece);
        let byte_offset = if piece.is_ascii {
            local_char
        } else {
            byte_index_for_char_offset(&text, local_char)
        };
        self.text = Some(text);
        self.piece_start_char = piece_start_char;
        self.piece_char_len = piece.char_len;
        self.byte_offset = byte_offset;
        Some(())
    }
}
