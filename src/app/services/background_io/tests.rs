use super::{BackgroundIoDispatcher, BackgroundIoRequest, LaneDepths};
use std::sync::Arc;
use std::sync::mpsc;

#[test]
fn dispatcher_returns_request_when_path_lane_is_full() {
    let (path_tx, _path_rx) = mpsc::sync_channel(1);
    let (session_tx, _session_rx) = mpsc::sync_channel(1);
    let (analysis_tx, _analysis_rx) = mpsc::sync_channel(1);
    let dispatcher = BackgroundIoDispatcher {
        path_tx,
        session_tx,
        analysis_tx,
        lane_depths: Arc::new(LaneDepths::default()),
    };

    assert!(
        dispatcher
            .send(BackgroundIoRequest::LoadPaths {
                request_id: 1,
                requests: Vec::new(),
                streaming: false,
            })
            .is_ok()
    );
    let error = dispatcher
        .send(BackgroundIoRequest::LoadPaths {
            request_id: 2,
            requests: Vec::new(),
            streaming: false,
        })
        .expect_err("second request should hit backpressure");

    match error.into_request() {
        BackgroundIoRequest::LoadPaths { request_id, .. } => assert_eq!(request_id, 2),
        _ => panic!("expected load request"),
    }
}

#[test]
fn dispatcher_records_lane_saturation_when_full() {
    use crate::app::capacity_metrics::{capacity_metrics_snapshot, reset_capacity_metrics};

    reset_capacity_metrics();
    let (path_tx, _path_rx) = mpsc::sync_channel(1);
    let (session_tx, _session_rx) = mpsc::sync_channel(1);
    let (analysis_tx, _analysis_rx) = mpsc::sync_channel(1);
    let dispatcher = BackgroundIoDispatcher {
        path_tx,
        session_tx,
        analysis_tx,
        lane_depths: Arc::new(LaneDepths::default()),
    };

    // First send fills the lane.
    let _ = dispatcher.send(BackgroundIoRequest::LoadPaths {
        request_id: 1,
        requests: Vec::new(),
        streaming: false,
    });
    // Second send should hit `Full` and bump the saturation counter.
    let _ = dispatcher.send(BackgroundIoRequest::LoadPaths {
        request_id: 2,
        requests: Vec::new(),
        streaming: false,
    });

    assert!(capacity_metrics_snapshot().background_io_path_saturation_count >= 1);
}

#[test]
fn parallel_load_paths_preserves_input_order() {
    use super::{PathLoadRequest, load_paths};
    use std::env;
    use std::fs;

    let dir = env::temp_dir().join("scratchpad_load_paths_order_test");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create temp dir");

    let mut paths = Vec::new();
    for i in 0..6 {
        let path = dir.join(format!("file_{i}.txt"));
        fs::write(&path, format!("contents {i}")).expect("write temp file");
        paths.push(PathLoadRequest::Standard(path));
    }

    let results = load_paths(paths);
    assert_eq!(results.len(), 6);
    for (i, result) in results.iter().enumerate() {
        assert!(
            result
                .path
                .file_name()
                .and_then(|name| name.to_str())
                .map(|name| name == format!("file_{i}.txt"))
                .unwrap_or(false),
            "result {i} preserved order: got {:?}",
            result.path
        );
    }

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn streaming_load_paths_emits_one_partial_per_path_in_order() {
    use super::stream_load_paths;
    use super::{BackgroundIoResult, PathLoadRequest};
    use std::env;
    use std::fs;
    use std::sync::mpsc;

    let dir = env::temp_dir().join("scratchpad_streaming_load_paths_test");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create temp dir");

    let mut paths = Vec::new();
    for i in 0..5 {
        let path = dir.join(format!("stream_{i}.txt"));
        fs::write(&path, format!("contents {i}")).expect("write temp file");
        paths.push(PathLoadRequest::Standard(path));
    }

    let (tx, rx) = mpsc::channel();
    let send_failed = stream_load_paths(42, paths, &tx);
    assert!(!send_failed);
    drop(tx);

    let messages: Vec<BackgroundIoResult> = rx.iter().collect();
    assert_eq!(messages.len(), 5);

    for (i, message) in messages.iter().enumerate() {
        let BackgroundIoResult::PathsLoaded {
            request_id,
            results,
            is_partial,
        } = message
        else {
            panic!("expected PathsLoaded");
        };
        assert_eq!(*request_id, 42);
        assert_eq!(results.len(), 1);
        assert_eq!(*is_partial, i + 1 < 5, "is_partial for index {i}");
        let name = results[0]
            .path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        assert_eq!(name, format!("stream_{i}.txt"));
    }

    let _ = fs::remove_dir_all(&dir);
}
