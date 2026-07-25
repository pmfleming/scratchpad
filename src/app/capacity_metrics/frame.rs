use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use serde::Serialize;

use super::{
    CapacityMetricsSnapshot, FRAME_HISTOGRAM_BUCKETS, divide_u64, load_counter, reset_counter,
    reset_counters, saturating_u64, update_max,
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

#[derive(Clone, Copy)]
struct Counters {
    total_ns: &'static AtomicU64,
    max_ns: &'static AtomicU64,
}

macro_rules! frame_counters {
    ($($name:ident),+ $(,)?) => {
        $(static $name: AtomicU64 = AtomicU64::new(0);)+
    };
}

frame_counters! {
    FRAME_COUNT,
    FRAME_TIME_TOTAL_NS,
    FRAME_TIME_MAX_NS,
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
}

static FRAME_TIME_BUCKET_COUNTS: [AtomicU64; FRAME_HISTOGRAM_BUCKETS] =
    [const { AtomicU64::new(0) }; FRAME_HISTOGRAM_BUCKETS];

impl FramePhase {
    const ALL: [Self; 10] = [
        Self::Prepare,
        Self::BackgroundPoll,
        Self::Paint,
        Self::Chrome,
        Self::ActiveSurface,
        Self::Gutter,
        Self::Scroll,
        Self::Dialogs,
        Self::Shortcuts,
        Self::Finish,
    ];
}

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
        for (index, count) in self.frame_time_bucket_counts.iter().enumerate() {
            cumulative += count;
            if cumulative >= target {
                return (((index as u64) + 1) * self.frame_time_bucket_width_ns) as f64;
            }
        }
        self.frame_time_max_ns as f64
    }
}

pub(super) fn reset() {
    reset_counters(&[&FRAME_COUNT, &FRAME_TIME_TOTAL_NS, &FRAME_TIME_MAX_NS]);
    for bucket in &FRAME_TIME_BUCKET_COUNTS {
        reset_counter(bucket);
    }
    for phase in FramePhase::ALL {
        counters(phase).reset();
    }
}

pub(super) fn snapshot() -> MetricsSnapshot {
    MetricsSnapshot {
        count: load_counter(&FRAME_COUNT),
        total_ns: load_counter(&FRAME_TIME_TOTAL_NS),
        max_ns: load_counter(&FRAME_TIME_MAX_NS),
        bucket_width_ns: FRAME_HISTOGRAM_BUCKET_WIDTH_NS,
        bucket_counts: bucket_counts(),
        prepare: counters(FramePhase::Prepare).snapshot(),
        background_poll: counters(FramePhase::BackgroundPoll).snapshot(),
        paint: counters(FramePhase::Paint).snapshot(),
        chrome: counters(FramePhase::Chrome).snapshot(),
        active_surface: counters(FramePhase::ActiveSurface).snapshot(),
        gutter: counters(FramePhase::Gutter).snapshot(),
        scroll: counters(FramePhase::Scroll).snapshot(),
        dialogs: counters(FramePhase::Dialogs).snapshot(),
        shortcuts: counters(FramePhase::Shortcuts).snapshot(),
        finish: counters(FramePhase::Finish).snapshot(),
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
    counters(phase).record(saturating_u64(elapsed.as_nanos()));
}

impl Counters {
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

fn counters(phase: FramePhase) -> Counters {
    match phase {
        FramePhase::Prepare => Counters {
            total_ns: &FRAME_PREPARE_TOTAL_NS,
            max_ns: &FRAME_PREPARE_MAX_NS,
        },
        FramePhase::BackgroundPoll => Counters {
            total_ns: &FRAME_BACKGROUND_POLL_TOTAL_NS,
            max_ns: &FRAME_BACKGROUND_POLL_MAX_NS,
        },
        FramePhase::Paint => Counters {
            total_ns: &FRAME_PAINT_TOTAL_NS,
            max_ns: &FRAME_PAINT_MAX_NS,
        },
        FramePhase::Chrome => Counters {
            total_ns: &FRAME_CHROME_TOTAL_NS,
            max_ns: &FRAME_CHROME_MAX_NS,
        },
        FramePhase::ActiveSurface => Counters {
            total_ns: &FRAME_ACTIVE_SURFACE_TOTAL_NS,
            max_ns: &FRAME_ACTIVE_SURFACE_MAX_NS,
        },
        FramePhase::Gutter => Counters {
            total_ns: &FRAME_GUTTER_TOTAL_NS,
            max_ns: &FRAME_GUTTER_MAX_NS,
        },
        FramePhase::Scroll => Counters {
            total_ns: &FRAME_SCROLL_TOTAL_NS,
            max_ns: &FRAME_SCROLL_MAX_NS,
        },
        FramePhase::Dialogs => Counters {
            total_ns: &FRAME_DIALOGS_TOTAL_NS,
            max_ns: &FRAME_DIALOGS_MAX_NS,
        },
        FramePhase::Shortcuts => Counters {
            total_ns: &FRAME_SHORTCUTS_TOTAL_NS,
            max_ns: &FRAME_SHORTCUTS_MAX_NS,
        },
        FramePhase::Finish => Counters {
            total_ns: &FRAME_FINISH_TOTAL_NS,
            max_ns: &FRAME_FINISH_MAX_NS,
        },
    }
}

fn bucket_counts() -> [u64; FRAME_HISTOGRAM_BUCKETS] {
    let mut counts = [0; FRAME_HISTOGRAM_BUCKETS];
    for (index, bucket) in FRAME_TIME_BUCKET_COUNTS.iter().enumerate() {
        counts[index] = load_counter(bucket);
    }
    counts
}

fn bucket_index(elapsed_ns: u64) -> usize {
    ((elapsed_ns.saturating_sub(1) / FRAME_HISTOGRAM_BUCKET_WIDTH_NS) as usize)
        .min(FRAME_HISTOGRAM_BUCKETS - 1)
}
