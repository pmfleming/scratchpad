use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use serde::Serialize;

use super::{CapacityMetricsSnapshot, load_counter, reset_counters, saturating_u64, update_max};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct BackgroundIoLaneMetricsSnapshot {
    pub requests: u64,
    pub active_ns: u64,
    pub max_queue_depth: u64,
    pub saturation_count: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BackgroundIoLane {
    Path,
    Session,
    Analysis,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct MetricsSnapshot {
    pub path: BackgroundIoLaneMetricsSnapshot,
    pub session: BackgroundIoLaneMetricsSnapshot,
    pub analysis: BackgroundIoLaneMetricsSnapshot,
}

#[derive(Clone, Copy)]
struct Counters {
    requests: &'static AtomicU64,
    active_ns: &'static AtomicU64,
    max_queue_depth: &'static AtomicU64,
    saturation_count: &'static AtomicU64,
}

macro_rules! lane_counters {
    ($($name:ident),+ $(,)?) => {
        $(static $name: AtomicU64 = AtomicU64::new(0);)+
    };
}

lane_counters! {
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
}

impl BackgroundIoLane {
    const ALL: [Self; 3] = [Self::Path, Self::Session, Self::Analysis];
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
}

pub(super) fn reset() {
    for lane in BackgroundIoLane::ALL {
        counters(lane).reset();
    }
}

pub(super) fn snapshot() -> MetricsSnapshot {
    MetricsSnapshot {
        path: counters(BackgroundIoLane::Path).snapshot(),
        session: counters(BackgroundIoLane::Session).snapshot(),
        analysis: counters(BackgroundIoLane::Analysis).snapshot(),
    }
}

pub fn record_background_io_lane(lane: BackgroundIoLane, elapsed: Duration) {
    counters(lane).record_elapsed(saturating_u64(elapsed.as_nanos()));
}

pub fn record_background_io_queue_depth(lane: BackgroundIoLane, depth: usize) {
    counters(lane).record_queue_depth(saturating_u64(depth));
}

pub fn record_background_io_saturation(lane: BackgroundIoLane) {
    counters(lane).record_saturation();
}

impl Counters {
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

fn counters(lane: BackgroundIoLane) -> Counters {
    match lane {
        BackgroundIoLane::Path => Counters {
            requests: &BACKGROUND_IO_PATH_REQUESTS,
            active_ns: &BACKGROUND_IO_PATH_ACTIVE_NS,
            max_queue_depth: &BACKGROUND_IO_PATH_MAX_QUEUE_DEPTH,
            saturation_count: &BACKGROUND_IO_PATH_SATURATION_COUNT,
        },
        BackgroundIoLane::Session => Counters {
            requests: &BACKGROUND_IO_SESSION_REQUESTS,
            active_ns: &BACKGROUND_IO_SESSION_ACTIVE_NS,
            max_queue_depth: &BACKGROUND_IO_SESSION_MAX_QUEUE_DEPTH,
            saturation_count: &BACKGROUND_IO_SESSION_SATURATION_COUNT,
        },
        BackgroundIoLane::Analysis => Counters {
            requests: &BACKGROUND_IO_ANALYSIS_REQUESTS,
            active_ns: &BACKGROUND_IO_ANALYSIS_ACTIVE_NS,
            max_queue_depth: &BACKGROUND_IO_ANALYSIS_MAX_QUEUE_DEPTH,
            saturation_count: &BACKGROUND_IO_ANALYSIS_SATURATION_COUNT,
        },
    }
}
