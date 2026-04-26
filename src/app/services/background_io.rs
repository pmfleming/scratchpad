use crate::app::domain::{
    BufferState, DiskFileState, DocumentSnapshot, TextArtifactSummary, TextFormatMetadata,
};
use crate::app::services::file_service::FileService;
use crate::app::services::session_store::{RestoredSession, SessionPersistRequest, SessionStore};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, SendError, Sender};
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
    RefreshTextMetadata {
        request_id: u64,
        buffer_id: u64,
        revision: u64,
        snapshot: DocumentSnapshot,
        format: TextFormatMetadata,
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
    TextMetadataRefreshed {
        request_id: u64,
        buffer_id: u64,
        revision: u64,
        result: Result<(usize, TextArtifactSummary, TextFormatMetadata), String>,
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

pub(crate) struct BackgroundIoDispatcher {
    path_tx: Sender<BackgroundIoRequest>,
    session_tx: Sender<BackgroundIoRequest>,
    analysis_tx: Sender<BackgroundIoRequest>,
}

impl BackgroundIoDispatcher {
    pub(crate) fn send(
        &self,
        request: BackgroundIoRequest,
    ) -> Result<(), SendError<BackgroundIoRequest>> {
        match request {
            request @ BackgroundIoRequest::LoadPaths { .. } => self.path_tx.send(request),
            request @ BackgroundIoRequest::RestoreSession { .. }
            | request @ BackgroundIoRequest::PersistSession { .. } => self.session_tx.send(request),
            request @ BackgroundIoRequest::RefreshTextMetadata { .. }
            | request @ BackgroundIoRequest::RefreshEncodingCompliance { .. } => {
                self.analysis_tx.send(request)
            }
        }
    }
}

pub(crate) fn spawn_background_io_worker() -> (BackgroundIoDispatcher, Receiver<BackgroundIoResult>)
{
    let (result_tx, result_rx) = mpsc::channel::<BackgroundIoResult>();
    let (path_tx, path_rx) = mpsc::channel::<BackgroundIoRequest>();
    let (session_tx, session_rx) = mpsc::channel::<BackgroundIoRequest>();
    let (analysis_tx, analysis_rx) = mpsc::channel::<BackgroundIoRequest>();

    spawn_path_lane(path_rx, result_tx.clone());
    spawn_session_lane(session_rx, result_tx.clone());
    spawn_analysis_lane(analysis_rx, result_tx);

    (
        BackgroundIoDispatcher {
            path_tx,
            session_tx,
            analysis_tx,
        },
        result_rx,
    )
}

fn spawn_path_lane(
    request_rx: Receiver<BackgroundIoRequest>,
    result_tx: Sender<BackgroundIoResult>,
) {
    thread::spawn(move || {
        while let Ok(request) = request_rx.recv() {
            let BackgroundIoRequest::LoadPaths {
                request_id,
                requests,
            } = request
            else {
                continue;
            };
            if result_tx
                .send(BackgroundIoResult::PathsLoaded {
                    request_id,
                    results: load_paths(requests),
                })
                .is_err()
            {
                break;
            }
        }
    });
}

fn spawn_session_lane(
    request_rx: Receiver<BackgroundIoRequest>,
    result_tx: Sender<BackgroundIoResult>,
) {
    thread::spawn(move || {
        while let Ok(request) = request_rx.recv() {
            let result = match request {
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
                BackgroundIoRequest::LoadPaths { .. }
                | BackgroundIoRequest::RefreshTextMetadata { .. }
                | BackgroundIoRequest::RefreshEncodingCompliance { .. } => continue,
            };

            if result_tx.send(result).is_err() {
                break;
            }
        }
    });
}

fn spawn_analysis_lane(
    request_rx: Receiver<BackgroundIoRequest>,
    result_tx: Sender<BackgroundIoResult>,
) {
    thread::spawn(move || {
        while let Ok(request) = request_rx.recv() {
            let result = match request {
                BackgroundIoRequest::RefreshTextMetadata {
                    request_id,
                    buffer_id,
                    revision,
                    snapshot,
                    format,
                } => BackgroundIoResult::TextMetadataRefreshed {
                    request_id,
                    buffer_id,
                    revision,
                    result: Ok(refresh_text_metadata(snapshot, format)),
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
                            .spans_for_range(0..snapshot.document_length().chars)
                            .map(|span| span.text),
                    )),
                },
                BackgroundIoRequest::LoadPaths { .. }
                | BackgroundIoRequest::RestoreSession { .. }
                | BackgroundIoRequest::PersistSession { .. } => continue,
            };

            if result_tx.send(result).is_err() {
                break;
            }
        }
    });
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

fn refresh_text_metadata(
    snapshot: DocumentSnapshot,
    mut format: TextFormatMetadata,
) -> (usize, TextArtifactSummary, TextFormatMetadata) {
    let metadata = crate::app::domain::buffer::buffer_text_metadata_from_piece_tree(
        snapshot.piece_tree(),
        &mut format,
    );
    (metadata.line_count, metadata.artifact_summary, format)
}
