//! Phase 8: central memory budget accountant.
//!
//! Caches that hold revision-scoped or viewport-adjacent reusable state
//! (layout galleys, display metadata, search/index summaries, snapshot
//! buffers, etc.) report their byte usage by category here. The values are
//! global counters surfaced through capacity reports and consulted by the
//! cache eviction policies as a soft pressure signal.
//!
//! The budget is intentionally a lightweight, contention-free accountant: no
//! locks, no scheduling. Caches stay responsible for their own LRU eviction.
//! When a reading at a given category exceeds its soft cap, callers should
//! evict more aggressively at the next safe point (e.g. between frames).

use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BudgetCategory {
    /// Cached `egui::Galley`s and bounded viewport layout records.
    Layout,
    /// Display-row metadata held outside the live snapshot.
    DisplayMetadata,
    /// Worker-prepared snapshots awaiting install on the UI thread.
    PendingSnapshots,
    /// Search/index summaries (chunk descriptors, deferred analysis caches).
    SearchIndex,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct MemoryBudgetSnapshot {
    pub layout_bytes: u64,
    pub display_metadata_bytes: u64,
    pub pending_snapshots_bytes: u64,
    pub search_index_bytes: u64,
    pub layout_peak_bytes: u64,
    pub display_metadata_peak_bytes: u64,
    pub pending_snapshots_peak_bytes: u64,
    pub search_index_peak_bytes: u64,
}

impl MemoryBudgetSnapshot {
    pub fn total_bytes(&self) -> u64 {
        self.layout_bytes
            .saturating_add(self.display_metadata_bytes)
            .saturating_add(self.pending_snapshots_bytes)
            .saturating_add(self.search_index_bytes)
    }
}

/// Soft cap per category; readings above the cap should trigger eviction at
/// the next safe boundary. The values reflect the plan's targets for
/// "modern PCs but still predictable on smaller machines": layout caches are
/// bounded to a few megabytes per view because layouts rebuild quickly.
pub const LAYOUT_BUDGET_BYTES: u64 = 32 * 1024 * 1024;
pub const DISPLAY_METADATA_BUDGET_BYTES: u64 = 16 * 1024 * 1024;
pub const PENDING_SNAPSHOTS_BUDGET_BYTES: u64 = 64 * 1024 * 1024;
pub const SEARCH_INDEX_BUDGET_BYTES: u64 = 32 * 1024 * 1024;

static LAYOUT_BYTES: AtomicU64 = AtomicU64::new(0);
static DISPLAY_METADATA_BYTES: AtomicU64 = AtomicU64::new(0);
static PENDING_SNAPSHOTS_BYTES: AtomicU64 = AtomicU64::new(0);
static SEARCH_INDEX_BYTES: AtomicU64 = AtomicU64::new(0);

static LAYOUT_PEAK: AtomicU64 = AtomicU64::new(0);
static DISPLAY_METADATA_PEAK: AtomicU64 = AtomicU64::new(0);
static PENDING_SNAPSHOTS_PEAK: AtomicU64 = AtomicU64::new(0);
static SEARCH_INDEX_PEAK: AtomicU64 = AtomicU64::new(0);

fn counters(category: BudgetCategory) -> (&'static AtomicU64, &'static AtomicU64, u64) {
    match category {
        BudgetCategory::Layout => (&LAYOUT_BYTES, &LAYOUT_PEAK, LAYOUT_BUDGET_BYTES),
        BudgetCategory::DisplayMetadata => (
            &DISPLAY_METADATA_BYTES,
            &DISPLAY_METADATA_PEAK,
            DISPLAY_METADATA_BUDGET_BYTES,
        ),
        BudgetCategory::PendingSnapshots => (
            &PENDING_SNAPSHOTS_BYTES,
            &PENDING_SNAPSHOTS_PEAK,
            PENDING_SNAPSHOTS_BUDGET_BYTES,
        ),
        BudgetCategory::SearchIndex => (
            &SEARCH_INDEX_BYTES,
            &SEARCH_INDEX_PEAK,
            SEARCH_INDEX_BUDGET_BYTES,
        ),
    }
}

pub fn record_alloc(category: BudgetCategory, bytes: usize) {
    let bytes = bytes as u64;
    let (current, peak, _cap) = counters(category);
    let new = current.fetch_add(bytes, Ordering::Relaxed) + bytes;
    update_max(peak, new);
}

pub fn record_free(category: BudgetCategory, bytes: usize) {
    let (current, _peak, _cap) = counters(category);
    let bytes = bytes as u64;
    let mut existing = current.load(Ordering::Relaxed);
    loop {
        let next = existing.saturating_sub(bytes);
        match current.compare_exchange_weak(existing, next, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return,
            Err(observed) => existing = observed,
        }
    }
}

pub fn over_budget(category: BudgetCategory) -> bool {
    let (current, _peak, cap) = counters(category);
    current.load(Ordering::Relaxed) > cap
}

pub fn current_bytes(category: BudgetCategory) -> u64 {
    counters(category).0.load(Ordering::Relaxed)
}

pub fn snapshot() -> MemoryBudgetSnapshot {
    MemoryBudgetSnapshot {
        layout_bytes: LAYOUT_BYTES.load(Ordering::Relaxed),
        display_metadata_bytes: DISPLAY_METADATA_BYTES.load(Ordering::Relaxed),
        pending_snapshots_bytes: PENDING_SNAPSHOTS_BYTES.load(Ordering::Relaxed),
        search_index_bytes: SEARCH_INDEX_BYTES.load(Ordering::Relaxed),
        layout_peak_bytes: LAYOUT_PEAK.load(Ordering::Relaxed),
        display_metadata_peak_bytes: DISPLAY_METADATA_PEAK.load(Ordering::Relaxed),
        pending_snapshots_peak_bytes: PENDING_SNAPSHOTS_PEAK.load(Ordering::Relaxed),
        search_index_peak_bytes: SEARCH_INDEX_PEAK.load(Ordering::Relaxed),
    }
}

pub fn reset() {
    LAYOUT_BYTES.store(0, Ordering::Relaxed);
    DISPLAY_METADATA_BYTES.store(0, Ordering::Relaxed);
    PENDING_SNAPSHOTS_BYTES.store(0, Ordering::Relaxed);
    SEARCH_INDEX_BYTES.store(0, Ordering::Relaxed);
    LAYOUT_PEAK.store(0, Ordering::Relaxed);
    DISPLAY_METADATA_PEAK.store(0, Ordering::Relaxed);
    PENDING_SNAPSHOTS_PEAK.store(0, Ordering::Relaxed);
    SEARCH_INDEX_PEAK.store(0, Ordering::Relaxed);
}

fn update_max(counter: &AtomicU64, value: u64) {
    let mut current = counter.load(Ordering::Relaxed);
    while value > current {
        match counter.compare_exchange_weak(current, value, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => break,
            Err(next) => current = next,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BudgetCategory, current_bytes, over_budget, record_alloc, record_free, reset, snapshot,
    };

    #[test]
    fn alloc_and_free_round_trip_against_global_counters() {
        reset();
        record_alloc(BudgetCategory::Layout, 1024);
        record_alloc(BudgetCategory::Layout, 512);
        assert_eq!(current_bytes(BudgetCategory::Layout), 1536);
        let s = snapshot();
        assert_eq!(s.layout_bytes, 1536);
        assert_eq!(s.layout_peak_bytes, 1536);

        record_free(BudgetCategory::Layout, 1024);
        assert_eq!(current_bytes(BudgetCategory::Layout), 512);
        let s = snapshot();
        assert_eq!(s.layout_peak_bytes, 1536, "peak should be sticky");

        // free below zero should saturate
        record_free(BudgetCategory::Layout, 9999);
        assert_eq!(current_bytes(BudgetCategory::Layout), 0);

        reset();
        assert_eq!(current_bytes(BudgetCategory::Layout), 0);
        assert!(!over_budget(BudgetCategory::Layout));
    }
}
