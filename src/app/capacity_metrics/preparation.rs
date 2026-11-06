//! Wall-clock preparation work; intervals include storage waits, not CPU time.
use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

#[derive(Clone, Copy)]
pub(crate) enum PreparationPhase {
    Bind,
    Normalize,
    Mutate,
    Inspect,
    Total,
}
static TIMES: [AtomicU64; 5] = [const { AtomicU64::new(0) }; 5];
static COMPLETED: AtomicU64 = AtomicU64::new(0);

pub(crate) fn record_preparation(phase: PreparationPhase, elapsed: Duration) {
    TIMES[phase as usize].fetch_add(
        elapsed.as_nanos().min(u128::from(u64::MAX)) as u64,
        Ordering::Relaxed,
    );
    if matches!(phase, PreparationPhase::Total) {
        COMPLETED.fetch_add(1, Ordering::Relaxed);
    }
}

#[derive(Clone, Copy)]
pub(crate) struct PreparationMetrics {
    times: [u64; 5],
    completed: u64,
}
pub(crate) fn preparation_metrics() -> PreparationMetrics {
    PreparationMetrics {
        times: std::array::from_fn(|index| TIMES[index].load(Ordering::Relaxed)),
        completed: COMPLETED.load(Ordering::Relaxed),
    }
}
impl PreparationMetrics {
    pub(crate) fn since(self, before: Self) -> serde_json::Value {
        let delta = |phase: PreparationPhase| {
            self.times[phase as usize].saturating_sub(before.times[phase as usize])
        };
        serde_json::json!({
            "prepare_completed":self.completed.saturating_sub(before.completed),
            "prepare_store_binding_ns":delta(PreparationPhase::Bind),
            "prepare_normalization_ns":delta(PreparationPhase::Normalize),
            "prepare_mutation_ns":delta(PreparationPhase::Mutate),
            "prepare_inspection_ns":delta(PreparationPhase::Inspect),
            "prepare_total_ns":delta(PreparationPhase::Total),
            "prepare_timing_scope":"process_preparation_wall_intervals_total_excludes_store_binding_may_overlap_other_work"
        })
    }
}
