use super::{DocumentSnapshot, OperationDirection, TextDocument};

impl TextDocument {
    /// Keep one older boundary per successful persistence interval. This is
    /// deliberately sampled history, not an unbounded second undo log.
    pub(super) fn preserve_undo_boundary(&mut self) {
        if self.history.pending_recovery.is_none() {
            self.history.pending_recovery = self.oldest_recoverable_snapshot();
        }
    }

    fn oldest_recoverable_snapshot(&self) -> Option<(u64, DocumentSnapshot)> {
        let mut candidate = self.clone();
        candidate.history.pending_recovery = None;
        let mut oldest = None;
        let entries = self.history.entries.iter().rev()
            .filter(|entry| !entry.is_undone())
            .map(|entry| (entry.id, entry.global_seq))
            .collect::<Vec<_>>();
        for (id, sequence) in entries {
            if candidate.apply_text_history_entry(id, OperationDirection::Undo).is_err() {
                break;
            }
            oldest = Some(sequence);
        }
        oldest.map(|sequence| (sequence, candidate.snapshot()))
    }

    pub(crate) fn recovery_snapshot(&self) -> Option<(u64, DocumentSnapshot)> {
        self.history.pending_recovery.clone().or_else(|| {
            let bytes: usize = self.history.entries.iter()
                .map(|entry| self.export_history_entry(entry).payload_bytes()).sum();
            (bytes as u64 > self.history.budget.persisted_payload_budget)
                .then(|| self.oldest_recoverable_snapshot()).flatten()
        })
    }

    pub(crate) fn acknowledge_recovery_snapshot(&mut self, sequence: u64) {
        if self.history.pending_recovery.as_ref().is_some_and(|(id, _)| *id == sequence) {
            self.history.pending_recovery = None;
        }
    }
}
