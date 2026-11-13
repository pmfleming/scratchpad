use crate::app::services::background_io::{
    BackgroundIoRequest, BackgroundIoResult, ColdFileShellResult, LoadedPathResult,
};

/// The result applied in place of a request that could not be queued, so its
/// pending action still completes. Session-tab hydration runs synchronously
/// instead of failing. `PrepareText` has no fallback result: its caller retries
/// or cancels the preparation directly.
pub(super) fn unavailable_result(request: BackgroundIoRequest) -> Option<BackgroundIoResult> {
    fn unavailable<T>(service: &str) -> Result<T, String> {
        Err(format!("Background {service} unavailable."))
    }
    Some(match request {
        BackgroundIoRequest::PrepareText { .. } => return None,
        BackgroundIoRequest::ReopenRetained { request_id, .. } => {
            BackgroundIoResult::RetainedReopened {
                request_id,
                result: unavailable("document reopening"),
            }
        }
        BackgroundIoRequest::LoadPaths {
            request_id,
            requests,
            ..
        } => BackgroundIoResult::PathsLoaded {
            request_id,
            results: requests
                .iter()
                .map(|request| LoadedPathResult {
                    missing: false,
                    retained_tab: None,
                    path: request.path().clone(),
                    disk_state: None,
                    result: unavailable("file loader"),
                })
                .collect(),
            is_partial: false,
        },
        BackgroundIoRequest::BuildColdFileShells {
            request_id, paths, ..
        } => BackgroundIoResult::ColdFileShellsBuilt {
            request_id,
            shells: paths
                .into_iter()
                .map(|path| ColdFileShellResult {
                    path,
                    result: unavailable("cold-file shell builder"),
                })
                .collect(),
            selected_path: None,
        },
        BackgroundIoRequest::SavePath {
            request_id, path, ..
        } => BackgroundIoResult::PathSaved {
            request_id,
            path,
            disk_state: None,
            digest: None,
            result: unavailable("file saver"),
        },
        BackgroundIoRequest::RestoreSession { request_id, .. } => {
            BackgroundIoResult::SessionRestored {
                request_id,
                result: unavailable("session restore"),
            }
        }
        BackgroundIoRequest::HydrateSessionTab {
            request_id,
            session_store,
            tab_index,
            cold_session_tab,
        } => BackgroundIoResult::SessionTabHydrated {
            request_id,
            tab_index,
            result: session_store
                .restore_cold_session_tab(cold_session_tab)
                .map(|(tab, status)| (Box::new(tab), status))
                .map_err(|error| error.to_string()),
        },
        BackgroundIoRequest::PersistSession { request_id, .. } => {
            BackgroundIoResult::SessionPersisted {
                request_id,
                result: unavailable("session save"),
            }
        }
        BackgroundIoRequest::RefreshTextMetadata {
            request_id,
            buffer_id,
            revision,
            ..
        } => BackgroundIoResult::TextMetadataRefreshed {
            request_id,
            buffer_id,
            revision,
            result: unavailable("text metadata refresh"),
        },
        BackgroundIoRequest::RefreshEncodingCompliance {
            request_id,
            buffer_id,
            revision,
            ..
        } => BackgroundIoResult::EncodingComplianceRefreshed {
            request_id,
            buffer_id,
            revision,
            result: unavailable("encoding compliance refresh"),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::unavailable_result;
    use crate::app::domain::{TextDocument, TextFormatMetadata};
    use crate::app::services::background_io::{
        BackgroundIoRequest, BackgroundIoResult, PathLoadRequest,
    };
    use crate::app::services::session_store::SessionStore;
    use std::path::PathBuf;

    /// Request id and every error message carried by a fallback result.
    fn errors(result: BackgroundIoResult) -> (u64, Vec<String>) {
        let one = |result: Result<_, String>| result.err().into_iter().collect();
        match result {
            BackgroundIoResult::RetainedReopened { request_id, result } => {
                (request_id, one(result.map(drop)))
            }
            BackgroundIoResult::PathsLoaded {
                request_id,
                results,
                is_partial,
            } => {
                assert!(!is_partial);
                let errors = results.into_iter().filter_map(|r| r.result.err());
                (request_id, errors.collect())
            }
            BackgroundIoResult::ColdFileShellsBuilt {
                request_id, shells, ..
            } => (
                request_id,
                shells.into_iter().filter_map(|s| s.result.err()).collect(),
            ),
            BackgroundIoResult::PathSaved {
                request_id, result, ..
            }
            | BackgroundIoResult::SessionPersisted { request_id, result } => {
                (request_id, one(result))
            }
            BackgroundIoResult::SessionRestored { request_id, result } => {
                (request_id, one(result.map(drop)))
            }
            BackgroundIoResult::TextMetadataRefreshed {
                request_id,
                buffer_id,
                revision,
                result,
            } => {
                assert_eq!((buffer_id, revision), (7, 9));
                (request_id, one(result.map(drop)))
            }
            BackgroundIoResult::EncodingComplianceRefreshed {
                request_id,
                buffer_id,
                revision,
                result,
            } => {
                assert_eq!((buffer_id, revision), (7, 9));
                (request_id, one(result.map(drop)))
            }
            _ => panic!("unexpected fallback result"),
        }
    }

    #[test]
    fn unqueued_requests_complete_with_unavailable_errors() {
        let root = tempfile::tempdir().unwrap();
        let store = SessionStore::new(root.path().join("session"));
        let snapshot = || TextDocument::new("text".to_owned()).snapshot();
        let format = || TextFormatMetadata::utf8_for_new_file("text");
        let paths = vec![PathBuf::from("a.txt"), PathBuf::from("b.txt")];
        let cases = [
            (
                BackgroundIoRequest::LoadPaths {
                    request_id: 1,
                    requests: paths
                        .iter()
                        .cloned()
                        .map(PathLoadRequest::Standard)
                        .collect(),
                    streaming: true,
                },
                "file loader",
                2,
            ),
            (
                BackgroundIoRequest::BuildColdFileShells {
                    request_id: 2,
                    paths: paths.clone(),
                    session_store: store.clone(),
                },
                "cold-file shell builder",
                2,
            ),
            (
                BackgroundIoRequest::SavePath {
                    request_id: 3,
                    path: paths[0].clone(),
                    snapshot: snapshot(),
                    format: format(),
                },
                "file saver",
                1,
            ),
            (
                BackgroundIoRequest::RestoreSession {
                    request_id: 4,
                    session_store: store.clone(),
                },
                "session restore",
                1,
            ),
            (
                BackgroundIoRequest::RefreshTextMetadata {
                    request_id: 5,
                    buffer_id: 7,
                    revision: 9,
                    snapshot: snapshot(),
                    format: format(),
                },
                "text metadata refresh",
                1,
            ),
            (
                BackgroundIoRequest::RefreshEncodingCompliance {
                    request_id: 6,
                    buffer_id: 7,
                    revision: 9,
                    snapshot: snapshot(),
                    format: format(),
                },
                "encoding compliance refresh",
                1,
            ),
        ];
        for (request, service, count) in cases {
            let expected_id = match &request {
                BackgroundIoRequest::LoadPaths { request_id, .. }
                | BackgroundIoRequest::BuildColdFileShells { request_id, .. }
                | BackgroundIoRequest::SavePath { request_id, .. }
                | BackgroundIoRequest::RestoreSession { request_id, .. }
                | BackgroundIoRequest::RefreshTextMetadata { request_id, .. }
                | BackgroundIoRequest::RefreshEncodingCompliance { request_id, .. } => *request_id,
                _ => unreachable!(),
            };
            let (request_id, errors) = errors(unavailable_result(request).unwrap());
            assert_eq!(request_id, expected_id, "{service}");
            assert_eq!(
                errors,
                vec![format!("Background {service} unavailable."); count]
            );
        }
    }
}
