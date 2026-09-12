use super::{
    BackgroundIoLane, CapacityMetricsSnapshot, FramePhase, capacity_metrics_snapshot,
    record_background_io_lane, record_background_io_queue_depth, record_background_io_saturation,
    record_frame, record_frame_phase, reset_capacity_metrics,
};
use std::{sync::Mutex, time::Duration};

static CAPACITY_METRICS_TEST_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn snapshot_groups_background_io_lane_metrics() {
    let _guard = CAPACITY_METRICS_TEST_LOCK.lock().unwrap();
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
    let _guard = CAPACITY_METRICS_TEST_LOCK.lock().unwrap();
    reset_capacity_metrics();
    record_frame_phase(FramePhase::Prepare, Duration::from_millis(10));
    record_frame_phase(FramePhase::Prepare, Duration::from_millis(3));

    let phase = capacity_metrics_snapshot().frame_phase(FramePhase::Prepare);
    assert_eq!(phase.total_ns, 13_000_000);
    assert_eq!(phase.max_ns, 10_000_000);
}

#[test]
fn histogram_overflow_does_not_clip_long_stalls() {
    let mut snapshot = CapacityMetricsSnapshot::default();
    snapshot.frame_count = 100;
    snapshot.frame_time_bucket_width_ns = 500_000;
    snapshot.frame_time_max_ns = 250_000_000;
    snapshot.frame_time_bucket_counts[0] = 95;
    snapshot.frame_time_bucket_counts[31] = 5;
    assert_eq!(snapshot.frame_time_percentile_ns(0.50), 500_000.0);
    assert_eq!(snapshot.frame_time_percentile_ns(0.99), 250_000_000.0);
}

#[test]
fn snapshot_reports_frame_time_summary_metrics() {
    let _guard = CAPACITY_METRICS_TEST_LOCK.lock().unwrap();
    reset_capacity_metrics();
    record_frame(Duration::from_millis(1));
    record_frame(Duration::from_millis(3));

    let snapshot = capacity_metrics_snapshot();
    assert_eq!(snapshot.frame_time_mean_ns(), 2_000_000.0);
    assert_eq!(snapshot.frame_time_percentile_ns(0.50), 1_000_000.0);
    assert_eq!(snapshot.frame_time_percentile_ns(0.95), 3_000_000.0);
    assert_eq!(
        CapacityMetricsSnapshot::default().frame_time_percentile_ns(0.95),
        0.0
    );
}
