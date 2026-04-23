use crate::app::domain::{BufferState, DiskFileState, DocumentSnapshot, TextFormatMetadata};
use crate::app::services::file_service::FileService;
use crate::app::services::session_store::{RestoredSession, SessionPersistRequest, SessionStore};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

pub(crate) enum PathLoadRequest {
    Standard(PathBuf),
    WithEncoding {
        path: PathBuf,
        encoding_name: String,
    },
}

impl PathLoadRequest {
    pub(crate) fn path(&self) -> &PathBuf {
        match self {
            Self::Standard(path) => path,
            Self::WithEncoding { path, .. } => path,
        }
    }
}

pub(crate) enum BackgroundIoRequest {
    LoadPaths {
        request_id: u64,
        requests: Vec<PathLoadRequest>,
    },
    RestoreSession {
        request_id: u64,
        session_store: SessionStore,
    },
    PersistSession {
        request_id: u64,
        session_store: SessionStore,
        request: SessionPersistRequest,
    },
    RefreshEncodingCompliance {
        request_id: u64,
        buffer_id: u64,
        revision: u64,
        snapshot: DocumentSnapshot,
        format: TextFormatMetadata,
    },
}

pub(crate) enum BackgroundIoResult {
    PathsLoaded {
        request_id: u64,
        results: Vec<LoadedPathResult>,
    },
    SessionRestored {
        request_id: u64,
        result: Result<Option<RestoredSession>, String>,
    },
    SessionPersisted {
        request_id: u64,
        result: Result<(), String>,
    },
    EncodingComplianceRefreshed {
        request_id: u64,
        buffer_id: u64,
        revision: u64,
        result: Result<bool, String>,
    },
}

pub(crate) struct LoadedPathResult {
    pub(crate) path: PathBuf,
    pub(crate) disk_state: Option<DiskFileState>,
    pub(crate) result: Result<BufferState, String>,
}

pub(crate) fn spawn_background_io_worker()
-> (Sender<BackgroundIoRequest>, Receiver<BackgroundIoResult>) {
    let (request_tx, request_rx) = mpsc::channel::<BackgroundIoRequest>();
    let (result_tx, result_rx) = mpsc::channel::<BackgroundIoResult>();
    thread::spawn(move || {
        while let Ok(request) = request_rx.recv() {
            let result = match request {
                BackgroundIoRequest::LoadPaths {
                    request_id,
                    requests,
                } => BackgroundIoResult::PathsLoaded {
                    request_id,
                    results: load_paths(requests),
                },
                BackgroundIoRequest::RestoreSession {
                    request_id,
                    session_store,
                } => BackgroundIoResult::SessionRestored {
                    request_id,
                    result: session_store.load().map_err(|error| error.to_string()),
                },
                BackgroundIoRequest::PersistSession {
                    request_id,
                    session_store,
                    request,
                } => BackgroundIoResult::SessionPersisted {
                    request_id,
                    result: session_store
                        .persist_request(request)
                        .map_err(|error| error.to_string()),
                },
                BackgroundIoRequest::RefreshEncodingCompliance {
                    request_id,
                    buffer_id,
                    revision,
                    snapshot,
                    format,
                } => BackgroundIoResult::EncodingComplianceRefreshed {
                    request_id,
                    buffer_id,
                    revision,
                    result: Ok(format.has_non_compliant_characters_spans(
                        snapshot
                            .piece_tree()
                            .spans_for_range(0..snapshot.len_chars())
                            .map(|span| span.text),
                    )),
                },
            };

            if result_tx.send(result).is_err() {
                break;
            }
        }
    });
    (request_tx, result_rx)
}

fn load_paths(requests: Vec<PathLoadRequest>) -> Vec<LoadedPathResult> {
    requests
        .into_iter()
        .map(|request| match request {
            PathLoadRequest::Standard(path) => {
                let disk_state = FileService::read_disk_state(&path).ok();
                let result = FileService::read_file(&path)
                    .map(|file_content| {
                        FileService::build_buffer_from_file_content(
                            &path,
                            file_content,
                            disk_state.clone(),
                        )
                    })
                    .map_err(|error| error.to_string());
                LoadedPathResult {
                    path,
                    disk_state,
                    result,
                }
            }
            PathLoadRequest::WithEncoding {
                path,
                encoding_name,
            } => {
                let disk_state = FileService::read_disk_state(&path).ok();
                let result = FileService::read_file_with_encoding(&path, &encoding_name)
                    .map(|file_content| {
                        FileService::build_buffer_from_file_content(
                            &path,
                            file_content,
                            disk_state.clone(),
                        )
                    })
                    .map_err(|error| error.to_string());
                LoadedPathResult {
                    path,
                    disk_state,
                    result,
                }
            }
        })
        .collect()
}
