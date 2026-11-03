use super::{DocumentSnapshot, TextDocument};

impl TextDocument {
    pub(crate) fn recovery_snapshot(&self) -> Option<(u64, DocumentSnapshot)> {
        None
    }
    pub(crate) fn acknowledge_recovery_snapshot(&mut self, _sequence: u64) {}
}
