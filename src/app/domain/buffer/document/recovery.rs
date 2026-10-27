use super::{DocumentSnapshot, OperationDirection, TextDocument};
use std::sync::Mutex;

#[derive(PartialEq, Eq)]
struct RecoveryKey {
    oldest: Option<u64>,
    revision: u64,
    content_generation: u64,
    budget: u64,
}

struct CachedRecovery {
    key: RecoveryKey,
    point: Option<(u64, DocumentSnapshot)>,
}

#[derive(Default)]
pub(super) struct RecoveryCache(Mutex<Option<CachedRecovery>>);

impl Clone for RecoveryCache {
    fn clone(&self) -> Self {
        // A document clone may diverge independently. Never share acknowledgements.
        Self::default()
    }
}

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
        let entries = self
            .history
            .entries
            .iter()
            .rev()
            .filter(|entry| !entry.is_undone())
            .map(|entry| (entry.id, entry.global_seq))
            .collect::<Vec<_>>();
        for (id, sequence) in entries {
            if candidate
                .apply_text_history_entry(id, OperationDirection::Undo)
                .is_err()
            {
                break;
            }
            oldest = Some(sequence);
        }
        oldest.map(|sequence| (sequence, candidate.snapshot()))
    }

    pub(crate) fn recovery_snapshot(&self) -> Option<(u64, DocumentSnapshot)> {
        if let Some(point) = &self.history.pending_recovery {
            return Some(point.clone());
        }
        let key = RecoveryKey {
            oldest: self.history.entries.first().map(|entry| entry.id),
            revision: self.history.revision_counter,
            content_generation: self.content.piece_tree.generation(),
            budget: self.history.budget.persisted_payload_budget,
        };
        {
            let cache = self
                .history
                .recovery_cache
                .0
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            if let Some(cached) = cache.as_ref().filter(|cached| cached.key == key) {
                return cached.point.clone();
            }
        }
        let bytes: usize = self
            .history
            .entries
            .iter()
            .map(|entry| self.export_history_entry(entry).payload_bytes())
            .sum();
        let point = (bytes as u64 > key.budget)
            .then(|| self.oldest_recoverable_snapshot())
            .flatten();
        *self
            .history
            .recovery_cache
            .0
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = Some(CachedRecovery {
            key,
            point: point.clone(),
        });
        point
    }

    pub(crate) fn acknowledge_recovery_snapshot(&mut self, sequence: u64) {
        if self
            .history
            .pending_recovery
            .as_ref()
            .is_some_and(|(id, _)| *id == sequence)
        {
            self.history.pending_recovery = None;
        }
        let cache = self
            .history
            .recovery_cache
            .0
            .get_mut()
            .unwrap_or_else(|e| e.into_inner());
        if let Some(cached) = cache
            && cached
                .point
                .as_ref()
                .is_some_and(|(token, _)| *token == sequence)
        {
            // Keep the key, including negative results, so an idle capture does
            // not reconstruct and resubmit the acknowledged boundary.
            cached.point = None;
        }
    }
}
