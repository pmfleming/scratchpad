use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use serde::Serialize;

mod background_io;
mod frame;

#[cfg(test)]
mod tests;

pub use background_io::{
    BackgroundIoLane, BackgroundIoLaneMetricsSnapshot, record_background_io_lane,
    record_background_io_queue_depth, record_background_io_saturation,
};
pub use frame::{FramePhase, FramePhaseMetricsSnapshot, record_frame, record_frame_phase};

const FRAME_HISTOGRAM_BUCKETS: usize = 32;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct CapacityMetricsSnapshot {
    pub full_text_flatten_count: u64,
    pub full_text_flatten_bytes: u64,
    pub range_flatten_count: u64,
    pub range_flatten_bytes: u64,
    pub layout_job_count: u64,
    pub layout_input_bytes: u64,
    pub layout_time_ns: u64,
    pub layout_cache_hit_count: u64,
    pub layout_cache_miss_count: u64,
    pub search_request_count: u64,
    pub search_target_count: u64,
    pub search_chunk_count: u64,
    pub search_intra_buffer_max_workers: u64,
    pub search_worker_active_ns: u64,
    pub search_max_queue_depth: u64,
    pub background_io_path_requests: u64,
    pub background_io_path_active_ns: u64,
    pub background_io_path_max_queue_depth: u64,
    pub background_io_session_requests: u64,
    pub background_io_session_active_ns: u64,
    pub background_io_session_max_queue_depth: u64,
    pub background_io_analysis_requests: u64,
    pub background_io_analysis_active_ns: u64,
    pub background_io_analysis_max_queue_depth: u64,
    pub background_io_path_saturation_count: u64,
    pub background_io_session_saturation_count: u64,
    pub background_io_analysis_saturation_count: u64,
    pub frame_count: u64,
    pub frame_time_total_ns: u64,
    pub frame_time_max_ns: u64,
    pub frame_time_bucket_width_ns: u64,
    pub frame_time_bucket_counts: [u64; FRAME_HISTOGRAM_BUCKETS],
    pub frame_prepare_total_ns: u64,
    pub frame_prepare_max_ns: u64,
    pub frame_background_poll_total_ns: u64,
    pub frame_background_poll_max_ns: u64,
    pub frame_paint_total_ns: u64,
    pub frame_paint_max_ns: u64,
    pub frame_chrome_total_ns: u64,
    pub frame_chrome_max_ns: u64,
    pub frame_active_surface_total_ns: u64,
    pub frame_active_surface_max_ns: u64,
    pub frame_gutter_total_ns: u64,
    pub frame_gutter_max_ns: u64,
    pub frame_scroll_total_ns: u64,
    pub frame_scroll_max_ns: u64,
    pub frame_dialogs_total_ns: u64,
    pub frame_dialogs_max_ns: u64,
    pub frame_shortcuts_total_ns: u64,
    pub frame_shortcuts_max_ns: u64,
    pub frame_finish_total_ns: u64,
    pub frame_finish_max_ns: u64,
    pub history_evictions_per_file: u64,
    pub history_evictions_aggregate: u64,
    pub history_evicted_bytes: u64,
}

macro_rules! capacity_counters {
    ($($name:ident),+ $(,)?) => {
        $(static $name: AtomicU64 = AtomicU64::new(0);)+
    };
}

capacity_counters! {
    FULL_TEXT_FLATTEN_COUNT,
    FULL_TEXT_FLATTEN_BYTES,
    RANGE_FLATTEN_COUNT,
    RANGE_FLATTEN_BYTES,
    LAYOUT_JOB_COUNT,
    LAYOUT_INPUT_BYTES,
    LAYOUT_TIME_NS,
    SEARCH_REQUEST_COUNT,
    SEARCH_TARGET_COUNT,
    SEARCH_CHUNK_COUNT,
    SEARCH_INTRA_BUFFER_MAX_WORKERS,
    SEARCH_WORKER_ACTIVE_NS,
    SEARCH_MAX_QUEUE_DEPTH,
    LAYOUT_CACHE_HIT_COUNT,
    LAYOUT_CACHE_MISS_COUNT,
    HISTORY_EVICTIONS_PER_FILE,
    HISTORY_EVICTIONS_AGGREGATE,
    HISTORY_EVICTED_BYTES,
}

pub fn reset_capacity_metrics() {
    reset_counters(&[
        &FULL_TEXT_FLATTEN_COUNT,
        &FULL_TEXT_FLATTEN_BYTES,
        &RANGE_FLATTEN_COUNT,
        &RANGE_FLATTEN_BYTES,
        &LAYOUT_JOB_COUNT,
        &LAYOUT_INPUT_BYTES,
        &LAYOUT_TIME_NS,
        &SEARCH_REQUEST_COUNT,
        &SEARCH_TARGET_COUNT,
        &SEARCH_CHUNK_COUNT,
        &SEARCH_INTRA_BUFFER_MAX_WORKERS,
        &SEARCH_WORKER_ACTIVE_NS,
        &SEARCH_MAX_QUEUE_DEPTH,
        &LAYOUT_CACHE_HIT_COUNT,
        &LAYOUT_CACHE_MISS_COUNT,
        &HISTORY_EVICTIONS_PER_FILE,
        &HISTORY_EVICTIONS_AGGREGATE,
        &HISTORY_EVICTED_BYTES,
    ]);
    background_io::reset();
    frame::reset();
}

#[must_use]
pub fn capacity_metrics_snapshot() -> CapacityMetricsSnapshot {
    let background_io = background_io::snapshot();
    let frame = frame::snapshot();

    CapacityMetricsSnapshot {
        full_text_flatten_count: load_counter(&FULL_TEXT_FLATTEN_COUNT),
        full_text_flatten_bytes: load_counter(&FULL_TEXT_FLATTEN_BYTES),
        range_flatten_count: load_counter(&RANGE_FLATTEN_COUNT),
        range_flatten_bytes: load_counter(&RANGE_FLATTEN_BYTES),
        layout_job_count: load_counter(&LAYOUT_JOB_COUNT),
        layout_input_bytes: load_counter(&LAYOUT_INPUT_BYTES),
        layout_time_ns: load_counter(&LAYOUT_TIME_NS),
        search_request_count: load_counter(&SEARCH_REQUEST_COUNT),
        search_target_count: load_counter(&SEARCH_TARGET_COUNT),
        search_chunk_count: load_counter(&SEARCH_CHUNK_COUNT),
        search_intra_buffer_max_workers: load_counter(&SEARCH_INTRA_BUFFER_MAX_WORKERS),
        search_worker_active_ns: load_counter(&SEARCH_WORKER_ACTIVE_NS),
        search_max_queue_depth: load_counter(&SEARCH_MAX_QUEUE_DEPTH),
        background_io_path_requests: background_io.path.requests,
        background_io_path_active_ns: background_io.path.active_ns,
        background_io_path_max_queue_depth: background_io.path.max_queue_depth,
        background_io_session_requests: background_io.session.requests,
        background_io_session_active_ns: background_io.session.active_ns,
        background_io_session_max_queue_depth: background_io.session.max_queue_depth,
        background_io_analysis_requests: background_io.analysis.requests,
        background_io_analysis_active_ns: background_io.analysis.active_ns,
        background_io_analysis_max_queue_depth: background_io.analysis.max_queue_depth,
        background_io_path_saturation_count: background_io.path.saturation_count,
        background_io_session_saturation_count: background_io.session.saturation_count,
        background_io_analysis_saturation_count: background_io.analysis.saturation_count,
        layout_cache_hit_count: load_counter(&LAYOUT_CACHE_HIT_COUNT),
        layout_cache_miss_count: load_counter(&LAYOUT_CACHE_MISS_COUNT),
        frame_count: frame.count,
        frame_time_total_ns: frame.total_ns,
        frame_time_max_ns: frame.max_ns,
        frame_time_bucket_width_ns: frame.bucket_width_ns,
        frame_time_bucket_counts: frame.bucket_counts,
        frame_prepare_total_ns: frame.prepare.total_ns,
        frame_prepare_max_ns: frame.prepare.max_ns,
        frame_background_poll_total_ns: frame.background_poll.total_ns,
        frame_background_poll_max_ns: frame.background_poll.max_ns,
        frame_paint_total_ns: frame.paint.total_ns,
        frame_paint_max_ns: frame.paint.max_ns,
        frame_chrome_total_ns: frame.chrome.total_ns,
        frame_chrome_max_ns: frame.chrome.max_ns,
        frame_active_surface_total_ns: frame.active_surface.total_ns,
        frame_active_surface_max_ns: frame.active_surface.max_ns,
        frame_gutter_total_ns: frame.gutter.total_ns,
        frame_gutter_max_ns: frame.gutter.max_ns,
        frame_scroll_total_ns: frame.scroll.total_ns,
        frame_scroll_max_ns: frame.scroll.max_ns,
        frame_dialogs_total_ns: frame.dialogs.total_ns,
        frame_dialogs_max_ns: frame.dialogs.max_ns,
        frame_shortcuts_total_ns: frame.shortcuts.total_ns,
        frame_shortcuts_max_ns: frame.shortcuts.max_ns,
        frame_finish_total_ns: frame.finish.total_ns,
        frame_finish_max_ns: frame.finish.max_ns,
        history_evictions_per_file: load_counter(&HISTORY_EVICTIONS_PER_FILE),
        history_evictions_aggregate: load_counter(&HISTORY_EVICTIONS_AGGREGATE),
        history_evicted_bytes: load_counter(&HISTORY_EVICTED_BYTES),
    }
}

pub fn record_history_eviction_per_file(bytes: usize) {
    HISTORY_EVICTIONS_PER_FILE.fetch_add(1, Ordering::Relaxed);
    HISTORY_EVICTED_BYTES.fetch_add(saturating_u64(bytes), Ordering::Relaxed);
}

pub fn record_history_eviction_aggregate(bytes: usize) {
    HISTORY_EVICTIONS_AGGREGATE.fetch_add(1, Ordering::Relaxed);
    HISTORY_EVICTED_BYTES.fetch_add(saturating_u64(bytes), Ordering::Relaxed);
}

pub fn record_full_text_flatten(bytes: usize) {
    FULL_TEXT_FLATTEN_COUNT.fetch_add(1, Ordering::Relaxed);
    FULL_TEXT_FLATTEN_BYTES.fetch_add(saturating_u64(bytes), Ordering::Relaxed);
}

pub fn record_range_flatten(bytes: usize) {
    RANGE_FLATTEN_COUNT.fetch_add(1, Ordering::Relaxed);
    RANGE_FLATTEN_BYTES.fetch_add(saturating_u64(bytes), Ordering::Relaxed);
}

pub fn record_layout_job(input_bytes: usize, elapsed: Duration) {
    LAYOUT_JOB_COUNT.fetch_add(1, Ordering::Relaxed);
    LAYOUT_INPUT_BYTES.fetch_add(saturating_u64(input_bytes), Ordering::Relaxed);
    LAYOUT_TIME_NS.fetch_add(saturating_u64(elapsed.as_nanos()), Ordering::Relaxed);
}

pub fn record_search_request(target_count: usize, coalesced_queue_depth: usize) {
    SEARCH_REQUEST_COUNT.fetch_add(1, Ordering::Relaxed);
    SEARCH_TARGET_COUNT.fetch_add(saturating_u64(target_count), Ordering::Relaxed);
    update_max(
        &SEARCH_MAX_QUEUE_DEPTH,
        saturating_u64(coalesced_queue_depth),
    );
}

pub fn record_search_chunks(chunk_count: usize) {
    SEARCH_CHUNK_COUNT.fetch_add(saturating_u64(chunk_count), Ordering::Relaxed);
}

pub fn record_search_intra_buffer_workers(workers: usize) {
    update_max(&SEARCH_INTRA_BUFFER_MAX_WORKERS, saturating_u64(workers));
}

pub fn record_search_worker_active(elapsed: Duration) {
    SEARCH_WORKER_ACTIVE_NS.fetch_add(saturating_u64(elapsed.as_nanos()), Ordering::Relaxed);
}

pub fn record_layout_cache_hit() {
    LAYOUT_CACHE_HIT_COUNT.fetch_add(1, Ordering::Relaxed);
}

pub fn record_layout_cache_miss() {
    LAYOUT_CACHE_MISS_COUNT.fetch_add(1, Ordering::Relaxed);
}

fn reset_counters(counters: &[&AtomicU64]) {
    for counter in counters {
        reset_counter(counter);
    }
}

fn reset_counter(counter: &AtomicU64) {
    counter.store(0, Ordering::Relaxed);
}

fn load_counter(counter: &AtomicU64) -> u64 {
    counter.load(Ordering::Relaxed)
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

fn divide_u64(total: u64, count: u64) -> f64 {
    if count == 0 {
        0.0
    } else {
        (total as f64) / (count as f64)
    }
}

fn saturating_u64(value: impl TryInto<u64>) -> u64 {
    value.try_into().unwrap_or(u64::MAX)
}
