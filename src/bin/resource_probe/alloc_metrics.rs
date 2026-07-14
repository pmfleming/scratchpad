//! Allocation telemetry for the resource-probe binary.
//!
//! This is the only intentionally unsafe allocation hook in Scratchpad. It is
//! isolated to the profiling binary so the application and library continue to
//! forbid unsafe code, while capacity probes can measure live and peak heap use.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicU64, Ordering};

static ALLOCATED_BYTES: AtomicU64 = AtomicU64::new(0);
static DEALLOCATED_BYTES: AtomicU64 = AtomicU64::new(0);
static LIVE_BYTES: AtomicU64 = AtomicU64::new(0);
static PEAK_LIVE_BYTES: AtomicU64 = AtomicU64::new(0);
static ALLOCATION_COUNT: AtomicU64 = AtomicU64::new(0);
static DEALLOCATION_COUNT: AtomicU64 = AtomicU64::new(0);
static REALLOCATION_COUNT: AtomicU64 = AtomicU64::new(0);

pub(super) struct TrackingAllocator;

#[global_allocator]
static GLOBAL_ALLOCATOR: TrackingAllocator = TrackingAllocator;

unsafe impl GlobalAlloc for TrackingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: This allocator delegates the actual allocation to the system
        // allocator with the same `layout` it received from the runtime.
        record_allocated_ptr(unsafe { System.alloc(layout) }, layout)
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: This allocator delegates the actual zeroed allocation to the
        // system allocator with the same `layout` it received from the runtime.
        record_allocated_ptr(unsafe { System.alloc_zeroed(layout) }, layout)
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if !ptr.is_null() {
            record_deallocation(layout.size() as u64);
        }
        // SAFETY: `ptr` and `layout` are exactly the pair provided by the
        // runtime for this global allocator's deallocation call.
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: This allocator forwards the runtime-provided pointer/layout
        // pair and requested size to the system allocator, then records only
        // the observed size delta when reallocation succeeds.
        let new_ptr = unsafe { System.realloc(ptr, layout, new_size) };
        if !new_ptr.is_null() {
            REALLOCATION_COUNT.fetch_add(1, Ordering::Relaxed);
            let old_size = layout.size() as u64;
            let new_size = new_size as u64;
            if new_size >= old_size {
                let delta = new_size - old_size;
                if delta > 0 {
                    record_allocation(delta);
                }
            } else {
                let delta = old_size - new_size;
                if delta > 0 {
                    record_deallocation(delta);
                }
            }
        }
        new_ptr
    }
}

fn record_allocated_ptr(ptr: *mut u8, layout: Layout) -> *mut u8 {
    if !ptr.is_null() {
        record_allocation(layout.size() as u64);
    }
    ptr
}

#[derive(Clone, Copy)]
pub(super) struct AllocationSnapshot {
    pub(super) allocated_bytes: u64,
    pub(super) deallocated_bytes: u64,
    pub(super) live_bytes: u64,
    pub(super) peak_live_bytes: u64,
    pub(super) allocation_count: u64,
    pub(super) deallocation_count: u64,
    pub(super) reallocation_count: u64,
}

pub(super) fn reset_allocation_counters() {
    ALLOCATED_BYTES.store(0, Ordering::Relaxed);
    DEALLOCATED_BYTES.store(0, Ordering::Relaxed);
    LIVE_BYTES.store(0, Ordering::Relaxed);
    PEAK_LIVE_BYTES.store(0, Ordering::Relaxed);
    ALLOCATION_COUNT.store(0, Ordering::Relaxed);
    DEALLOCATION_COUNT.store(0, Ordering::Relaxed);
    REALLOCATION_COUNT.store(0, Ordering::Relaxed);
}

pub(super) fn allocation_snapshot() -> AllocationSnapshot {
    AllocationSnapshot {
        allocated_bytes: ALLOCATED_BYTES.load(Ordering::Relaxed),
        deallocated_bytes: DEALLOCATED_BYTES.load(Ordering::Relaxed),
        live_bytes: LIVE_BYTES.load(Ordering::Relaxed),
        peak_live_bytes: PEAK_LIVE_BYTES.load(Ordering::Relaxed),
        allocation_count: ALLOCATION_COUNT.load(Ordering::Relaxed),
        deallocation_count: DEALLOCATION_COUNT.load(Ordering::Relaxed),
        reallocation_count: REALLOCATION_COUNT.load(Ordering::Relaxed),
    }
}

fn record_allocation(bytes: u64) {
    ALLOCATED_BYTES.fetch_add(bytes, Ordering::Relaxed);
    ALLOCATION_COUNT.fetch_add(1, Ordering::Relaxed);
    let live = add_live_bytes(bytes);
    update_peak_live(live);
}

fn record_deallocation(bytes: u64) {
    DEALLOCATED_BYTES.fetch_add(bytes, Ordering::Relaxed);
    DEALLOCATION_COUNT.fetch_add(1, Ordering::Relaxed);
    subtract_live_bytes(bytes);
}

fn add_live_bytes(bytes: u64) -> u64 {
    let mut current = LIVE_BYTES.load(Ordering::Relaxed);
    loop {
        let next = current.saturating_add(bytes);
        match LIVE_BYTES.compare_exchange(current, next, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return next,
            Err(observed) => current = observed,
        }
    }
}

fn subtract_live_bytes(bytes: u64) {
    let mut current = LIVE_BYTES.load(Ordering::Relaxed);
    loop {
        let next = current.saturating_sub(bytes);
        match LIVE_BYTES.compare_exchange(current, next, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return,
            Err(observed) => current = observed,
        }
    }
}

fn update_peak_live(candidate: u64) {
    let mut current = PEAK_LIVE_BYTES.load(Ordering::Relaxed);
    while candidate > current {
        match PEAK_LIVE_BYTES.compare_exchange(
            current,
            candidate,
            Ordering::Relaxed,
            Ordering::Relaxed,
        ) {
            Ok(_) => break,
            Err(observed) => current = observed,
        }
    }
}
