//! Failure-only RAM admission; committed cache and ordinary owned originals are excluded.
use serde::{Deserialize, Serialize};
const MIB: u64 = 1024 * 1024;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FailureRamBudget {
    pub bytes: u64,
    pub automatic: bool,
}
impl Default for FailureRamBudget {
    fn default() -> Self {
        Self::for_total(total_memory())
    }
}
impl FailureRamBudget {
    fn for_total(total: u64) -> Self {
        Self {
            bytes: (total / 4).max(256 * MIB).min(total),
            automatic: true,
        }
    }
    /// Refresh automatic defaults and clamp overrides when moving between machines.
    #[must_use]
    pub fn at_startup(self) -> Self {
        let total = total_memory();
        if self.automatic {
            Self::for_total(total)
        } else {
            Self {
                bytes: self.bytes.clamp(64 * MIB, total.max(64 * MIB)),
                automatic: false,
            }
        }
    }
}
#[must_use]
pub fn total_memory() -> u64 {
    let mut system = sysinfo::System::new();
    system.refresh_memory();
    let bytes = system.total_memory();
    if bytes == 0 { 2 * 1024 * MIB } else { bytes }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn automatic_uses_total_memory_and_overrides_are_clamped() {
        assert_eq!(FailureRamBudget::for_total(2 * 1024 * MIB).bytes, 512 * MIB);
        assert_eq!(FailureRamBudget::for_total(512 * MIB).bytes, 256 * MIB);
        assert!(
            FailureRamBudget {
                bytes: u64::MAX,
                automatic: false
            }
            .at_startup()
            .bytes
                <= total_memory()
        );
        assert_eq!(
            FailureRamBudget {
                bytes: 0,
                automatic: false
            }
            .at_startup()
            .bytes,
            64 * MIB
        );
    }
}

#[cfg(test)]
mod admission_tests {
    use crate::app::services::chunk_store::{ChunkStoreHandle, Health};
    use std::{io, sync::Arc};
    #[test]
    fn failure_limit_refuses_bulk_without_losing_accepted_text_and_reaps_only_abandoned_chunks() {
        let store = ChunkStoreHandle::unavailable("full disk".into());
        store.set_failure_limit(12);
        store.set_other_ram_bytes(3);
        let accepted = store.seal(Arc::new("accepted".into()), 1);
        assert_eq!(store.ram_only_bytes(), 11);
        assert_eq!(
            store.check_bulk_capacity(2).unwrap_err().kind(),
            io::ErrorKind::OutOfMemory
        );
        let typing = store.seal(Arc::new("typing is always accepted".into()), 1);
        assert!(store.ram_only_bytes() > 12);
        assert_eq!(&*accepted.load(false, true).unwrap(), "accepted");
        drop(typing);
        store.reap_abandoned();
        assert_eq!(store.ram_only_bytes(), 11);
        store.set_health(Health::Healthy);
        assert!(store.check_bulk_capacity(1024 * 1024).is_ok());
        assert_eq!(&*accepted.load(false, true).unwrap(), "accepted");
    }
}
