mod analysis_lane;
mod path_lane;
mod session_lane;

pub(super) use analysis_lane::spawn_analysis_lane;
pub(super) use path_lane::spawn_path_lane;
pub(super) use session_lane::spawn_session_lane;

use super::dispatcher::LaneDepths;
use super::types::{BackgroundIoRequest, BackgroundIoResult};
use crate::app::capacity_metrics::{self, BackgroundIoLane};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender};
use std::thread;
use std::time::Instant;

pub(super) struct LaneEndpoints {
    pub(super) request_rx: Receiver<BackgroundIoRequest>,
    pub(super) result_tx: Sender<BackgroundIoResult>,
    pub(super) lane_depths: Arc<LaneDepths>,
}

/// Outcome of a per-request handler.
pub(super) enum LaneOutcome {
    /// Send `result` over `result_tx`; if that fails, terminate the lane.
    Result(Box<BackgroundIoResult>),
    /// Handler emitted its own messages; continue if `false`, terminate if `true`.
    HandledWithSendFailure(bool),
    /// Skip this request (wrong lane) and keep running.
    Skip,
}

impl LaneOutcome {
    pub(super) fn result(result: BackgroundIoResult) -> Self {
        Self::Result(Box::new(result))
    }
}

pub(super) fn spawn_lane(
    lane: BackgroundIoLane,
    request_rx: Receiver<BackgroundIoRequest>,
    result_tx: Sender<BackgroundIoResult>,
    lane_depths: Arc<LaneDepths>,
    handle: impl Fn(BackgroundIoRequest, &Sender<BackgroundIoResult>) -> LaneOutcome + Send + 'static,
) {
    thread::spawn(move || {
        while let Ok(request) = request_rx.recv() {
            lane_depths.decrement(lane);
            let started_at = Instant::now();
            let send_failed = match handle(request, &result_tx) {
                LaneOutcome::Result(result) => result_tx.send(*result).is_err(),
                LaneOutcome::HandledWithSendFailure(failed) => failed,
                LaneOutcome::Skip => continue,
            };
            if send_failed {
                break;
            }
            capacity_metrics::record_background_io_lane(lane, started_at.elapsed());
        }
    });
}
