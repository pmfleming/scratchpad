use super::{LaneEndpoints, LaneOutcome, spawn_lane};
use crate::app::capacity_metrics::BackgroundIoLane;
use crate::app::services::background_io::types::{BackgroundIoRequest, BackgroundIoResult};
use crate::app::services::session_store::{ColdSessionTab, SessionStore};
use std::sync::mpsc::Sender;

pub(in crate::app::services::background_io) fn spawn_session_lane(endpoints: LaneEndpoints) {
    spawn_lane(
        BackgroundIoLane::Session,
        endpoints.request_rx,
        endpoints.result_tx,
        endpoints.lane_depths,
        |request, result_tx| match request {
            BackgroundIoRequest::RestoreSession {
                request_id,
                session_store,
            } => LaneOutcome::HandledWithSendFailure(stream_restore_session(
                request_id,
                session_store,
                result_tx,
            )),
            BackgroundIoRequest::HydrateSessionTab {
                request_id,
                session_store,
                tab_index,
                cold_session_tab,
            } => LaneOutcome::result(hydrate_session_tab(
                request_id,
                session_store,
                tab_index,
                cold_session_tab,
            )),
            BackgroundIoRequest::PersistSession {
                request_id,
                session_store,
                request,
            } => LaneOutcome::result(BackgroundIoResult::SessionPersisted {
                request_id,
                result: session_store
                    .persist_request(request)
                    .map_err(|error| error.to_string()),
            }),
            _ => LaneOutcome::Skip,
        },
    );
}

fn stream_restore_session(
    request_id: u64,
    session_store: SessionStore,
    result_tx: &Sender<BackgroundIoResult>,
) -> bool {
    let send_failed = std::cell::Cell::new(false);
    let result = session_store.load_streaming(
        |active_tab_index, active_surface, legacy_settings| {
            if result_tx
                .send(BackgroundIoResult::SessionRestoreStarted {
                    request_id,
                    active_tab_index,
                    active_surface,
                    legacy_settings,
                })
                .is_err()
            {
                send_failed.set(true);
                return false;
            }
            true
        },
        |tab_index, tab, cold_session_tab| {
            if result_tx
                .send(BackgroundIoResult::SessionTabRestored {
                    request_id,
                    tab_index,
                    cold_session_tab,
                    tab: Box::new(tab),
                })
                .is_err()
            {
                send_failed.set(true);
                return false;
            }
            true
        },
    );

    if send_failed.get() {
        return true;
    }

    result_tx
        .send(BackgroundIoResult::SessionRestored {
            request_id,
            result: result.map_err(|error| error.to_string()),
        })
        .is_err()
}

fn hydrate_session_tab(
    request_id: u64,
    session_store: SessionStore,
    tab_index: usize,
    cold_session_tab: ColdSessionTab,
) -> BackgroundIoResult {
    let (tab, restore_status) = session_store.restore_cold_session_tab(cold_session_tab);
    BackgroundIoResult::SessionTabHydrated {
        request_id,
        tab_index,
        restore_status,
        tab: Box::new(tab),
    }
}
