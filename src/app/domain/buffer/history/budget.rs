const MIB: u64 = 1024 * 1024;
use std::sync::Mutex;
use std::time::{Duration, Instant};

static DEFAULT_BUDGET: Mutex<Option<(Instant, TextHistoryBudget)>> = Mutex::new(None);
const DEFAULT_REFRESH_INTERVAL: Duration = Duration::from_secs(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TextHistoryBudget {
    pub per_file_byte_budget: u64,
    pub aggregate_byte_budget: u64,
    pub derived_from_memory: bool,
}

impl Default for TextHistoryBudget {
    fn default() -> Self {
        cached_default(
            &DEFAULT_BUDGET,
            Instant::now(),
            Self::derive_from_available_memory,
        )
    }
}

fn cached_default(
    cache: &Mutex<Option<(Instant, TextHistoryBudget)>>,
    now: Instant,
    refresh: impl FnOnce() -> TextHistoryBudget,
) -> TextHistoryBudget {
    let mut cache = cache
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some((sampled, budget)) = *cache
        && now.saturating_duration_since(sampled) < DEFAULT_REFRESH_INTERVAL
    {
        return budget;
    }
    // A burst of new buffers shares one memory sample. Explicit auto-budget
    // resets still call derive_from_available_memory directly and sample anew.
    let budget = refresh();
    *cache = Some((now, budget));
    budget
}

impl TextHistoryBudget {
    #[must_use]
    pub fn derive_from_available_memory() -> Self {
        let available = available_memory_bytes().unwrap_or(2 * 1024 * MIB);
        let aggregate = clamp_u64(available / 50, 16 * MIB, 512 * MIB);
        let per_file = clamp_u64(aggregate / 8, 4 * MIB, 64 * MIB);
        Self {
            per_file_byte_budget: per_file,
            aggregate_byte_budget: aggregate,
            derived_from_memory: true,
        }
    }

    #[must_use]
    pub fn sanitized(mut self) -> Self {
        self.per_file_byte_budget = self.per_file_byte_budget.clamp(MIB, 1024 * MIB);
        self.aggregate_byte_budget = self.aggregate_byte_budget.clamp(4 * MIB, 4096 * MIB);
        self
    }
}

fn clamp_u64(value: u64, min: u64, max: u64) -> u64 {
    value.clamp(min, max)
}

fn available_memory_bytes() -> Option<u64> {
    let mut system = sysinfo::System::new();
    system.refresh_memory();
    let available = system.available_memory();
    (available > 0).then_some(available)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn construction_bursts_share_memory_sampling_but_later_buffers_refresh() {
        let cache = Mutex::new(None);
        let now = Instant::now();
        let first = TextHistoryBudget {
            per_file_byte_budget: 4 * MIB,
            aggregate_byte_budget: 32 * MIB,
            derived_from_memory: true,
        };
        let later = TextHistoryBudget {
            aggregate_byte_budget: 16 * MIB,
            ..first
        };
        assert_eq!(cached_default(&cache, now, || first), first);
        for _ in 0..1000 {
            assert_eq!(
                cached_default(&cache, now + Duration::from_millis(999), || panic!(
                    "resampled within construction burst"
                )),
                first
            );
        }
        assert_eq!(
            cached_default(&cache, now + DEFAULT_REFRESH_INTERVAL, || later),
            later
        );
    }
}
