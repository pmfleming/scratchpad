use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use serde::Serialize;

use super::{
    CapacityMetricsSnapshot, FRAME_HISTOGRAM_BUCKETS, divide_u64, load_counter, reset_counter,
    saturating_u64, update_max,
};

const FRAME_HISTOGRAM_BUCKET_WIDTH_NS: u64 = 500_000;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct FramePhaseMetricsSnapshot {
    pub total_ns: u64,
    pub max_ns: u64,
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct MetricsSnapshot {
    pub count: u64,
    pub total_ns: u64,
    pub max_ns: u64,
    pub bucket_width_ns: u64,
    pub bucket_counts: [u64; FRAME_HISTOGRAM_BUCKETS],
    pub prepare: FramePhaseMetricsSnapshot,
    pub background_poll: FramePhaseMetricsSnapshot,
    pub paint: FramePhaseMetricsSnapshot,
    pub chrome: FramePhaseMetricsSnapshot,
    pub active_surface: FramePhaseMetricsSnapshot,
    pub gutter: FramePhaseMetricsSnapshot,
    pub scroll: FramePhaseMetricsSnapshot,
    pub dialogs: FramePhaseMetricsSnapshot,
    pub shortcuts: FramePhaseMetricsSnapshot,
    pub finish: FramePhaseMetricsSnapshot,
}

static FRAME_COUNT: AtomicU64 = AtomicU64::new(0);
static FRAME_TIME_TOTAL_NS: AtomicU64 = AtomicU64::new(0);
static FRAME_TIME_MAX_NS: AtomicU64 = AtomicU64::new(0);
static FRAME_TIME_BUCKET_COUNTS: [AtomicU64; FRAME_HISTOGRAM_BUCKETS] =
    [const { AtomicU64::new(0) }; FRAME_HISTOGRAM_BUCKETS];

/// Phase counters are indexed by `FramePhase as usize`.
const PHASE_COUNT: usize = FramePhase::Finish as usize + 1;
static PHASE_TOTAL_NS: [AtomicU64; PHASE_COUNT] = [const { AtomicU64::new(0) }; PHASE_COUNT];
static PHASE_MAX_NS: [AtomicU64; PHASE_COUNT] = [const { AtomicU64::new(0) }; PHASE_COUNT];

impl CapacityMetricsSnapshot {
    #[must_use]
    pub fn frame_phase(&self, phase: FramePhase) -> FramePhaseMetricsSnapshot {
        let (total_ns, max_ns) = match phase {
            FramePhase::Prepare => (self.frame_prepare_total_ns, self.frame_prepare_max_ns),
            FramePhase::BackgroundPoll => (
                self.frame_background_poll_total_ns,
                self.frame_background_poll_max_ns,
            ),
            FramePhase::Paint => (self.frame_paint_total_ns, self.frame_paint_max_ns),
            FramePhase::Chrome => (self.frame_chrome_total_ns, self.frame_chrome_max_ns),
            FramePhase::ActiveSurface => (
                self.frame_active_surface_total_ns,
                self.frame_active_surface_max_ns,
            ),
            FramePhase::Gutter => (self.frame_gutter_total_ns, self.frame_gutter_max_ns),
            FramePhase::Scroll => (self.frame_scroll_total_ns, self.frame_scroll_max_ns),
            FramePhase::Dialogs => (self.frame_dialogs_total_ns, self.frame_dialogs_max_ns),
            FramePhase::Shortcuts => (self.frame_shortcuts_total_ns, self.frame_shortcuts_max_ns),
            FramePhase::Finish => (self.frame_finish_total_ns, self.frame_finish_max_ns),
        };
        FramePhaseMetricsSnapshot { total_ns, max_ns }
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
        let Some(index) = self.frame_time_bucket_counts.iter().position(|count| {
            cumulative += count;
            cumulative >= target
        }) else {
            return self.frame_time_max_ns as f64;
        };
        let upper = ((index as u64) + 1) * self.frame_time_bucket_width_ns;
        // The final bucket includes overflow; its nominal edge is not an upper bound.
        // Return a conservative observed bound rather than clipping long stalls.
        if index + 1 == FRAME_HISTOGRAM_BUCKETS {
            upper.max(self.frame_time_max_ns) as f64
        } else {
            upper as f64
        }
    }
}

pub(super) fn reset() {
    [&FRAME_COUNT, &FRAME_TIME_TOTAL_NS, &FRAME_TIME_MAX_NS]
        .into_iter()
        .chain(&FRAME_TIME_BUCKET_COUNTS)
        .chain(&PHASE_TOTAL_NS)
        .chain(&PHASE_MAX_NS)
        .for_each(reset_counter);
}

pub(super) fn snapshot() -> MetricsSnapshot {
    MetricsSnapshot {
        count: load_counter(&FRAME_COUNT),
        total_ns: load_counter(&FRAME_TIME_TOTAL_NS),
        max_ns: load_counter(&FRAME_TIME_MAX_NS),
        bucket_width_ns: FRAME_HISTOGRAM_BUCKET_WIDTH_NS,
        bucket_counts: std::array::from_fn(|index| load_counter(&FRAME_TIME_BUCKET_COUNTS[index])),
        prepare: phase_snapshot(FramePhase::Prepare),
        background_poll: phase_snapshot(FramePhase::BackgroundPoll),
        paint: phase_snapshot(FramePhase::Paint),
        chrome: phase_snapshot(FramePhase::Chrome),
        active_surface: phase_snapshot(FramePhase::ActiveSurface),
        gutter: phase_snapshot(FramePhase::Gutter),
        scroll: phase_snapshot(FramePhase::Scroll),
        dialogs: phase_snapshot(FramePhase::Dialogs),
        shortcuts: phase_snapshot(FramePhase::Shortcuts),
        finish: phase_snapshot(FramePhase::Finish),
    }
}

pub fn record_frame(elapsed: Duration) {
    let elapsed_ns = saturating_u64(elapsed.as_nanos());
    FRAME_COUNT.fetch_add(1, Ordering::Relaxed);
    FRAME_TIME_TOTAL_NS.fetch_add(elapsed_ns, Ordering::Relaxed);
    FRAME_TIME_BUCKET_COUNTS[bucket_index(elapsed_ns)].fetch_add(1, Ordering::Relaxed);
    update_max(&FRAME_TIME_MAX_NS, elapsed_ns);
}

pub fn record_frame_phase(phase: FramePhase, elapsed: Duration) {
    let elapsed_ns = saturating_u64(elapsed.as_nanos());
    PHASE_TOTAL_NS[phase as usize].fetch_add(elapsed_ns, Ordering::Relaxed);
    update_max(&PHASE_MAX_NS[phase as usize], elapsed_ns);
}

fn phase_snapshot(phase: FramePhase) -> FramePhaseMetricsSnapshot {
    FramePhaseMetricsSnapshot {
        total_ns: load_counter(&PHASE_TOTAL_NS[phase as usize]),
        max_ns: load_counter(&PHASE_MAX_NS[phase as usize]),
    }
}

fn bucket_index(elapsed_ns: u64) -> usize {
    ((elapsed_ns.saturating_sub(1) / FRAME_HISTOGRAM_BUCKET_WIDTH_NS) as usize)
        .min(FRAME_HISTOGRAM_BUCKETS - 1)
}
