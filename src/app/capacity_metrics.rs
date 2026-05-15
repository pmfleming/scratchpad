use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use serde::Serialize;

const FRAME_HISTOGRAM_BUCKETS: usize = 32;
const FRAME_HISTOGRAM_BUCKET_WIDTH_NS: u64 = 1_000_000;

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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct BackgroundIoLaneMetricsSnapshot {
    pub requests: u64,
    pub active_ns: u64,
    pub max_queue_depth: u64,
    pub saturation_count: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct FramePhaseMetricsSnapshot {
    pub total_ns: u64,
    pub max_ns: u64,
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
static FRAME_TIME_BUCKET_COUNTS: [AtomicU64; FRAME_HISTOGRAM_BUCKETS] =
    [const { AtomicU64::new(0) }; FRAME_HISTOGRAM_BUCKETS];
static FRAME_PREPARE_TOTAL_NS: AtomicU64 = AtomicU64::new(0);
static FRAME_PREPARE_MAX_NS: AtomicU64 = AtomicU64::new(0);
static FRAME_BACKGROUND_POLL_TOTAL_NS: AtomicU64 = AtomicU64::new(0);
static FRAME_BACKGROUND_POLL_MAX_NS: AtomicU64 = AtomicU64::new(0);
static FRAME_PAINT_TOTAL_NS: AtomicU64 = AtomicU64::new(0);
static FRAME_PAINT_MAX_NS: AtomicU64 = AtomicU64::new(0);
static FRAME_CHROME_TOTAL_NS: AtomicU64 = AtomicU64::new(0);
static FRAME_CHROME_MAX_NS: AtomicU64 = AtomicU64::new(0);
static FRAME_ACTIVE_SURFACE_TOTAL_NS: AtomicU64 = AtomicU64::new(0);
static FRAME_ACTIVE_SURFACE_MAX_NS: AtomicU64 = AtomicU64::new(0);
static FRAME_GUTTER_TOTAL_NS: AtomicU64 = AtomicU64::new(0);
static FRAME_GUTTER_MAX_NS: AtomicU64 = AtomicU64::new(0);
static FRAME_SCROLL_TOTAL_NS: AtomicU64 = AtomicU64::new(0);
static FRAME_SCROLL_MAX_NS: AtomicU64 = AtomicU64::new(0);
static FRAME_DIALOGS_TOTAL_NS: AtomicU64 = AtomicU64::new(0);
static FRAME_DIALOGS_MAX_NS: AtomicU64 = AtomicU64::new(0);
static FRAME_SHORTCUTS_TOTAL_NS: AtomicU64 = AtomicU64::new(0);
static FRAME_SHORTCUTS_MAX_NS: AtomicU64 = AtomicU64::new(0);
static FRAME_FINISH_TOTAL_NS: AtomicU64 = AtomicU64::new(0);
static FRAME_FINISH_MAX_NS: AtomicU64 = AtomicU64::new(0);
static HISTORY_EVICTIONS_PER_FILE: AtomicU64 = AtomicU64::new(0);
static HISTORY_EVICTIONS_AGGREGATE: AtomicU64 = AtomicU64::new(0);
static HISTORY_EVICTED_BYTES: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BackgroundIoLane {
    Path,
    Session,
    Analysis,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FramePhase {
    Prepare,
    BackgroundPoll,
    Paint,
    Chrome,
    ActiveSurface,
    Gutter,
    Scroll,
    Dialogs,
    Shortcuts,
    Finish,
}

impl CapacityMetricsSnapshot {
    #[must_use]
    pub fn background_io_lane(&self, lane: BackgroundIoLane) -> BackgroundIoLaneMetricsSnapshot {
        match lane {
            BackgroundIoLane::Path => BackgroundIoLaneMetricsSnapshot {
                requests: self.background_io_path_requests,
                active_ns: self.background_io_path_active_ns,
                max_queue_depth: self.background_io_path_max_queue_depth,
                saturation_count: self.background_io_path_saturation_count,
            },
            BackgroundIoLane::Session => BackgroundIoLaneMetricsSnapshot {
                requests: self.background_io_session_requests,
                active_ns: self.background_io_session_active_ns,
                max_queue_depth: self.background_io_session_max_queue_depth,
                saturation_count: self.background_io_session_saturation_count,
            },
            BackgroundIoLane::Analysis => BackgroundIoLaneMetricsSnapshot {
                requests: self.background_io_analysis_requests,
                active_ns: self.background_io_analysis_active_ns,
                max_queue_depth: self.background_io_analysis_max_queue_depth,
                saturation_count: self.background_io_analysis_saturation_count,
            },
        }
    }

    #[must_use]
    pub fn frame_phase(&self, phase: FramePhase) -> FramePhaseMetricsSnapshot {
        match phase {
            FramePhase::Prepare => FramePhaseMetricsSnapshot {
                total_ns: self.frame_prepare_total_ns,
                max_ns: self.frame_prepare_max_ns,
            },
            FramePhase::BackgroundPoll => FramePhaseMetricsSnapshot {
                total_ns: self.frame_background_poll_total_ns,
                max_ns: self.frame_background_poll_max_ns,
            },
            FramePhase::Paint => FramePhaseMetricsSnapshot {
                total_ns: self.frame_paint_total_ns,
                max_ns: self.frame_paint_max_ns,
            },
            FramePhase::Chrome => FramePhaseMetricsSnapshot {
                total_ns: self.frame_chrome_total_ns,
                max_ns: self.frame_chrome_max_ns,
            },
            FramePhase::ActiveSurface => FramePhaseMetricsSnapshot {
                total_ns: self.frame_active_surface_total_ns,
                max_ns: self.frame_active_surface_max_ns,
            },
            FramePhase::Gutter => FramePhaseMetricsSnapshot {
                total_ns: self.frame_gutter_total_ns,
                max_ns: self.frame_gutter_max_ns,
            },
            FramePhase::Scroll => FramePhaseMetricsSnapshot {
                total_ns: self.frame_scroll_total_ns,
                max_ns: self.frame_scroll_max_ns,
            },
            FramePhase::Dialogs => FramePhaseMetricsSnapshot {
                total_ns: self.frame_dialogs_total_ns,
                max_ns: self.frame_dialogs_max_ns,
            },
            FramePhase::Shortcuts => FramePhaseMetricsSnapshot {
                total_ns: self.frame_shortcuts_total_ns,
                max_ns: self.frame_shortcuts_max_ns,
            },
            FramePhase::Finish => FramePhaseMetricsSnapshot {
                total_ns: self.frame_finish_total_ns,
                max_ns: self.frame_finish_max_ns,
            },
        }
    }

    #[must_use]
    pub fn frame_time_mean_ns(&self) -> f64 {
        divide_u64(self.frame_time_total_ns, self.frame_count)
    }

    #[must_use]
    pub fn frame_time_percentile_ns(&self, percentile: f64) -> f64 {
        if self.frame_count == 0 {
            return 0.0;
        }
        let target = ((self.frame_count as f64) * percentile).ceil() as u64;
        let mut cumulative = 0;
        for (index, count) in self.frame_time_bucket_counts.iter().enumerate() {
            cumulative += count;
            if cumulative >= target {
                return (((index as u64) + 1) * self.frame_time_bucket_width_ns) as f64;
            }
        }
        self.frame_time_max_ns as f64
    }
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
    for bucket in &FRAME_TIME_BUCKET_COUNTS {
        bucket.store(0, Ordering::Relaxed);
    }
    FRAME_PREPARE_TOTAL_NS.store(0, Ordering::Relaxed);
    FRAME_PREPARE_MAX_NS.store(0, Ordering::Relaxed);
    FRAME_BACKGROUND_POLL_TOTAL_NS.store(0, Ordering::Relaxed);
    FRAME_BACKGROUND_POLL_MAX_NS.store(0, Ordering::Relaxed);
    FRAME_PAINT_TOTAL_NS.store(0, Ordering::Relaxed);
    FRAME_PAINT_MAX_NS.store(0, Ordering::Relaxed);
    FRAME_CHROME_TOTAL_NS.store(0, Ordering::Relaxed);
    FRAME_CHROME_MAX_NS.store(0, Ordering::Relaxed);
    FRAME_ACTIVE_SURFACE_TOTAL_NS.store(0, Ordering::Relaxed);
    FRAME_ACTIVE_SURFACE_MAX_NS.store(0, Ordering::Relaxed);
    FRAME_GUTTER_TOTAL_NS.store(0, Ordering::Relaxed);
    FRAME_GUTTER_MAX_NS.store(0, Ordering::Relaxed);
    FRAME_SCROLL_TOTAL_NS.store(0, Ordering::Relaxed);
    FRAME_SCROLL_MAX_NS.store(0, Ordering::Relaxed);
    FRAME_DIALOGS_TOTAL_NS.store(0, Ordering::Relaxed);
    FRAME_DIALOGS_MAX_NS.store(0, Ordering::Relaxed);
    FRAME_SHORTCUTS_TOTAL_NS.store(0, Ordering::Relaxed);
    FRAME_SHORTCUTS_MAX_NS.store(0, Ordering::Relaxed);
    FRAME_FINISH_TOTAL_NS.store(0, Ordering::Relaxed);
    FRAME_FINISH_MAX_NS.store(0, Ordering::Relaxed);
    HISTORY_EVICTIONS_PER_FILE.store(0, Ordering::Relaxed);
    HISTORY_EVICTIONS_AGGREGATE.store(0, Ordering::Relaxed);
    HISTORY_EVICTED_BYTES.store(0, Ordering::Relaxed);
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
        frame_time_bucket_width_ns: FRAME_HISTOGRAM_BUCKET_WIDTH_NS,
        frame_time_bucket_counts: frame_bucket_counts(),
        frame_prepare_total_ns: FRAME_PREPARE_TOTAL_NS.load(Ordering::Relaxed),
        frame_prepare_max_ns: FRAME_PREPARE_MAX_NS.load(Ordering::Relaxed),
        frame_background_poll_total_ns: FRAME_BACKGROUND_POLL_TOTAL_NS.load(Ordering::Relaxed),
        frame_background_poll_max_ns: FRAME_BACKGROUND_POLL_MAX_NS.load(Ordering::Relaxed),
        frame_paint_total_ns: FRAME_PAINT_TOTAL_NS.load(Ordering::Relaxed),
        frame_paint_max_ns: FRAME_PAINT_MAX_NS.load(Ordering::Relaxed),
        frame_chrome_total_ns: FRAME_CHROME_TOTAL_NS.load(Ordering::Relaxed),
        frame_chrome_max_ns: FRAME_CHROME_MAX_NS.load(Ordering::Relaxed),
        frame_active_surface_total_ns: FRAME_ACTIVE_SURFACE_TOTAL_NS.load(Ordering::Relaxed),
        frame_active_surface_max_ns: FRAME_ACTIVE_SURFACE_MAX_NS.load(Ordering::Relaxed),
        frame_gutter_total_ns: FRAME_GUTTER_TOTAL_NS.load(Ordering::Relaxed),
        frame_gutter_max_ns: FRAME_GUTTER_MAX_NS.load(Ordering::Relaxed),
        frame_scroll_total_ns: FRAME_SCROLL_TOTAL_NS.load(Ordering::Relaxed),
        frame_scroll_max_ns: FRAME_SCROLL_MAX_NS.load(Ordering::Relaxed),
        frame_dialogs_total_ns: FRAME_DIALOGS_TOTAL_NS.load(Ordering::Relaxed),
        frame_dialogs_max_ns: FRAME_DIALOGS_MAX_NS.load(Ordering::Relaxed),
        frame_shortcuts_total_ns: FRAME_SHORTCUTS_TOTAL_NS.load(Ordering::Relaxed),
        frame_shortcuts_max_ns: FRAME_SHORTCUTS_MAX_NS.load(Ordering::Relaxed),
        frame_finish_total_ns: FRAME_FINISH_TOTAL_NS.load(Ordering::Relaxed),
        frame_finish_max_ns: FRAME_FINISH_MAX_NS.load(Ordering::Relaxed),
        history_evictions_per_file: HISTORY_EVICTIONS_PER_FILE.load(Ordering::Relaxed),
        history_evictions_aggregate: HISTORY_EVICTIONS_AGGREGATE.load(Ordering::Relaxed),
        history_evicted_bytes: HISTORY_EVICTED_BYTES.load(Ordering::Relaxed),
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
    FRAME_TIME_BUCKET_COUNTS[frame_bucket_index(elapsed_ns)].fetch_add(1, Ordering::Relaxed);
    update_max(&FRAME_TIME_MAX_NS, elapsed_ns);
}

pub fn record_frame_phase(phase: FramePhase, elapsed: Duration) {
    let elapsed_ns = saturating_u64(elapsed.as_nanos());
    let (total, max) = match phase {
        FramePhase::Prepare => (&FRAME_PREPARE_TOTAL_NS, &FRAME_PREPARE_MAX_NS),
        FramePhase::BackgroundPoll => (
            &FRAME_BACKGROUND_POLL_TOTAL_NS,
            &FRAME_BACKGROUND_POLL_MAX_NS,
        ),
        FramePhase::Paint => (&FRAME_PAINT_TOTAL_NS, &FRAME_PAINT_MAX_NS),
        FramePhase::Chrome => (&FRAME_CHROME_TOTAL_NS, &FRAME_CHROME_MAX_NS),
        FramePhase::ActiveSurface => (&FRAME_ACTIVE_SURFACE_TOTAL_NS, &FRAME_ACTIVE_SURFACE_MAX_NS),
        FramePhase::Gutter => (&FRAME_GUTTER_TOTAL_NS, &FRAME_GUTTER_MAX_NS),
        FramePhase::Scroll => (&FRAME_SCROLL_TOTAL_NS, &FRAME_SCROLL_MAX_NS),
        FramePhase::Dialogs => (&FRAME_DIALOGS_TOTAL_NS, &FRAME_DIALOGS_MAX_NS),
        FramePhase::Shortcuts => (&FRAME_SHORTCUTS_TOTAL_NS, &FRAME_SHORTCUTS_MAX_NS),
        FramePhase::Finish => (&FRAME_FINISH_TOTAL_NS, &FRAME_FINISH_MAX_NS),
    };
    total.fetch_add(elapsed_ns, Ordering::Relaxed);
    update_max(max, elapsed_ns);
}

fn frame_bucket_counts() -> [u64; FRAME_HISTOGRAM_BUCKETS] {
    let mut counts = [0; FRAME_HISTOGRAM_BUCKETS];
    for (index, bucket) in FRAME_TIME_BUCKET_COUNTS.iter().enumerate() {
        counts[index] = bucket.load(Ordering::Relaxed);
    }
    counts
}

fn frame_bucket_index(elapsed_ns: u64) -> usize {
    ((elapsed_ns / FRAME_HISTOGRAM_BUCKET_WIDTH_NS) as usize).min(FRAME_HISTOGRAM_BUCKETS - 1)
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

#[cfg(test)]
mod tests {
    use super::{
        BackgroundIoLane, CapacityMetricsSnapshot, FramePhase, capacity_metrics_snapshot,
        record_background_io_lane, record_background_io_queue_depth,
        record_background_io_saturation, record_frame, record_frame_phase, reset_capacity_metrics,
    };
    use std::time::Duration;

    #[test]
    fn snapshot_groups_background_io_lane_metrics() {
        reset_capacity_metrics();
        record_background_io_lane(BackgroundIoLane::Path, Duration::from_millis(2));
        record_background_io_queue_depth(BackgroundIoLane::Path, 7);
        record_background_io_saturation(BackgroundIoLane::Path);

        let lane = capacity_metrics_snapshot().background_io_lane(BackgroundIoLane::Path);
        assert_eq!(lane.requests, 1);
        assert_eq!(lane.active_ns, 2_000_000);
        assert_eq!(lane.max_queue_depth, 7);
        assert_eq!(lane.saturation_count, 1);
    }

    #[test]
    fn snapshot_groups_frame_phase_metrics() {
        reset_capacity_metrics();
        record_frame_phase(FramePhase::Prepare, Duration::from_millis(10));
        record_frame_phase(FramePhase::Prepare, Duration::from_millis(3));

        let phase = capacity_metrics_snapshot().frame_phase(FramePhase::Prepare);
        assert_eq!(phase.total_ns, 13_000_000);
        assert_eq!(phase.max_ns, 10_000_000);
    }

    #[test]
    fn snapshot_reports_frame_time_summary_metrics() {
        reset_capacity_metrics();
        record_frame(Duration::from_millis(1));
        record_frame(Duration::from_millis(3));

        let snapshot = capacity_metrics_snapshot();
        assert_eq!(snapshot.frame_time_mean_ns(), 2_000_000.0);
        assert_eq!(snapshot.frame_time_percentile_ns(0.50), 2_000_000.0);
        assert_eq!(
            CapacityMetricsSnapshot::default().frame_time_percentile_ns(0.95),
            0.0
        );
    }
}
