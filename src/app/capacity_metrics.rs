use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use serde::Serialize;

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
}

static FULL_TEXT_FLATTEN_COUNT: AtomicU64 = AtomicU64::new(0);
static FULL_TEXT_FLATTEN_BYTES: AtomicU64 = AtomicU64::new(0);
static RANGE_FLATTEN_COUNT: AtomicU64 = AtomicU64::new(0);
static RANGE_FLATTEN_BYTES: AtomicU64 = AtomicU64::new(0);
static LAYOUT_JOB_COUNT: AtomicU64 = AtomicU64::new(0);
static LAYOUT_INPUT_BYTES: AtomicU64 = AtomicU64::new(0);
static LAYOUT_TIME_NS: AtomicU64 = AtomicU64::new(0);
static SEARCH_REQUEST_COUNT: AtomicU64 = AtomicU64::new(0);
static SEARCH_TARGET_COUNT: AtomicU64 = AtomicU64::new(0);
static SEARCH_CHUNK_COUNT: AtomicU64 = AtomicU64::new(0);
static SEARCH_INTRA_BUFFER_MAX_WORKERS: AtomicU64 = AtomicU64::new(0);
static SEARCH_WORKER_ACTIVE_NS: AtomicU64 = AtomicU64::new(0);
static SEARCH_MAX_QUEUE_DEPTH: AtomicU64 = AtomicU64::new(0);
static BACKGROUND_IO_PATH_REQUESTS: AtomicU64 = AtomicU64::new(0);
static BACKGROUND_IO_PATH_ACTIVE_NS: AtomicU64 = AtomicU64::new(0);
static BACKGROUND_IO_PATH_MAX_QUEUE_DEPTH: AtomicU64 = AtomicU64::new(0);
static BACKGROUND_IO_SESSION_REQUESTS: AtomicU64 = AtomicU64::new(0);
static BACKGROUND_IO_SESSION_ACTIVE_NS: AtomicU64 = AtomicU64::new(0);
static BACKGROUND_IO_SESSION_MAX_QUEUE_DEPTH: AtomicU64 = AtomicU64::new(0);
static BACKGROUND_IO_ANALYSIS_REQUESTS: AtomicU64 = AtomicU64::new(0);
static BACKGROUND_IO_ANALYSIS_ACTIVE_NS: AtomicU64 = AtomicU64::new(0);
static BACKGROUND_IO_ANALYSIS_MAX_QUEUE_DEPTH: AtomicU64 = AtomicU64::new(0);
static BACKGROUND_IO_PATH_SATURATION_COUNT: AtomicU64 = AtomicU64::new(0);
static BACKGROUND_IO_SESSION_SATURATION_COUNT: AtomicU64 = AtomicU64::new(0);
static BACKGROUND_IO_ANALYSIS_SATURATION_COUNT: AtomicU64 = AtomicU64::new(0);
static LAYOUT_CACHE_HIT_COUNT: AtomicU64 = AtomicU64::new(0);
static LAYOUT_CACHE_MISS_COUNT: AtomicU64 = AtomicU64::new(0);
static FRAME_COUNT: AtomicU64 = AtomicU64::new(0);
static FRAME_TIME_TOTAL_NS: AtomicU64 = AtomicU64::new(0);
static FRAME_TIME_MAX_NS: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BackgroundIoLane {
    Path,
    Session,
    Analysis,
}

pub fn reset_capacity_metrics() {
    FULL_TEXT_FLATTEN_COUNT.store(0, Ordering::Relaxed);
    FULL_TEXT_FLATTEN_BYTES.store(0, Ordering::Relaxed);
    RANGE_FLATTEN_COUNT.store(0, Ordering::Relaxed);
    RANGE_FLATTEN_BYTES.store(0, Ordering::Relaxed);
    LAYOUT_JOB_COUNT.store(0, Ordering::Relaxed);
    LAYOUT_INPUT_BYTES.store(0, Ordering::Relaxed);
    LAYOUT_TIME_NS.store(0, Ordering::Relaxed);
    SEARCH_REQUEST_COUNT.store(0, Ordering::Relaxed);
    SEARCH_TARGET_COUNT.store(0, Ordering::Relaxed);
    SEARCH_CHUNK_COUNT.store(0, Ordering::Relaxed);
    SEARCH_INTRA_BUFFER_MAX_WORKERS.store(0, Ordering::Relaxed);
    SEARCH_WORKER_ACTIVE_NS.store(0, Ordering::Relaxed);
    SEARCH_MAX_QUEUE_DEPTH.store(0, Ordering::Relaxed);
    BACKGROUND_IO_PATH_REQUESTS.store(0, Ordering::Relaxed);
    BACKGROUND_IO_PATH_ACTIVE_NS.store(0, Ordering::Relaxed);
    BACKGROUND_IO_PATH_MAX_QUEUE_DEPTH.store(0, Ordering::Relaxed);
    BACKGROUND_IO_SESSION_REQUESTS.store(0, Ordering::Relaxed);
    BACKGROUND_IO_SESSION_ACTIVE_NS.store(0, Ordering::Relaxed);
    BACKGROUND_IO_SESSION_MAX_QUEUE_DEPTH.store(0, Ordering::Relaxed);
    BACKGROUND_IO_ANALYSIS_REQUESTS.store(0, Ordering::Relaxed);
    BACKGROUND_IO_ANALYSIS_ACTIVE_NS.store(0, Ordering::Relaxed);
    BACKGROUND_IO_ANALYSIS_MAX_QUEUE_DEPTH.store(0, Ordering::Relaxed);
    BACKGROUND_IO_PATH_SATURATION_COUNT.store(0, Ordering::Relaxed);
    BACKGROUND_IO_SESSION_SATURATION_COUNT.store(0, Ordering::Relaxed);
    BACKGROUND_IO_ANALYSIS_SATURATION_COUNT.store(0, Ordering::Relaxed);
    LAYOUT_CACHE_HIT_COUNT.store(0, Ordering::Relaxed);
    LAYOUT_CACHE_MISS_COUNT.store(0, Ordering::Relaxed);
    FRAME_COUNT.store(0, Ordering::Relaxed);
    FRAME_TIME_TOTAL_NS.store(0, Ordering::Relaxed);
    FRAME_TIME_MAX_NS.store(0, Ordering::Relaxed);
}

pub fn capacity_metrics_snapshot() -> CapacityMetricsSnapshot {
    CapacityMetricsSnapshot {
        full_text_flatten_count: FULL_TEXT_FLATTEN_COUNT.load(Ordering::Relaxed),
        full_text_flatten_bytes: FULL_TEXT_FLATTEN_BYTES.load(Ordering::Relaxed),
        range_flatten_count: RANGE_FLATTEN_COUNT.load(Ordering::Relaxed),
        range_flatten_bytes: RANGE_FLATTEN_BYTES.load(Ordering::Relaxed),
        layout_job_count: LAYOUT_JOB_COUNT.load(Ordering::Relaxed),
        layout_input_bytes: LAYOUT_INPUT_BYTES.load(Ordering::Relaxed),
        layout_time_ns: LAYOUT_TIME_NS.load(Ordering::Relaxed),
        search_request_count: SEARCH_REQUEST_COUNT.load(Ordering::Relaxed),
        search_target_count: SEARCH_TARGET_COUNT.load(Ordering::Relaxed),
        search_chunk_count: SEARCH_CHUNK_COUNT.load(Ordering::Relaxed),
        search_intra_buffer_max_workers: SEARCH_INTRA_BUFFER_MAX_WORKERS.load(Ordering::Relaxed),
        search_worker_active_ns: SEARCH_WORKER_ACTIVE_NS.load(Ordering::Relaxed),
        search_max_queue_depth: SEARCH_MAX_QUEUE_DEPTH.load(Ordering::Relaxed),
        background_io_path_requests: BACKGROUND_IO_PATH_REQUESTS.load(Ordering::Relaxed),
        background_io_path_active_ns: BACKGROUND_IO_PATH_ACTIVE_NS.load(Ordering::Relaxed),
        background_io_path_max_queue_depth: BACKGROUND_IO_PATH_MAX_QUEUE_DEPTH
            .load(Ordering::Relaxed),
        background_io_session_requests: BACKGROUND_IO_SESSION_REQUESTS.load(Ordering::Relaxed),
        background_io_session_active_ns: BACKGROUND_IO_SESSION_ACTIVE_NS.load(Ordering::Relaxed),
        background_io_session_max_queue_depth: BACKGROUND_IO_SESSION_MAX_QUEUE_DEPTH
            .load(Ordering::Relaxed),
        background_io_analysis_requests: BACKGROUND_IO_ANALYSIS_REQUESTS.load(Ordering::Relaxed),
        background_io_analysis_active_ns: BACKGROUND_IO_ANALYSIS_ACTIVE_NS.load(Ordering::Relaxed),
        background_io_analysis_max_queue_depth: BACKGROUND_IO_ANALYSIS_MAX_QUEUE_DEPTH
            .load(Ordering::Relaxed),
        background_io_path_saturation_count: BACKGROUND_IO_PATH_SATURATION_COUNT
            .load(Ordering::Relaxed),
        background_io_session_saturation_count: BACKGROUND_IO_SESSION_SATURATION_COUNT
            .load(Ordering::Relaxed),
        background_io_analysis_saturation_count: BACKGROUND_IO_ANALYSIS_SATURATION_COUNT
            .load(Ordering::Relaxed),
        layout_cache_hit_count: LAYOUT_CACHE_HIT_COUNT.load(Ordering::Relaxed),
        layout_cache_miss_count: LAYOUT_CACHE_MISS_COUNT.load(Ordering::Relaxed),
        frame_count: FRAME_COUNT.load(Ordering::Relaxed),
        frame_time_total_ns: FRAME_TIME_TOTAL_NS.load(Ordering::Relaxed),
        frame_time_max_ns: FRAME_TIME_MAX_NS.load(Ordering::Relaxed),
    }
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

pub fn record_background_io_lane(lane: BackgroundIoLane, elapsed: Duration) {
    let elapsed_ns = saturating_u64(elapsed.as_nanos());
    match lane {
        BackgroundIoLane::Path => {
            BACKGROUND_IO_PATH_REQUESTS.fetch_add(1, Ordering::Relaxed);
            BACKGROUND_IO_PATH_ACTIVE_NS.fetch_add(elapsed_ns, Ordering::Relaxed);
        }
        BackgroundIoLane::Session => {
            BACKGROUND_IO_SESSION_REQUESTS.fetch_add(1, Ordering::Relaxed);
            BACKGROUND_IO_SESSION_ACTIVE_NS.fetch_add(elapsed_ns, Ordering::Relaxed);
        }
        BackgroundIoLane::Analysis => {
            BACKGROUND_IO_ANALYSIS_REQUESTS.fetch_add(1, Ordering::Relaxed);
            BACKGROUND_IO_ANALYSIS_ACTIVE_NS.fetch_add(elapsed_ns, Ordering::Relaxed);
        }
    }
}

pub fn record_background_io_queue_depth(lane: BackgroundIoLane, depth: usize) {
    let value = saturating_u64(depth);
    let counter = match lane {
        BackgroundIoLane::Path => &BACKGROUND_IO_PATH_MAX_QUEUE_DEPTH,
        BackgroundIoLane::Session => &BACKGROUND_IO_SESSION_MAX_QUEUE_DEPTH,
        BackgroundIoLane::Analysis => &BACKGROUND_IO_ANALYSIS_MAX_QUEUE_DEPTH,
    };
    update_max(counter, value);
}

pub fn record_background_io_saturation(lane: BackgroundIoLane) {
    let counter = match lane {
        BackgroundIoLane::Path => &BACKGROUND_IO_PATH_SATURATION_COUNT,
        BackgroundIoLane::Session => &BACKGROUND_IO_SESSION_SATURATION_COUNT,
        BackgroundIoLane::Analysis => &BACKGROUND_IO_ANALYSIS_SATURATION_COUNT,
    };
    counter.fetch_add(1, Ordering::Relaxed);
}

pub fn record_layout_cache_hit() {
    LAYOUT_CACHE_HIT_COUNT.fetch_add(1, Ordering::Relaxed);
}

pub fn record_layout_cache_miss() {
    LAYOUT_CACHE_MISS_COUNT.fetch_add(1, Ordering::Relaxed);
}

pub fn record_frame(elapsed: Duration) {
    let elapsed_ns = saturating_u64(elapsed.as_nanos());
    FRAME_COUNT.fetch_add(1, Ordering::Relaxed);
    FRAME_TIME_TOTAL_NS.fetch_add(elapsed_ns, Ordering::Relaxed);
    update_max(&FRAME_TIME_MAX_NS, elapsed_ns);
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

fn saturating_u64(value: impl TryInto<u64>) -> u64 {
    value.try_into().unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::{
        BackgroundIoLane, capacity_metrics_snapshot, record_background_io_lane,
        record_background_io_queue_depth, record_background_io_saturation, record_frame,
        record_full_text_flatten, record_layout_cache_hit, record_layout_cache_miss,
        record_layout_job, record_range_flatten, record_search_chunks, record_search_request,
        record_search_worker_active, reset_capacity_metrics,
    };
    use std::time::Duration;

    #[test]
    fn capacity_metrics_record_and_reset_phase_zero_counters() {
        reset_capacity_metrics();

        record_full_text_flatten(128);
        record_range_flatten(32);
        record_layout_job(256, Duration::from_nanos(99));
        record_search_request(4, 2);
        record_search_request(2, 5);
        record_search_chunks(7);
        record_search_worker_active(Duration::from_nanos(123));
        record_background_io_lane(BackgroundIoLane::Path, Duration::from_nanos(10));
        record_background_io_lane(BackgroundIoLane::Session, Duration::from_nanos(20));
        record_background_io_lane(BackgroundIoLane::Analysis, Duration::from_nanos(30));
        record_background_io_queue_depth(BackgroundIoLane::Path, 3);
        record_background_io_queue_depth(BackgroundIoLane::Path, 1);
        record_background_io_queue_depth(BackgroundIoLane::Session, 2);
        record_background_io_queue_depth(BackgroundIoLane::Analysis, 6);
        record_background_io_saturation(BackgroundIoLane::Path);
        record_background_io_saturation(BackgroundIoLane::Path);
        record_background_io_saturation(BackgroundIoLane::Analysis);
        record_layout_cache_hit();
        record_layout_cache_hit();
        record_layout_cache_miss();
        record_frame(Duration::from_nanos(50));
        record_frame(Duration::from_nanos(80));

        let snapshot = capacity_metrics_snapshot();
        assert_eq!(snapshot.full_text_flatten_count, 1);
        assert_eq!(snapshot.full_text_flatten_bytes, 128);
        assert_eq!(snapshot.range_flatten_count, 1);
        assert_eq!(snapshot.range_flatten_bytes, 32);
        assert_eq!(snapshot.layout_job_count, 1);
        assert_eq!(snapshot.layout_input_bytes, 256);
        assert_eq!(snapshot.layout_time_ns, 99);
        assert_eq!(snapshot.search_request_count, 2);
        assert_eq!(snapshot.search_target_count, 6);
        assert_eq!(snapshot.search_chunk_count, 7);
        assert_eq!(snapshot.search_worker_active_ns, 123);
        assert_eq!(snapshot.search_max_queue_depth, 5);
        assert_eq!(snapshot.background_io_path_requests, 1);
        assert_eq!(snapshot.background_io_path_active_ns, 10);
        assert_eq!(snapshot.background_io_session_requests, 1);
        assert_eq!(snapshot.background_io_session_active_ns, 20);
        assert_eq!(snapshot.background_io_analysis_requests, 1);
        assert_eq!(snapshot.background_io_analysis_active_ns, 30);
        assert_eq!(snapshot.background_io_path_max_queue_depth, 3);
        assert_eq!(snapshot.background_io_session_max_queue_depth, 2);
        assert_eq!(snapshot.background_io_analysis_max_queue_depth, 6);
        assert_eq!(snapshot.background_io_path_saturation_count, 2);
        assert_eq!(snapshot.background_io_session_saturation_count, 0);
        assert_eq!(snapshot.background_io_analysis_saturation_count, 1);
        assert_eq!(snapshot.layout_cache_hit_count, 2);
        assert_eq!(snapshot.layout_cache_miss_count, 1);
        assert_eq!(snapshot.frame_count, 2);
        assert_eq!(snapshot.frame_time_total_ns, 130);
        assert_eq!(snapshot.frame_time_max_ns, 80);

        reset_capacity_metrics();
        assert_eq!(capacity_metrics_snapshot().layout_job_count, 0);
        assert_eq!(capacity_metrics_snapshot().search_request_count, 0);
    }
}
