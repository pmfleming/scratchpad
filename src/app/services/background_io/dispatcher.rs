use super::types::{BackgroundIoRequest, BackgroundIoResult};
use super::worker::{LaneEndpoints, spawn_analysis_lane, spawn_path_lane, spawn_session_lane};
use crate::app::capacity_metrics::{self, BackgroundIoLane};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};

const PATH_LANE_QUEUE_BOUND: usize = 8;
const SESSION_LANE_QUEUE_BOUND: usize = 2;
const ANALYSIS_LANE_QUEUE_BOUND: usize = 16;

pub(crate) struct BackgroundIoDispatcher {
    path_tx: SyncSender<BackgroundIoRequest>,
    session_tx: SyncSender<BackgroundIoRequest>,
    analysis_tx: SyncSender<BackgroundIoRequest>,
    lane_depths: Arc<LaneDepths>,
}

#[derive(Default)]
pub(super) struct LaneDepths {
    path: AtomicU64,
    session: AtomicU64,
    analysis: AtomicU64,
}

impl LaneDepths {
    fn counter(&self, lane: BackgroundIoLane) -> &AtomicU64 {
        match lane {
            BackgroundIoLane::Path => &self.path,
            BackgroundIoLane::Session => &self.session,
            BackgroundIoLane::Analysis => &self.analysis,
        }
    }

    pub(super) fn increment(&self, lane: BackgroundIoLane) {
        let depth = self.counter(lane).fetch_add(1, Ordering::Relaxed) + 1;
        capacity_metrics::record_background_io_queue_depth(lane, depth as usize);
    }

    pub(super) fn decrement(&self, lane: BackgroundIoLane) {
        self.counter(lane).fetch_sub(1, Ordering::Relaxed);
    }
}

pub(crate) struct BackgroundIoSendError {
    request: Box<BackgroundIoRequest>,
    lane: BackgroundIoLane,
    reason: &'static str,
    request_kind: &'static str,
}

impl BackgroundIoSendError {
    fn from_try_send_error(
        error: TrySendError<BackgroundIoRequest>,
        lane: BackgroundIoLane,
    ) -> Self {
        let (reason, request) = match error {
            TrySendError::Full(request) => ("full", request),
            TrySendError::Disconnected(request) => ("disconnected", request),
        };
        let request_kind = request.kind();
        Self {
            request: Box::new(request),
            lane,
            reason,
            request_kind,
        }
    }

    pub(crate) fn lane_name(&self) -> &'static str {
        lane_name(self.lane)
    }

    pub(crate) fn reason(&self) -> &'static str {
        self.reason
    }

    pub(crate) fn request_kind(&self) -> &'static str {
        self.request_kind
    }

    pub(crate) fn into_request(self) -> BackgroundIoRequest {
        *self.request
    }
}

impl BackgroundIoDispatcher {
    pub(crate) fn send(&self, request: BackgroundIoRequest) -> Result<(), BackgroundIoSendError> {
        // Increment BEFORE try_send so the receiving worker can never observe
        // a decrement before the corresponding increment (and underflow the
        // counter into u64::MAX). Roll back on failure.
        let lane = request.lane();
        let tx = match lane {
            BackgroundIoLane::Path => &self.path_tx,
            BackgroundIoLane::Session => &self.session_tx,
            BackgroundIoLane::Analysis => &self.analysis_tx,
        };
        self.lane_depths.increment(lane);
        match tx.try_send(request) {
            Ok(()) => Ok(()),
            Err(error) => {
                self.lane_depths.decrement(lane);
                if matches!(error, TrySendError::Full(_)) {
                    capacity_metrics::record_background_io_saturation(lane);
                }
                Err(BackgroundIoSendError::from_try_send_error(error, lane))
            }
        }
    }
}

pub(crate) fn spawn_background_io_worker() -> (BackgroundIoDispatcher, Receiver<BackgroundIoResult>)
{
    let (result_tx, result_rx) = mpsc::channel::<BackgroundIoResult>();
    let (path_tx, path_rx) = mpsc::sync_channel::<BackgroundIoRequest>(PATH_LANE_QUEUE_BOUND);
    let (session_tx, session_rx) =
        mpsc::sync_channel::<BackgroundIoRequest>(SESSION_LANE_QUEUE_BOUND);
    let (analysis_tx, analysis_rx) =
        mpsc::sync_channel::<BackgroundIoRequest>(ANALYSIS_LANE_QUEUE_BOUND);

    let lane_depths = Arc::new(LaneDepths::default());
    spawn_path_lane(LaneEndpoints {
        request_rx: path_rx,
        result_tx: result_tx.clone(),
        lane_depths: Arc::clone(&lane_depths),
    });
    spawn_session_lane(LaneEndpoints {
        request_rx: session_rx,
        result_tx: result_tx.clone(),
        lane_depths: Arc::clone(&lane_depths),
    });
    spawn_analysis_lane(LaneEndpoints {
        request_rx: analysis_rx,
        result_tx,
        lane_depths: Arc::clone(&lane_depths),
    });

    (
        BackgroundIoDispatcher {
            path_tx,
            session_tx,
            analysis_tx,
            lane_depths,
        },
        result_rx,
    )
}

fn lane_name(lane: BackgroundIoLane) -> &'static str {
    match lane {
        BackgroundIoLane::Path => "path",
        BackgroundIoLane::Session => "session",
        BackgroundIoLane::Analysis => "analysis",
    }
}

#[cfg(test)]
mod tests {
    use super::{BackgroundIoDispatcher, BackgroundIoRequest, LaneDepths};
    use crate::app::domain::{TextDocument, TextFormatMetadata};
    use crate::app::services::session_store::SessionStore;
    use std::sync::mpsc::Receiver;
    use std::sync::{Arc, mpsc};

    fn dispatcher_with_bounds(
        path_bound: usize,
        session_bound: usize,
        analysis_bound: usize,
    ) -> (
        BackgroundIoDispatcher,
        Receiver<BackgroundIoRequest>,
        Receiver<BackgroundIoRequest>,
        Receiver<BackgroundIoRequest>,
    ) {
        let (path_tx, path_rx) = mpsc::sync_channel(path_bound);
        let (session_tx, session_rx) = mpsc::sync_channel(session_bound);
        let (analysis_tx, analysis_rx) = mpsc::sync_channel(analysis_bound);
        (
            BackgroundIoDispatcher {
                path_tx,
                session_tx,
                analysis_tx,
                lane_depths: Arc::new(LaneDepths::default()),
            },
            path_rx,
            session_rx,
            analysis_rx,
        )
    }

    #[test]
    fn send_reports_full_lane_without_losing_request() {
        let (dispatcher, _path_rx, _session_rx, _analysis_rx) = dispatcher_with_bounds(0, 1, 1);
        let request = BackgroundIoRequest::LoadPaths {
            request_id: 7,
            requests: Vec::new(),
            streaming: false,
        };

        let error = dispatcher.send(request).unwrap_err();

        assert_eq!(error.lane_name(), "path");
        assert_eq!(error.reason(), "full");
        assert_eq!(error.request_kind(), "load_paths");
        assert!(matches!(
            error.into_request(),
            BackgroundIoRequest::LoadPaths { request_id: 7, .. }
        ));
    }

    #[test]
    fn send_reports_disconnected_lane_without_losing_request() {
        let directory = tempfile::tempdir().unwrap();
        let (dispatcher, _path_rx, session_rx, _analysis_rx) = dispatcher_with_bounds(1, 1, 1);
        drop(session_rx);
        let request = BackgroundIoRequest::RestoreSession {
            request_id: 9,
            session_store: SessionStore::new(directory.path().to_path_buf()),
        };

        let error = dispatcher.send(request).unwrap_err();

        assert_eq!(error.lane_name(), "session");
        assert_eq!(error.reason(), "disconnected");
        assert_eq!(error.request_kind(), "restore_session");
        assert!(matches!(
            error.into_request(),
            BackgroundIoRequest::RestoreSession { request_id: 9, .. }
        ));
    }

    #[test]
    fn analysis_requests_route_to_analysis_lane() {
        let (dispatcher, path_rx, session_rx, analysis_rx) = dispatcher_with_bounds(1, 1, 1);
        let document = TextDocument::new("alpha".to_owned());
        let snapshot = document.snapshot();
        let format = TextFormatMetadata::utf8_for_new_file("alpha");

        assert!(
            dispatcher
                .send(BackgroundIoRequest::RefreshTextMetadata {
                    request_id: 11,
                    buffer_id: 3,
                    revision: 4,
                    snapshot,
                    format,
                })
                .is_ok()
        );

        assert!(path_rx.try_recv().is_err());
        assert!(session_rx.try_recv().is_err());
        assert!(matches!(
            analysis_rx.try_recv().unwrap(),
            BackgroundIoRequest::RefreshTextMetadata {
                request_id: 11,
                buffer_id: 3,
                revision: 4,
                ..
            }
        ));
    }
}
