use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use serde::Serialize;

use super::{CapacityMetricsSnapshot, load_counter, reset_counter, saturating_u64, update_max};

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

/// Lane counters are indexed by `BackgroundIoLane as usize`.
const LANE_COUNT: usize = BackgroundIoLane::Analysis as usize + 1;
static REQUESTS: [AtomicU64; LANE_COUNT] = [const { AtomicU64::new(0) }; LANE_COUNT];
static ACTIVE_NS: [AtomicU64; LANE_COUNT] = [const { AtomicU64::new(0) }; LANE_COUNT];
static MAX_QUEUE_DEPTH: [AtomicU64; LANE_COUNT] = [const { AtomicU64::new(0) }; LANE_COUNT];
static SATURATION_COUNT: [AtomicU64; LANE_COUNT] = [const { AtomicU64::new(0) }; LANE_COUNT];

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
    REQUESTS
        .iter()
        .chain(&ACTIVE_NS)
        .chain(&MAX_QUEUE_DEPTH)
        .chain(&SATURATION_COUNT)
        .for_each(reset_counter);
}

pub(super) fn snapshot() -> MetricsSnapshot {
    MetricsSnapshot {
        path: lane_snapshot(BackgroundIoLane::Path),
        session: lane_snapshot(BackgroundIoLane::Session),
        analysis: lane_snapshot(BackgroundIoLane::Analysis),
    }
}

pub fn record_background_io_lane(lane: BackgroundIoLane, elapsed: Duration) {
    REQUESTS[lane as usize].fetch_add(1, Ordering::Relaxed);
    ACTIVE_NS[lane as usize].fetch_add(saturating_u64(elapsed.as_nanos()), Ordering::Relaxed);
}

pub fn record_background_io_queue_depth(lane: BackgroundIoLane, depth: usize) {
    update_max(&MAX_QUEUE_DEPTH[lane as usize], saturating_u64(depth));
}

pub fn record_background_io_saturation(lane: BackgroundIoLane) {
    SATURATION_COUNT[lane as usize].fetch_add(1, Ordering::Relaxed);
}

fn lane_snapshot(lane: BackgroundIoLane) -> BackgroundIoLaneMetricsSnapshot {
    let lane = lane as usize;
    BackgroundIoLaneMetricsSnapshot {
        requests: load_counter(&REQUESTS[lane]),
        active_ns: load_counter(&ACTIVE_NS[lane]),
        max_queue_depth: load_counter(&MAX_QUEUE_DEPTH[lane]),
        saturation_count: load_counter(&SATURATION_COUNT[lane]),
    }
}
