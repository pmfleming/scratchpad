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

#[derive(Clone, Copy)]
struct BackgroundIoLaneCounters {
    requests: &'static AtomicU64,
    active_ns: &'static AtomicU64,
    max_queue_depth: &'static AtomicU64,
    saturation_count: &'static AtomicU64,
}

#[derive(Clone, Copy)]
struct FramePhaseCounters {
    total_ns: &'static AtomicU64,
    max_ns: &'static AtomicU64,
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
    BACKGROUND_IO_PATH_REQUESTS,
    BACKGROUND_IO_PATH_ACTIVE_NS,
    BACKGROUND_IO_PATH_MAX_QUEUE_DEPTH,
    BACKGROUND_IO_SESSION_REQUESTS,
    BACKGROUND_IO_SESSION_ACTIVE_NS,
    BACKGROUND_IO_SESSION_MAX_QUEUE_DEPTH,
    BACKGROUND_IO_ANALYSIS_REQUESTS,
    BACKGROUND_IO_ANALYSIS_ACTIVE_NS,
    BACKGROUND_IO_ANALYSIS_MAX_QUEUE_DEPTH,
    BACKGROUND_IO_PATH_SATURATION_COUNT,
    BACKGROUND_IO_SESSION_SATURATION_COUNT,
    BACKGROUND_IO_ANALYSIS_SATURATION_COUNT,
    LAYOUT_CACHE_HIT_COUNT,
    LAYOUT_CACHE_MISS_COUNT,
    FRAME_COUNT,
    FRAME_TIME_TOTAL_NS,
    FRAME_TIME_MAX_NS,
}
static FRAME_TIME_BUCKET_COUNTS: [AtomicU64; FRAME_HISTOGRAM_BUCKETS] =
    [const { AtomicU64::new(0) }; FRAME_HISTOGRAM_BUCKETS];
capacity_counters! {
    FRAME_PREPARE_TOTAL_NS,
    FRAME_PREPARE_MAX_NS,
    FRAME_BACKGROUND_POLL_TOTAL_NS,
    FRAME_BACKGROUND_POLL_MAX_NS,
    FRAME_PAINT_TOTAL_NS,
    FRAME_PAINT_MAX_NS,
    FRAME_CHROME_TOTAL_NS,
    FRAME_CHROME_MAX_NS,
    FRAME_ACTIVE_SURFACE_TOTAL_NS,
    FRAME_ACTIVE_SURFACE_MAX_NS,
    FRAME_GUTTER_TOTAL_NS,
    FRAME_GUTTER_MAX_NS,
    FRAME_SCROLL_TOTAL_NS,
    FRAME_SCROLL_MAX_NS,
    FRAME_DIALOGS_TOTAL_NS,
    FRAME_DIALOGS_MAX_NS,
    FRAME_SHORTCUTS_TOTAL_NS,
    FRAME_SHORTCUTS_MAX_NS,
    FRAME_FINISH_TOTAL_NS,
    FRAME_FINISH_MAX_NS,
    HISTORY_EVICTIONS_PER_FILE,
    HISTORY_EVICTIONS_AGGREGATE,
    HISTORY_EVICTED_BYTES,
}

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
            BackgroundIoLane::Path => self.path_lane_metrics(),
            BackgroundIoLane::Session => self.session_lane_metrics(),
            BackgroundIoLane::Analysis => self.analysis_lane_metrics(),
        }
    }

    fn path_lane_metrics(&self) -> BackgroundIoLaneMetricsSnapshot {
        BackgroundIoLaneMetricsSnapshot {
            requests: self.background_io_path_requests,
            active_ns: self.background_io_path_active_ns,
            max_queue_depth: self.background_io_path_max_queue_depth,
            saturation_count: self.background_io_path_saturation_count,
        }
    }

    fn session_lane_metrics(&self) -> BackgroundIoLaneMetricsSnapshot {
        BackgroundIoLaneMetricsSnapshot {
            requests: self.background_io_session_requests,
            active_ns: self.background_io_session_active_ns,
            max_queue_depth: self.background_io_session_max_queue_depth,
            saturation_count: self.background_io_session_saturation_count,
        }
    }

    fn analysis_lane_metrics(&self) -> BackgroundIoLaneMetricsSnapshot {
        BackgroundIoLaneMetricsSnapshot {
            requests: self.background_io_analysis_requests,
            active_ns: self.background_io_analysis_active_ns,
            max_queue_depth: self.background_io_analysis_max_queue_depth,
            saturation_count: self.background_io_analysis_saturation_count,
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
        &FRAME_COUNT,
        &FRAME_TIME_TOTAL_NS,
        &FRAME_TIME_MAX_NS,
        &HISTORY_EVICTIONS_PER_FILE,
        &HISTORY_EVICTIONS_AGGREGATE,
        &HISTORY_EVICTED_BYTES,
    ]);
    for lane in [
        BackgroundIoLane::Path,
        BackgroundIoLane::Session,
        BackgroundIoLane::Analysis,
    ] {
        background_io_lane_counters(lane).reset();
    }
    for bucket in &FRAME_TIME_BUCKET_COUNTS {
        reset_counter(bucket);
    }
    for phase in [
        FramePhase::Prepare,
        FramePhase::BackgroundPoll,
        FramePhase::Paint,
        FramePhase::Chrome,
        FramePhase::ActiveSurface,
        FramePhase::Gutter,
        FramePhase::Scroll,
        FramePhase::Dialogs,
        FramePhase::Shortcuts,
        FramePhase::Finish,
    ] {
        frame_phase_counters(phase).reset();
    }
}

pub fn capacity_metrics_snapshot() -> CapacityMetricsSnapshot {
    let path_lane = background_io_lane_counters(BackgroundIoLane::Path).snapshot();
    let session_lane = background_io_lane_counters(BackgroundIoLane::Session).snapshot();
    let analysis_lane = background_io_lane_counters(BackgroundIoLane::Analysis).snapshot();
    let prepare_phase = frame_phase_counters(FramePhase::Prepare).snapshot();
    let background_poll_phase = frame_phase_counters(FramePhase::BackgroundPoll).snapshot();
    let paint_phase = frame_phase_counters(FramePhase::Paint).snapshot();
    let chrome_phase = frame_phase_counters(FramePhase::Chrome).snapshot();
    let active_surface_phase = frame_phase_counters(FramePhase::ActiveSurface).snapshot();
    let gutter_phase = frame_phase_counters(FramePhase::Gutter).snapshot();
    let scroll_phase = frame_phase_counters(FramePhase::Scroll).snapshot();
    let dialogs_phase = frame_phase_counters(FramePhase::Dialogs).snapshot();
    let shortcuts_phase = frame_phase_counters(FramePhase::Shortcuts).snapshot();
    let finish_phase = frame_phase_counters(FramePhase::Finish).snapshot();

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
        background_io_path_requests: path_lane.requests,
        background_io_path_active_ns: path_lane.active_ns,
        background_io_path_max_queue_depth: path_lane.max_queue_depth,
        background_io_session_requests: session_lane.requests,
        background_io_session_active_ns: session_lane.active_ns,
        background_io_session_max_queue_depth: session_lane.max_queue_depth,
        background_io_analysis_requests: analysis_lane.requests,
        background_io_analysis_active_ns: analysis_lane.active_ns,
        background_io_analysis_max_queue_depth: analysis_lane.max_queue_depth,
        background_io_path_saturation_count: path_lane.saturation_count,
        background_io_session_saturation_count: session_lane.saturation_count,
        background_io_analysis_saturation_count: analysis_lane.saturation_count,
        layout_cache_hit_count: load_counter(&LAYOUT_CACHE_HIT_COUNT),
        layout_cache_miss_count: load_counter(&LAYOUT_CACHE_MISS_COUNT),
        frame_count: load_counter(&FRAME_COUNT),
        frame_time_total_ns: load_counter(&FRAME_TIME_TOTAL_NS),
        frame_time_max_ns: load_counter(&FRAME_TIME_MAX_NS),
        frame_time_bucket_width_ns: FRAME_HISTOGRAM_BUCKET_WIDTH_NS,
        frame_time_bucket_counts: frame_bucket_counts(),
        frame_prepare_total_ns: prepare_phase.total_ns,
        frame_prepare_max_ns: prepare_phase.max_ns,
        frame_background_poll_total_ns: background_poll_phase.total_ns,
        frame_background_poll_max_ns: background_poll_phase.max_ns,
        frame_paint_total_ns: paint_phase.total_ns,
        frame_paint_max_ns: paint_phase.max_ns,
        frame_chrome_total_ns: chrome_phase.total_ns,
        frame_chrome_max_ns: chrome_phase.max_ns,
        frame_active_surface_total_ns: active_surface_phase.total_ns,
        frame_active_surface_max_ns: active_surface_phase.max_ns,
        frame_gutter_total_ns: gutter_phase.total_ns,
        frame_gutter_max_ns: gutter_phase.max_ns,
        frame_scroll_total_ns: scroll_phase.total_ns,
        frame_scroll_max_ns: scroll_phase.max_ns,
        frame_dialogs_total_ns: dialogs_phase.total_ns,
        frame_dialogs_max_ns: dialogs_phase.max_ns,
        frame_shortcuts_total_ns: shortcuts_phase.total_ns,
        frame_shortcuts_max_ns: shortcuts_phase.max_ns,
        frame_finish_total_ns: finish_phase.total_ns,
        frame_finish_max_ns: finish_phase.max_ns,
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

pub fn record_background_io_lane(lane: BackgroundIoLane, elapsed: Duration) {
    let elapsed_ns = saturating_u64(elapsed.as_nanos());
    background_io_lane_counters(lane).record_elapsed(elapsed_ns);
}

pub fn record_background_io_queue_depth(lane: BackgroundIoLane, depth: usize) {
    background_io_lane_counters(lane).record_queue_depth(saturating_u64(depth));
}

pub fn record_background_io_saturation(lane: BackgroundIoLane) {
    background_io_lane_counters(lane).record_saturation();
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
    frame_phase_counters(phase).record(elapsed_ns);
}

impl BackgroundIoLaneCounters {
    fn reset(self) {
        reset_counters(&[
            self.requests,
            self.active_ns,
            self.max_queue_depth,
            self.saturation_count,
        ]);
    }

    fn snapshot(self) -> BackgroundIoLaneMetricsSnapshot {
        BackgroundIoLaneMetricsSnapshot {
            requests: load_counter(self.requests),
            active_ns: load_counter(self.active_ns),
            max_queue_depth: load_counter(self.max_queue_depth),
            saturation_count: load_counter(self.saturation_count),
        }
    }

    fn record_elapsed(self, elapsed_ns: u64) {
        self.requests.fetch_add(1, Ordering::Relaxed);
        self.active_ns.fetch_add(elapsed_ns, Ordering::Relaxed);
    }

    fn record_queue_depth(self, depth: u64) {
        update_max(self.max_queue_depth, depth);
    }

    fn record_saturation(self) {
        self.saturation_count.fetch_add(1, Ordering::Relaxed);
    }
}

fn background_io_lane_counters(lane: BackgroundIoLane) -> BackgroundIoLaneCounters {
    match lane {
        BackgroundIoLane::Path => BackgroundIoLaneCounters {
            requests: &BACKGROUND_IO_PATH_REQUESTS,
            active_ns: &BACKGROUND_IO_PATH_ACTIVE_NS,
            max_queue_depth: &BACKGROUND_IO_PATH_MAX_QUEUE_DEPTH,
            saturation_count: &BACKGROUND_IO_PATH_SATURATION_COUNT,
        },
        BackgroundIoLane::Session => BackgroundIoLaneCounters {
            requests: &BACKGROUND_IO_SESSION_REQUESTS,
            active_ns: &BACKGROUND_IO_SESSION_ACTIVE_NS,
            max_queue_depth: &BACKGROUND_IO_SESSION_MAX_QUEUE_DEPTH,
            saturation_count: &BACKGROUND_IO_SESSION_SATURATION_COUNT,
        },
        BackgroundIoLane::Analysis => BackgroundIoLaneCounters {
            requests: &BACKGROUND_IO_ANALYSIS_REQUESTS,
            active_ns: &BACKGROUND_IO_ANALYSIS_ACTIVE_NS,
            max_queue_depth: &BACKGROUND_IO_ANALYSIS_MAX_QUEUE_DEPTH,
            saturation_count: &BACKGROUND_IO_ANALYSIS_SATURATION_COUNT,
        },
    }
}

impl FramePhaseCounters {
    fn reset(self) {
        reset_counters(&[self.total_ns, self.max_ns]);
    }

    fn snapshot(self) -> FramePhaseMetricsSnapshot {
        FramePhaseMetricsSnapshot {
            total_ns: load_counter(self.total_ns),
            max_ns: load_counter(self.max_ns),
        }
    }

    fn record(self, elapsed_ns: u64) {
        self.total_ns.fetch_add(elapsed_ns, Ordering::Relaxed);
        update_max(self.max_ns, elapsed_ns);
    }
}

fn frame_phase_counters(phase: FramePhase) -> FramePhaseCounters {
    match phase {
        FramePhase::Prepare => FramePhaseCounters {
            total_ns: &FRAME_PREPARE_TOTAL_NS,
            max_ns: &FRAME_PREPARE_MAX_NS,
        },
        FramePhase::BackgroundPoll => FramePhaseCounters {
            total_ns: &FRAME_BACKGROUND_POLL_TOTAL_NS,
            max_ns: &FRAME_BACKGROUND_POLL_MAX_NS,
        },
        FramePhase::Paint => FramePhaseCounters {
            total_ns: &FRAME_PAINT_TOTAL_NS,
            max_ns: &FRAME_PAINT_MAX_NS,
        },
        FramePhase::Chrome => FramePhaseCounters {
            total_ns: &FRAME_CHROME_TOTAL_NS,
            max_ns: &FRAME_CHROME_MAX_NS,
        },
        FramePhase::ActiveSurface => FramePhaseCounters {
            total_ns: &FRAME_ACTIVE_SURFACE_TOTAL_NS,
            max_ns: &FRAME_ACTIVE_SURFACE_MAX_NS,
        },
        FramePhase::Gutter => FramePhaseCounters {
            total_ns: &FRAME_GUTTER_TOTAL_NS,
            max_ns: &FRAME_GUTTER_MAX_NS,
        },
        FramePhase::Scroll => FramePhaseCounters {
            total_ns: &FRAME_SCROLL_TOTAL_NS,
            max_ns: &FRAME_SCROLL_MAX_NS,
        },
        FramePhase::Dialogs => FramePhaseCounters {
            total_ns: &FRAME_DIALOGS_TOTAL_NS,
            max_ns: &FRAME_DIALOGS_MAX_NS,
        },
        FramePhase::Shortcuts => FramePhaseCounters {
            total_ns: &FRAME_SHORTCUTS_TOTAL_NS,
            max_ns: &FRAME_SHORTCUTS_MAX_NS,
        },
        FramePhase::Finish => FramePhaseCounters {
            total_ns: &FRAME_FINISH_TOTAL_NS,
            max_ns: &FRAME_FINISH_MAX_NS,
        },
    }
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

fn frame_bucket_counts() -> [u64; FRAME_HISTOGRAM_BUCKETS] {
    let mut counts = [0; FRAME_HISTOGRAM_BUCKETS];
    for (index, bucket) in FRAME_TIME_BUCKET_COUNTS.iter().enumerate() {
        counts[index] = load_counter(bucket);
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
