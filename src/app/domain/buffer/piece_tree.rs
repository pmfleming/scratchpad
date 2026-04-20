use std::ops::Range;

const MAX_LEAF_BYTES: usize = 256 * 1024;
const MAX_LEAF_PIECES: usize = 16;
const MAX_LEAVES_PER_INTERNAL: usize = 16;
const MIN_LEAVES_PER_INTERNAL: usize = MAX_LEAVES_PER_INTERNAL / 4;
const PREVIEW_MAX_CHARS: usize = 96;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PieceTreeCharPosition {
    pub offset_chars: usize,
    pub line_index: usize,
    pub column_index: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PieceTreeLineInfo {
    pub line_index: usize,
    pub start_char: usize,
    pub char_len: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PieceTreeSpan<'a> {
    pub text: &'a str,
    pub char_start: usize,
    pub char_len: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PieceTreeMetrics {
    pub bytes: usize,
    pub chars: usize,
    pub newlines: usize,
    pub pieces: usize,
}

impl PieceTreeMetrics {
    fn add_assign(&mut self, other: Self) {
        self.bytes += other.bytes;
        self.chars += other.chars;
        self.newlines += other.newlines;
        self.pieces += other.pieces;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PieceBuffer {
    Original,
    Add,
}

#[derive(Clone, Debug)]
struct Piece {
    buffer: PieceBuffer,
    start_byte: usize,
    byte_len: usize,
    char_len: usize,
    newline_count: usize,
}

impl Piece {
    fn from_slice(buffer: PieceBuffer, start_byte: usize, text: &str) -> Self {
        Self {
            buffer,
            start_byte,
            byte_len: text.len(),
            char_len: text.chars().count(),
            newline_count: count_newlines(text),
        }
    }

    fn metrics(&self) -> PieceTreeMetrics {
        PieceTreeMetrics {
            bytes: self.byte_len,
            chars: self.char_len,
            newlines: self.newline_count,
            pieces: 1,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct PieceTreeLeaf {
    pieces: Vec<Piece>,
    metrics: PieceTreeMetrics,
    piece_start_chars: Vec<usize>,
    piece_start_newlines: Vec<usize>,
}

impl PieceTreeLeaf {
    fn push_piece(&mut self, piece: Piece) {
        if piece.byte_len == 0 {
            return;
        }

        if let Some(last) = self.pieces.last_mut()
            && last.buffer == piece.buffer
            && last.start_byte + last.byte_len == piece.start_byte
        {
            last.byte_len += piece.byte_len;
            last.char_len += piece.char_len;
            last.newline_count += piece.newline_count;
        } else {
            self.pieces.push(piece);
        }

        self.recalculate();
    }

    fn recalculate(&mut self) {
        self.metrics = PieceTreeMetrics::default();
        self.piece_start_chars.clear();
        self.piece_start_newlines.clear();
        let mut current_chars = 0usize;
        let mut current_newlines = 0usize;
        for piece in &self.pieces {
            self.piece_start_chars.push(current_chars);
            self.piece_start_newlines.push(current_newlines);
            self.metrics.add_assign(piece.metrics());
            current_chars += piece.char_len;
            current_newlines += piece.newline_count;
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct PieceTreeInternalNode {
    leaves: Vec<PieceTreeLeaf>,
    metrics: PieceTreeMetrics,
    leaf_start_chars: Vec<usize>,
    leaf_start_newlines: Vec<usize>,
}

impl PieceTreeInternalNode {
    fn recalculate(&mut self) {
        if self.leaves.is_empty() {
            self.leaves.push(PieceTreeLeaf::default());
        }

        self.metrics = PieceTreeMetrics::default();
        self.leaf_start_chars.clear();
        self.leaf_start_newlines.clear();

        let mut current_chars = 0usize;
        let mut current_newlines = 0usize;
        for leaf in &mut self.leaves {
            leaf.recalculate();
            self.leaf_start_chars.push(current_chars);
            self.leaf_start_newlines.push(current_newlines);
            self.metrics.add_assign(leaf.metrics);
            current_chars += leaf.metrics.chars;
            current_newlines += leaf.metrics.newlines;
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct PieceTreeRoot {
    nodes: Vec<PieceTreeInternalNode>,
    metrics: PieceTreeMetrics,
    node_start_chars: Vec<usize>,
    node_start_newlines: Vec<usize>,
}

impl PieceTreeRoot {
    fn recalculate(&mut self) {
        if self.nodes.is_empty() {
            self.nodes.push(PieceTreeInternalNode::default());
        }

        self.metrics = PieceTreeMetrics::default();
        self.node_start_chars.clear();
        self.node_start_newlines.clear();

        let mut current_chars = 0usize;
        let mut current_newlines = 0usize;
        for node in &mut self.nodes {
            node.recalculate();
            self.node_start_chars.push(current_chars);
            self.node_start_newlines.push(current_newlines);
            self.metrics.add_assign(node.metrics);
            current_chars += node.metrics.chars;
            current_newlines += node.metrics.newlines;
        }
    }
}

#[derive(Clone, Debug)]
pub struct PieceTreeLite {
    original: String,
    add: String,
    root: PieceTreeRoot,
}

#[derive(Clone, Copy, Debug, Default)]
struct LeafAddress {
    node_index: usize,
    leaf_index: usize,
    leaf_start_char: usize,
    leaf_start_newline: usize,
}

pub struct PieceTreeSlice<'a> {
    tree: &'a PieceTreeLite,
    range_chars: Range<usize>,
    node_index: usize,
    leaf_index: usize,
    piece_index: usize,
    current_char: usize,
    finished: bool,
}

impl PieceTreeLite {
    pub fn from_string(text: String) -> Self {
        let pieces = build_chunked_pieces(PieceBuffer::Original, 0, &text);
        Self {
            original: text,
            add: String::new(),
            root: build_root_from_pieces(pieces),
        }
    }

    pub fn metrics(&self) -> PieceTreeMetrics {
        self.root.metrics
    }

    pub fn len_bytes(&self) -> usize {
        self.root.metrics.bytes
    }

    pub fn len_chars(&self) -> usize {
        self.root.metrics.chars
    }

    pub fn normalize_char_range(&self, range_chars: Range<usize>) -> Range<usize> {
        let start = range_chars.start.min(self.len_chars());
        let end = range_chars.end.min(self.len_chars());
        if start <= end { start..end } else { end..start }
    }

    pub fn char_position(&self, offset_chars: usize) -> PieceTreeCharPosition {
        let safe_offset = offset_chars.min(self.len_chars());
        let line_index = self.line_index_at_offset(safe_offset);
        let line_info = self.line_info(line_index);
        PieceTreeCharPosition {
            offset_chars: safe_offset,
            line_index,
            column_index: safe_offset.saturating_sub(line_info.start_char),
        }
    }

    pub fn line_info(&self, target_line: usize) -> PieceTreeLineInfo {
        let (start_char, char_len) = self.line_lookup(target_line);
        PieceTreeLineInfo {
            line_index: target_line.min(self.root.metrics.newlines),
            start_char,
            char_len,
        }
    }

    pub fn insert(&mut self, offset_chars: usize, text: &str) {
        assert!(offset_chars <= self.len_chars());
        if text.is_empty() {
            return;
        }

        let add_start = self.add.len();
        self.add.push_str(text);
        let inserted_pieces = build_chunked_pieces(PieceBuffer::Add, add_start, text);
        if self.len_chars() == 0 {
            self.root = build_root_from_pieces(inserted_pieces);
            return;
        }

        let address = self.find_leaf_for_char_offset(offset_chars);
        let replacement = {
            let leaf = &self.root.nodes[address.node_index].leaves[address.leaf_index];
            self.leaf_with_inserted_pieces(
                leaf,
                offset_chars.saturating_sub(address.leaf_start_char),
                inserted_pieces,
            )
        };
        let replacement_leaves = pack_pieces_into_leaves(replacement);
        self.replace_leaf_span(address, address, replacement_leaves);
    }

    pub fn remove_range(&mut self, range_chars: Range<usize>) {
        self.remove_char_range(range_chars);
    }

    pub fn remove_char_range(&mut self, range_chars: Range<usize>) {
        assert!(range_chars.start <= range_chars.end);
        assert!(range_chars.end <= self.len_chars());
        if range_chars.is_empty() {
            return;
        }

        let start_address = self.find_leaf_for_char_offset(range_chars.start);
        let end_probe = range_chars.end.saturating_sub(1);
        let end_address = self.find_leaf_for_char_offset(end_probe);
        let affected_pieces =
            self.retained_pieces_for_removal(start_address, end_address, range_chars);
        let replacement_leaves = pack_pieces_into_leaves(affected_pieces);
        self.replace_leaf_span(start_address, end_address, replacement_leaves);
    }

    pub fn preview_for_match(&self, range_chars: &Range<usize>) -> (usize, usize, String) {
        let normalized = self.normalize_char_range(range_chars.clone());
        let line_index = self.line_index_at_offset(normalized.start);
        let info = self.line_info(line_index);
        let column = normalized.start.saturating_sub(info.start_char);
        let (line_text, truncated) = self.extract_range_bounded(
            info.start_char..info.start_char + info.char_len,
            PREVIEW_MAX_CHARS,
        );
        let mut preview = compact_preview(&line_text);
        if truncated && !preview.ends_with("...") {
            preview.push_str("...");
        }
        (line_index + 1, column + 1, preview)
    }

    pub fn line_lookup(&self, target_line: usize) -> (usize, usize) {
        if self.len_chars() == 0 {
            return (0, 0);
        }

        let safe_line = target_line.min(self.root.metrics.newlines);
        let address = self.find_leaf_for_line(safe_line);
        let mut current_line = address.leaf_start_newline;
        let mut line_start = address.leaf_start_char;
        let mut current_char = line_start;
        let mut current_len = 0usize;
        let mut is_first_leaf = true;

        for (node_index, node) in self.root.nodes.iter().enumerate().skip(address.node_index) {
            let leaf_start = if node_index == address.node_index {
                address.leaf_index
            } else {
                0
            };

            for leaf in node.leaves.iter().skip(leaf_start) {
                let piece_skip = if is_first_leaf {
                    is_first_leaf = false;
                    let offset_in_leaf = safe_line.saturating_sub(current_line);
                    if offset_in_leaf > 0 && !leaf.piece_start_newlines.is_empty() {
                        let pi = leaf
                            .piece_start_newlines
                            .partition_point(|&n| n < offset_in_leaf)
                            .saturating_sub(1);
                        current_line += leaf.piece_start_newlines[pi];
                        current_char += leaf.piece_start_chars[pi];
                        pi
                    } else {
                        0
                    }
                } else {
                    0
                };

                for piece in leaf.pieces.iter().skip(piece_skip) {
                    if current_line < safe_line && current_line + piece.newline_count < safe_line {
                        current_line += piece.newline_count;
                        current_char += piece.char_len;
                        continue;
                    }
                    if current_line == safe_line && piece.newline_count == 0 {
                        current_len += piece.char_len;
                        current_char += piece.char_len;
                        continue;
                    }

                    for ch in self.piece_text(piece).chars() {
                        if current_line == safe_line {
                            if ch == '\n' {
                                return (line_start, current_len);
                            }
                            current_len += 1;
                        } else if ch == '\n' {
                            current_line += 1;
                            line_start = current_char + 1;
                            current_len = 0;
                        }
                        current_char += 1;
                    }
                }
            }
        }

        (line_start, current_len)
    }

    pub fn line_index_at_offset(&self, offset_chars: usize) -> usize {
        if self.len_chars() == 0 {
            return 0;
        }

        let safe_offset = offset_chars.min(self.len_chars());
        let address = self.find_leaf_for_char_offset(safe_offset);
        let mut current_line = address.leaf_start_newline;
        let mut current_char = address.leaf_start_char;

        let leaf = &self.root.nodes[address.node_index].leaves[address.leaf_index];
        let piece_skip = if !leaf.piece_start_chars.is_empty() {
            let offset_in_leaf = safe_offset - address.leaf_start_char;
            let pi = leaf
                .piece_start_chars
                .partition_point(|&c| c <= offset_in_leaf)
                .saturating_sub(1);
            current_line += leaf.piece_start_newlines[pi];
            current_char += leaf.piece_start_chars[pi];
            pi
        } else {
            0
        };

        for piece in leaf.pieces.iter().skip(piece_skip) {
            for ch in self.piece_text(piece).chars() {
                if current_char >= safe_offset {
                    return current_line;
                }
                if ch == '\n' {
                    current_line += 1;
                }
                current_char += 1;
            }
        }

        current_line
    }

    pub fn extract_range(&self, range_chars: Range<usize>) -> String {
        let mut result = String::new();
        for span in self.spans_for_range(range_chars) {
            result.push_str(span.text);
        }
        result
    }

    pub fn extract_range_bounded(
        &self,
        range_chars: Range<usize>,
        max_chars: usize,
    ) -> (String, bool) {
        if max_chars == 0 {
            return (
                String::new(),
                !self.normalize_char_range(range_chars).is_empty(),
            );
        }

        let mut remaining = max_chars;
        let mut result = String::new();
        let mut truncated = false;

        for span in self.spans_for_range(range_chars) {
            if remaining == 0 {
                truncated = true;
                break;
            }

            if span.char_len <= remaining {
                result.push_str(span.text);
                remaining -= span.char_len;
                continue;
            }

            let byte_end = byte_index_for_char_offset(span.text, remaining);
            result.push_str(&span.text[..byte_end]);
            truncated = true;
            break;
        }

        (result, truncated)
    }

    pub fn collect_line_bounded(&self, target_line: usize, max_chars: usize) -> (String, bool) {
        let line_info = self.line_info(target_line);
        self.extract_range_bounded(
            line_info.start_char..line_info.start_char + line_info.char_len,
            max_chars,
        )
    }

    pub fn spans_for_line(&self, target_line: usize) -> PieceTreeSlice<'_> {
        let line_info = self.line_info(target_line);
        self.spans_for_range(line_info.start_char..line_info.start_char + line_info.char_len)
    }

    pub fn spans_for_range(&self, range_chars: Range<usize>) -> PieceTreeSlice<'_> {
        let normalized = self.normalize_char_range(range_chars);
        if normalized.is_empty() || self.len_chars() == 0 {
            return PieceTreeSlice::empty(self, normalized);
        }

        let address = self.find_leaf_for_char_offset(normalized.start);
        PieceTreeSlice {
            tree: self,
            range_chars: normalized,
            node_index: address.node_index,
            leaf_index: address.leaf_index,
            piece_index: 0,
            current_char: address.leaf_start_char,
            finished: false,
        }
    }

    fn leaf_with_inserted_pieces(
        &self,
        leaf: &PieceTreeLeaf,
        offset_in_leaf_chars: usize,
        inserted_pieces: Vec<Piece>,
    ) -> Vec<Piece> {
        let mut result = Vec::with_capacity(leaf.pieces.len() + inserted_pieces.len() + 2);
        let mut current_char = 0usize;
        let mut inserted = Some(inserted_pieces);

        for piece in &leaf.pieces {
            let piece_end_char = current_char + piece.char_len;
            if let Some(new_pieces) = inserted.take() {
                if offset_in_leaf_chars <= current_char {
                    result.extend(new_pieces);
                } else if offset_in_leaf_chars < piece_end_char {
                    let inner_offset = offset_in_leaf_chars - current_char;
                    if inner_offset > 0 {
                        result.push(self.slice_piece_by_chars(piece, 0, inner_offset));
                    }
                    result.extend(new_pieces);
                    if inner_offset < piece.char_len {
                        result.push(self.slice_piece_by_chars(
                            piece,
                            inner_offset,
                            piece.char_len - inner_offset,
                        ));
                    }
                    current_char = piece_end_char;
                    continue;
                } else {
                    inserted = Some(new_pieces);
                }
            }

            result.push(piece.clone());
            current_char = piece_end_char;
        }

        if let Some(new_pieces) = inserted {
            result.extend(new_pieces);
        }

        result
    }

    fn slice_piece_by_chars(&self, piece: &Piece, start_char: usize, char_len: usize) -> Piece {
        let text = self.piece_text(piece);
        let byte_range = byte_range_for_char_range(text, start_char, start_char + char_len);
        Piece::from_slice(
            piece.buffer,
            piece.start_byte + byte_range.start,
            &text[byte_range],
        )
    }

    fn piece_text<'a>(&'a self, piece: &Piece) -> &'a str {
        let end = piece.start_byte + piece.byte_len;
        match piece.buffer {
            PieceBuffer::Original => &self.original[piece.start_byte..end],
            PieceBuffer::Add => &self.add[piece.start_byte..end],
        }
    }

    fn find_leaf_for_char_offset(&self, offset_chars: usize) -> LeafAddress {
        if self.root.nodes.is_empty() || self.len_chars() == 0 {
            return LeafAddress::default();
        }
        self.find_leaf_by(offset_chars, &self.root.node_start_chars, |node| {
            &node.leaf_start_chars
        })
    }

    fn find_leaf_for_line(&self, target_line: usize) -> LeafAddress {
        if self.root.nodes.is_empty() {
            return LeafAddress::default();
        }
        self.find_leaf_by(target_line, &self.root.node_start_newlines, |node| {
            &node.leaf_start_newlines
        })
    }

    fn find_leaf_by(
        &self,
        target: usize,
        node_starts: &[usize],
        leaf_starts: impl Fn(&PieceTreeInternalNode) -> &[usize],
    ) -> LeafAddress {
        let node_index = node_starts
            .partition_point(|start| *start <= target)
            .saturating_sub(1)
            .min(self.root.nodes.len() - 1);
        let node = &self.root.nodes[node_index];
        let offset_in_node = target.saturating_sub(node_starts[node_index]);
        let leaf_starts_slice = leaf_starts(node);
        let leaf_index = leaf_starts_slice
            .partition_point(|start| *start <= offset_in_node)
            .saturating_sub(1)
            .min(node.leaves.len() - 1);

        LeafAddress {
            node_index,
            leaf_index,
            leaf_start_char: self.root.node_start_chars[node_index]
                + node.leaf_start_chars[leaf_index],
            leaf_start_newline: self.root.node_start_newlines[node_index]
                + node.leaf_start_newlines[leaf_index],
        }
    }

    fn retained_pieces_for_removal(
        &self,
        start_address: LeafAddress,
        end_address: LeafAddress,
        range_chars: Range<usize>,
    ) -> Vec<Piece> {
        let mut affected_pieces = Vec::new();
        let mut current_char = start_address.leaf_start_char;

        for node_index in start_address.node_index..=end_address.node_index {
            let node = &self.root.nodes[node_index];
            let leaf_start = if node_index == start_address.node_index {
                start_address.leaf_index
            } else {
                0
            };
            let leaf_end = if node_index == end_address.node_index {
                end_address.leaf_index
            } else {
                node.leaves.len() - 1
            };

            for leaf in &node.leaves[leaf_start..=leaf_end] {
                for piece in &leaf.pieces {
                    let piece_start_char = current_char;
                    let piece_end_char = current_char + piece.char_len;

                    if range_chars.end <= piece_start_char || range_chars.start >= piece_end_char {
                        affected_pieces.push(piece.clone());
                    } else {
                        let left_chars = range_chars.start.saturating_sub(piece_start_char);
                        if left_chars > 0 {
                            affected_pieces.push(self.slice_piece_by_chars(piece, 0, left_chars));
                        }

                        let right_start_char = range_chars
                            .end
                            .saturating_sub(piece_start_char)
                            .min(piece.char_len);
                        if right_start_char < piece.char_len {
                            affected_pieces.push(self.slice_piece_by_chars(
                                piece,
                                right_start_char,
                                piece.char_len - right_start_char,
                            ));
                        }
                    }

                    current_char = piece_end_char;
                }
            }
        }

        affected_pieces
    }

    fn replace_leaf_span(
        &mut self,
        start: LeafAddress,
        end: LeafAddress,
        replacement_leaves: Vec<PieceTreeLeaf>,
    ) {
        let mut combined_leaves = Vec::new();
        combined_leaves.extend(
            self.root.nodes[start.node_index].leaves[..start.leaf_index]
                .iter()
                .cloned(),
        );
        combined_leaves.extend(replacement_leaves);
        combined_leaves.extend(
            self.root.nodes[end.node_index].leaves[end.leaf_index + 1..]
                .iter()
                .cloned(),
        );

        let replacement_nodes = pack_leaves_into_nodes(combined_leaves);
        let inserted_nodes = replacement_nodes.len();
        self.root
            .nodes
            .splice(start.node_index..=end.node_index, replacement_nodes);
        self.rebalance_node_window(start.node_index, inserted_nodes);
    }

    fn rebalance_node_window(&mut self, inserted_at: usize, inserted_nodes: usize) {
        if self.root.nodes.is_empty() {
            self.root.recalculate();
            return;
        }

        let mut window_start = inserted_at.saturating_sub(1);
        let mut window_end = (inserted_at + inserted_nodes + 1).min(self.root.nodes.len());

        if window_start > 0 && self.root.nodes[window_start].leaves.len() < MIN_LEAVES_PER_INTERNAL
        {
            window_start -= 1;
        }
        if window_end < self.root.nodes.len()
            && self.root.nodes[window_end - 1].leaves.len() < MIN_LEAVES_PER_INTERNAL
        {
            window_end = (window_end + 1).min(self.root.nodes.len());
        }

        let mut window_leaves = Vec::new();
        for node in &self.root.nodes[window_start..window_end] {
            window_leaves.extend(node.leaves.iter().cloned());
        }

        let rebalanced_nodes = pack_leaves_into_nodes(window_leaves);
        self.root
            .nodes
            .splice(window_start..window_end, rebalanced_nodes);
        self.root.recalculate();
    }
}

impl<'a> PieceTreeSlice<'a> {
    fn empty(tree: &'a PieceTreeLite, range_chars: Range<usize>) -> Self {
        let current_char = range_chars.start;
        Self {
            tree,
            range_chars,
            node_index: 0,
            leaf_index: 0,
            piece_index: 0,
            current_char,
            finished: true,
        }
    }

    fn advance_piece_cursor(&mut self) {
        if self.finished || self.node_index >= self.tree.root.nodes.len() {
            self.finished = true;
            return;
        }

        let node = &self.tree.root.nodes[self.node_index];
        if self.leaf_index >= node.leaves.len() {
            self.node_index += 1;
            self.leaf_index = 0;
            self.piece_index = 0;
            if self.node_index >= self.tree.root.nodes.len() {
                self.finished = true;
            }
            return;
        }

        let leaf = &node.leaves[self.leaf_index];
        self.piece_index += 1;
        if self.piece_index >= leaf.pieces.len() {
            self.leaf_index += 1;
            self.piece_index = 0;
            if self.leaf_index >= node.leaves.len() {
                self.node_index += 1;
                self.leaf_index = 0;
                if self.node_index >= self.tree.root.nodes.len() {
                    self.finished = true;
                }
            }
        }
    }
}

impl<'a> Iterator for PieceTreeSlice<'a> {
    type Item = PieceTreeSpan<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        while !self.finished {
            let node = self.tree.root.nodes.get(self.node_index)?;
            let leaf = node.leaves.get(self.leaf_index)?;
            let piece = match leaf.pieces.get(self.piece_index) {
                Some(piece) => piece,
                None => {
                    self.advance_piece_cursor();
                    continue;
                }
            };

            let piece_start_char = self.current_char;
            let piece_end_char = piece_start_char + piece.char_len;
            self.current_char = piece_end_char;
            self.advance_piece_cursor();

            if piece_end_char <= self.range_chars.start {
                continue;
            }
            if piece_start_char >= self.range_chars.end {
                self.finished = true;
                return None;
            }

            let local_start = self.range_chars.start.saturating_sub(piece_start_char);
            let local_end = (self.range_chars.end.min(piece_end_char)) - piece_start_char;
            let text = self.tree.piece_text(piece);
            let byte_range = byte_range_for_char_range(text, local_start, local_end);
            return Some(PieceTreeSpan {
                text: &text[byte_range],
                char_start: piece_start_char + local_start,
                char_len: local_end - local_start,
            });
        }

        None
    }
}

fn build_root_from_pieces(pieces: Vec<Piece>) -> PieceTreeRoot {
    build_root_from_leaves(pack_pieces_into_leaves(pieces))
}

fn build_root_from_leaves(mut leaves: Vec<PieceTreeLeaf>) -> PieceTreeRoot {
    if leaves.is_empty() {
        leaves.push(PieceTreeLeaf::default());
    }

    let mut root = PieceTreeRoot {
        nodes: pack_leaves_into_nodes(leaves),
        metrics: PieceTreeMetrics::default(),
        node_start_chars: Vec::new(),
        node_start_newlines: Vec::new(),
    };
    root.recalculate();
    root
}

fn pack_leaves_into_nodes(leaves: Vec<PieceTreeLeaf>) -> Vec<PieceTreeInternalNode> {
    let mut nodes = Vec::new();
    let mut index = 0usize;
    let total = leaves.len();
    while index < total {
        let remaining = total - index;
        let chunk_size = if remaining > MAX_LEAVES_PER_INTERNAL
            && remaining - MAX_LEAVES_PER_INTERNAL < MIN_LEAVES_PER_INTERNAL
        {
            remaining.div_ceil(2)
        } else {
            MAX_LEAVES_PER_INTERNAL.min(remaining)
        };
        let end = index + chunk_size;
        let mut node = PieceTreeInternalNode {
            leaves: leaves[index..end].to_vec(),
            metrics: PieceTreeMetrics::default(),
            leaf_start_chars: Vec::new(),
            leaf_start_newlines: Vec::new(),
        };
        node.recalculate();
        nodes.push(node);
        index = end;
    }
    nodes
}

fn build_chunked_pieces(buffer: PieceBuffer, start_byte: usize, text: &str) -> Vec<Piece> {
    if text.is_empty() {
        return Vec::new();
    }

    let mut pieces = Vec::new();
    let mut offset = 0usize;
    while offset < text.len() {
        let len = next_chunk_len(text, offset, MAX_LEAF_BYTES);
        let slice = &text[offset..offset + len];
        pieces.push(Piece::from_slice(buffer, start_byte + offset, slice));
        offset += len;
    }
    pieces
}

fn pack_pieces_into_leaves(pieces: Vec<Piece>) -> Vec<PieceTreeLeaf> {
    let mut leaves = Vec::new();
    let mut current = PieceTreeLeaf::default();

    for piece in pieces {
        if piece.byte_len == 0 {
            continue;
        }

        let would_exceed_bytes =
            !current.pieces.is_empty() && current.metrics.bytes + piece.byte_len > MAX_LEAF_BYTES;
        let would_exceed_pieces =
            !current.pieces.is_empty() && current.pieces.len() >= MAX_LEAF_PIECES;
        if would_exceed_bytes || would_exceed_pieces {
            current.recalculate();
            leaves.push(current);
            current = PieceTreeLeaf::default();
        }

        current.push_piece(piece);
    }

    if !current.pieces.is_empty() || leaves.is_empty() {
        current.recalculate();
        leaves.push(current);
    }

    leaves
}

fn next_chunk_len(text: &str, offset: usize, max_len: usize) -> usize {
    let candidate_end = (offset + max_len).min(text.len());
    if text.is_char_boundary(candidate_end) {
        return candidate_end - offset;
    }

    let mut end = candidate_end;
    while end > offset && !text.is_char_boundary(end) {
        end -= 1;
    }
    end - offset
}

fn count_newlines(text: &str) -> usize {
    text.bytes().filter(|byte| *byte == b'\n').count()
}

fn byte_range_for_char_range(text: &str, start_char: usize, end_char: usize) -> Range<usize> {
    let start = byte_index_for_char_offset(text, start_char);
    let end = byte_index_for_char_offset(text, end_char);
    start..end
}

fn byte_index_for_char_offset(text: &str, char_offset: usize) -> usize {
    if char_offset == 0 {
        return 0;
    }

    text.char_indices()
        .map(|(index, _)| index)
        .nth(char_offset)
        .unwrap_or(text.len())
}

fn compact_preview(line_text: &str) -> String {
    let trimmed = line_text.trim();
    let trimmed_chars = trimmed.chars().collect::<Vec<_>>();
    if trimmed_chars.len() <= PREVIEW_MAX_CHARS {
        return trimmed.to_owned();
    }

    let mut preview = trimmed_chars[..PREVIEW_MAX_CHARS]
        .iter()
        .collect::<String>();
    preview.push_str("...");
    preview
}

#[cfg(test)]
mod tests {
    use super::{
        MAX_LEAF_BYTES, MAX_LEAF_PIECES, MAX_LEAVES_PER_INTERNAL, MIN_LEAVES_PER_INTERNAL,
        PieceTreeLite,
    };

    #[test]
    fn repeated_inserts_split_into_multiple_balanced_nodes() {
        let mut tree = PieceTreeLite::from_string("abc".repeat(128));
        let mut expected = "abc".repeat(128);

        for _ in 0..320 {
            tree.insert(1, "x");
            insert_string_at_char(&mut expected, 1, "x");
        }

        assert_eq!(tree.extract_range(0..tree.len_chars()), expected);
        assert!(tree.root.nodes.len() > 1);
        assert_balanced(&tree);
    }

    #[test]
    fn repeated_removals_merge_nodes_back_down() {
        let mut tree = PieceTreeLite::from_string("abc".repeat(128));
        let mut expected = "abc".repeat(128);

        for _ in 0..320 {
            tree.insert(1, "x");
            insert_string_at_char(&mut expected, 1, "x");
        }
        let expanded_node_count = tree.root.nodes.len();

        tree.remove_char_range(1..321);
        remove_string_char_range(&mut expected, 1..321);

        assert_eq!(tree.extract_range(0..tree.len_chars()), expected);
        assert!(tree.root.nodes.len() < expanded_node_count);
        assert_balanced(&tree);
    }

    #[test]
    fn pack_avoids_runt_nodes() {
        // 18 leaves should produce 2 nodes of 9 each, not 16 + 2
        let mut tree = PieceTreeLite::from_string(String::new());
        // Build a tree that forces 18+ leaves via many insert sites
        let chunk = "x".repeat(1024);
        for i in 0..300 {
            tree.insert(i * 1024, &chunk);
        }
        for node in &tree.root.nodes {
            assert!(
                node.leaves.len() >= MIN_LEAVES_PER_INTERNAL || tree.root.nodes.len() == 1,
                "runt node with {} leaves (min {})",
                node.leaves.len(),
                MIN_LEAVES_PER_INTERNAL,
            );
        }
        assert_balanced(&tree);
    }

    fn assert_balanced(tree: &PieceTreeLite) {
        let mut computed_bytes = 0usize;
        let mut computed_chars = 0usize;
        let mut computed_newlines = 0usize;
        let mut computed_pieces = 0usize;

        if tree.root.nodes.len() > 1 {
            assert!(!tree.root.nodes.is_empty());
        }

        for node in &tree.root.nodes {
            assert!(!node.leaves.is_empty());
            assert!(node.leaves.len() <= MAX_LEAVES_PER_INTERNAL);

            for leaf in &node.leaves {
                if !leaf.pieces.is_empty() {
                    assert!(leaf.pieces.len() <= MAX_LEAF_PIECES);
                    assert!(leaf.metrics.bytes <= MAX_LEAF_BYTES);
                }

                // Verify piece-level prefix sums
                assert_eq!(leaf.piece_start_chars.len(), leaf.pieces.len());
                assert_eq!(leaf.piece_start_newlines.len(), leaf.pieces.len());
                let mut prefix_chars = 0usize;
                let mut prefix_newlines = 0usize;
                for (i, piece) in leaf.pieces.iter().enumerate() {
                    assert_eq!(leaf.piece_start_chars[i], prefix_chars);
                    assert_eq!(leaf.piece_start_newlines[i], prefix_newlines);
                    prefix_chars += piece.char_len;
                    prefix_newlines += piece.newline_count;
                }

                computed_bytes += leaf.metrics.bytes;
                computed_chars += leaf.metrics.chars;
                computed_newlines += leaf.metrics.newlines;
                computed_pieces += leaf.metrics.pieces;
            }
        }

        assert_eq!(tree.metrics().bytes, computed_bytes);
        assert_eq!(tree.metrics().chars, computed_chars);
        assert_eq!(tree.metrics().newlines, computed_newlines);
        assert_eq!(tree.metrics().pieces, computed_pieces);
    }

    fn insert_string_at_char(text: &mut String, char_offset: usize, inserted: &str) {
        let byte_offset = char_to_byte_offset(text, char_offset);
        text.insert_str(byte_offset, inserted);
    }

    fn remove_string_char_range(text: &mut String, range: std::ops::Range<usize>) {
        let start = char_to_byte_offset(text, range.start);
        let end = char_to_byte_offset(text, range.end);
        text.replace_range(start..end, "");
    }

    fn char_to_byte_offset(text: &str, char_offset: usize) -> usize {
        if char_offset == 0 {
            return 0;
        }

        text.char_indices()
            .map(|(index, _)| index)
            .nth(char_offset)
            .unwrap_or(text.len())
    }
}
