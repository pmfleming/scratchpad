use crate::app::services::background_io::{
    BackgroundIoRequest, BackgroundIoResult, LoadedPathResult,
};
use std::path::PathBuf;

pub(super) trait BackgroundIoFallback {
    fn into_loaded_path_results(self) -> Option<Vec<LoadedPathResult>>;
    fn into_path_saved_result(self) -> BackgroundIoResult;
    fn into_restore_result(
        self,
    ) -> Result<Option<crate::app::services::session_store::RestoredSession>, String>;
    fn into_hydrated_session_tab_result(self) -> BackgroundIoResult;
    fn into_persist_result(self) -> Result<(), String>;
    fn into_text_metadata_result(
        self,
    ) -> Result<
        (
            crate::app::domain::buffer::BufferLength,
            usize,
            crate::app::domain::TextArtifactSummary,
            crate::app::domain::TextFormatMetadata,
        ),
        String,
    >;
    fn into_encoding_compliance_result(self) -> Result<bool, String>;
}

impl BackgroundIoFallback for BackgroundIoRequest {
    fn into_loaded_path_results(self) -> Option<Vec<LoadedPathResult>> {
        match self {
            BackgroundIoRequest::LoadPaths { requests, .. } => Some(
                requests
                    .into_iter()
                    .map(|request| LoadedPathResult {
                        path: request.path().clone(),
                        disk_state: None,
                        result: Err("Background file loader unavailable.".to_owned()),
                    })
                    .collect(),
            ),
            _ => None,
        }
    }

    fn into_path_saved_result(self) -> BackgroundIoResult {
        match self {
            BackgroundIoRequest::SavePath {
                request_id, path, ..
            } => BackgroundIoResult::PathSaved {
                request_id,
                path,
                disk_state: None,
                result: Err("Background file saver unavailable.".to_owned()),
            },
            _ => BackgroundIoResult::PathSaved {
                request_id: 0,
                path: PathBuf::new(),
                disk_state: None,
                result: Err("Background file saver unavailable.".to_owned()),
            },
        }
    }

    fn into_restore_result(
        self,
    ) -> Result<Option<crate::app::services::session_store::RestoredSession>, String> {
        match self {
            BackgroundIoRequest::RestoreSession { .. } => {
                Err("Background session restore unavailable.".to_owned())
            }
            _ => Ok(None),
        }
    }

    fn into_hydrated_session_tab_result(self) -> BackgroundIoResult {
        match self {
            BackgroundIoRequest::HydrateSessionTab {
                request_id,
                session_store,
                tab_index,
                cold_session_tab,
            } => {
                let (tab, restore_status) =
                    session_store.restore_cold_session_tab(cold_session_tab);
                BackgroundIoResult::SessionTabHydrated {
                    request_id,
                    tab_index,
                    restore_status,
                    tab: Box::new(tab),
                }
            }
            _ => BackgroundIoResult::SessionTabHydrated {
                request_id: 0,
                tab_index: 0,
                restore_status: None,
                tab: Box::new(crate::app::domain::WorkspaceTab::untitled()),
            },
        }
    }

    fn into_persist_result(self) -> Result<(), String> {
        match self {
            BackgroundIoRequest::PersistSession { .. } => {
                Err("Background session save unavailable.".to_owned())
            }
            _ => Ok(()),
        }
    }

    fn into_text_metadata_result(
        self,
    ) -> Result<
        (
            crate::app::domain::buffer::BufferLength,
            usize,
            crate::app::domain::TextArtifactSummary,
            crate::app::domain::TextFormatMetadata,
        ),
        String,
    > {
        match self {
            BackgroundIoRequest::RefreshTextMetadata { .. } => {
                Err("Background text metadata refresh unavailable.".to_owned())
            }
            _ => Err("Background I/O channel unavailable.".to_owned()),
        }
    }

    fn into_encoding_compliance_result(self) -> Result<bool, String> {
        match self {
            BackgroundIoRequest::RefreshEncodingCompliance { .. } => {
                Err("Background encoding compliance refresh unavailable.".to_owned())
            }
            _ => Ok(false),
        }
    }
}
