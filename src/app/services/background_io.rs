use crate::app::domain::DiskFileState;
use crate::app::services::file_service::{FileContent, FileService};
use crate::app::services::session_store::{RestoredSession, SessionStore};
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
}

pub(crate) struct LoadedPathResult {
    pub(crate) path: PathBuf,
    pub(crate) disk_state: Option<DiskFileState>,
    pub(crate) result: Result<FileContent, String>,
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
            PathLoadRequest::Standard(path) => LoadedPathResult {
                disk_state: FileService::read_disk_state(&path).ok(),
                result: FileService::read_file(&path).map_err(|error| error.to_string()),
                path,
            },
            PathLoadRequest::WithEncoding {
                path,
                encoding_name,
            } => LoadedPathResult {
                disk_state: FileService::read_disk_state(&path).ok(),
                result: FileService::read_file_with_encoding(&path, &encoding_name)
                    .map_err(|error| error.to_string()),
                path,
            },
        })
        .collect()
}
