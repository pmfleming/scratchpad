use super::*;

impl BufferState {
    pub fn refresh_text_metadata(&mut self) {
        let metadata = buffer_text_metadata_from_piece_tree(
            self.document.piece_tree(),
            &mut self.state.format,
        );
        self.apply_text_metadata(metadata);
    }

    pub fn refresh_text_metadata_after_operation(
        &mut self,
        operation: Option<&TextDocumentOperationRecord>,
    ) {
        if self.refresh.text_metadata_refresh_stale {
            self.refresh_text_metadata();
            return;
        }

        if operation.is_some_and(|operation| self.can_skip_metadata_rescan(operation)) {
            return;
        }

        if let Some(metadata) = operation
            .and_then(|operation| self.incremental_text_metadata_after_operation(operation))
        {
            self.apply_text_metadata(metadata);
            return;
        }

        self.refresh_text_metadata();
    }

    pub fn recheck_encoding_compliance(&mut self) {
        if !self.refresh.encoding_compliance_stale {
            return;
        }
        let tree = self.document.piece_tree();
        self.has_non_compliant_characters = self.format.has_non_compliant_characters_spans(
            tree.spans_for_range(0..tree.len_chars()).map(|s| s.text),
        );
        self.refresh.encoding_compliance_stale = false;
    }

    pub fn encoding_compliance_refresh_needed(&self) -> bool {
        self.refresh.encoding_compliance_stale
    }

    pub fn text_metadata_refresh_needed(&self) -> bool {
        self.refresh.text_metadata_refresh_stale
    }

    pub fn apply_encoding_compliance_refresh(&mut self, revision: u64, has_non_compliant: bool) {
        if self.document_revision() != revision {
            return;
        }
        self.has_non_compliant_characters = has_non_compliant;
        self.refresh.encoding_compliance_stale = false;
    }

    pub fn apply_text_metadata_refresh(
        &mut self,
        revision: u64,
        line_count: usize,
        artifact_summary: TextArtifactSummary,
        format: TextFormatMetadata,
    ) {
        if self.document_revision() != revision {
            return;
        }

        let preferred_line_ending = format.preferred_line_ending_style();
        self.format = format;
        self.apply_text_metadata_fields(line_count, artifact_summary, preferred_line_ending);
    }

    fn apply_text_metadata(&mut self, metadata: BufferTextMetadata) {
        self.apply_text_metadata_fields(
            metadata.line_count,
            metadata.artifact_summary,
            metadata.preferred_line_ending,
        );
    }

    fn apply_text_metadata_fields(
        &mut self,
        line_count: usize,
        artifact_summary: TextArtifactSummary,
        preferred_line_ending: LineEndingStyle,
    ) {
        self.line_count = line_count;
        self.artifact_summary = artifact_summary;
        self.document
            .set_preferred_line_ending(preferred_line_ending);
        if self.show_control_chars && !self.has_visible_control_substitutions() {
            self.show_control_chars = false;
        }
        self.refresh.text_metadata_refresh_stale = false;
        self.refresh.encoding_compliance_stale = true;
    }

    pub(super) fn sync_document_preferred_line_ending(&mut self) {
        self.document
            .set_preferred_line_ending(self.format.preferred_line_ending_style());
    }

    fn can_skip_metadata_rescan(&self, operation: &TextDocumentOperationRecord) -> bool {
        self.format.is_ascii_subset
            && !self.artifact_summary.has_control_chars()
            && operation.edits.iter().all(|edit| {
                metadata_neutral_ascii_text(&edit.deleted_text)
                    && metadata_neutral_ascii_text(&edit.inserted_text)
            })
    }

    fn incremental_text_metadata_after_operation(
        &mut self,
        operation: &TextDocumentOperationRecord,
    ) -> Option<BufferTextMetadata> {
        if operation.edits.len() != 1 {
            return None;
        }

        let edit = operation.edits.first()?;
        let tree = self.document.piece_tree();
        let start_char = edit.start_char.min(tree.len_chars());
        let inserted_char_len = utf8_char_count(&edit.inserted_text);
        let previous_char = start_char
            .checked_sub(1)
            .and_then(|index| tree.char_at(index));
        let next_char = tree.char_at(start_char.saturating_add(inserted_char_len));

        buffer_text_metadata_from_edit(
            self.state.line_count,
            &self.state.artifact_summary,
            &mut self.state.format,
            IncrementalMetadataEdit {
                previous_char,
                deleted_text: &edit.deleted_text,
                inserted_text: &edit.inserted_text,
                next_char,
            },
        )
    }
}

fn metadata_neutral_ascii_text(text: &str) -> bool {
    text.bytes()
        .all(|byte| byte.is_ascii() && !matches!(byte, b'\n' | b'\r' | 0x00..=0x1F))
}

fn utf8_char_count(text: &str) -> usize {
    let bytes = text.as_bytes();
    if bytes.is_ascii() {
        return bytes.len();
    }
    bytes
        .iter()
        .filter(|byte| (*byte & 0b1100_0000) != 0b1000_0000)
        .count()
}
