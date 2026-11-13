//! Phase 8: central memory budget accountant.
//!
//! Caches that hold revision-scoped or viewport-adjacent reusable state report
//! their byte usage by category here. The values are global counters surfaced
//! through capacity reports and consulted by the cache eviction policies as a
//! soft pressure signal.
//!
//! The budget is intentionally a lightweight, contention-free accountant: no
//! locks, no scheduling. Caches stay responsible for their own LRU eviction.
//! When a reading at a given category exceeds its soft cap, callers should
//! evict more aggressively at the next safe point (e.g. between frames).

use std::sync::atomic::{AtomicU64, Ordering};

use super::capacity_metrics::update_max;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BudgetCategory {
    /// Cached `egui::Galley`s and bounded viewport layout records.
    Layout,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct MemoryBudgetSnapshot {
    pub layout_bytes: u64,
    pub layout_peak_bytes: u64,
}

/// Soft cap per category; readings above the cap should trigger eviction at
/// the next safe boundary. Layout caches are bounded to a few megabytes per
/// view because layouts rebuild quickly.
pub const LAYOUT_BUDGET_BYTES: u64 = 32 * 1024 * 1024;

static LAYOUT_BYTES: AtomicU64 = AtomicU64::new(0);
static LAYOUT_PEAK: AtomicU64 = AtomicU64::new(0);

fn counters(category: BudgetCategory) -> (&'static AtomicU64, &'static AtomicU64, u64) {
    match category {
        BudgetCategory::Layout => (&LAYOUT_BYTES, &LAYOUT_PEAK, LAYOUT_BUDGET_BYTES),
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
    // `fetch_update` only fails when the closure returns `None`, which it never does.
    let _ = current.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |existing| {
        Some(existing.saturating_sub(bytes))
    });
}

#[must_use]
pub fn over_budget(category: BudgetCategory) -> bool {
    let (current, _peak, cap) = counters(category);
    current.load(Ordering::Relaxed) > cap
}

pub fn snapshot() -> MemoryBudgetSnapshot {
    MemoryBudgetSnapshot {
        layout_bytes: LAYOUT_BYTES.load(Ordering::Relaxed),
        layout_peak_bytes: LAYOUT_PEAK.load(Ordering::Relaxed),
    }
}

pub fn reset() {
    LAYOUT_BYTES.store(0, Ordering::Relaxed);
    LAYOUT_PEAK.store(0, Ordering::Relaxed);
}
