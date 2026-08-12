use super::protocol::{BrokerResponse, LaunchRequest};
use super::transport;
use eframe::egui;
use interprocess::local_socket::traits::Listener;
use std::collections::{HashSet, VecDeque};
use std::fs::{File, OpenOptions};
use std::io;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant};

const INBOX_BOUND: usize = 64;
const RECENT_INVOCATION_LIMIT: usize = 256;
const FORWARD_TIMEOUT: Duration = Duration::from_secs(2);
const RETRY_INTERVAL: Duration = Duration::from_millis(25);

pub enum ElectionResult {
    Primary(PrimaryInstance),
    Forwarded,
    Rejected(String),
}

pub struct PrimaryInstance {
    endpoint: String,
    ownership: File,
    listener: Option<interprocess::local_socket::Listener>,
    launch_tx: mpsc::SyncSender<LaunchRequest>,
    launch_rx: Option<mpsc::Receiver<LaunchRequest>>,
    wake_context: Arc<Mutex<Option<egui::Context>>>,
    pending: Arc<AtomicBool>,
    shutdown: Arc<AtomicBool>,
}

pub struct BrokerInbox {
    endpoint: String,
    ownership: File,
    launch_rx: mpsc::Receiver<LaunchRequest>,
    pending: Arc<AtomicBool>,
    shutdown: Arc<AtomicBool>,
}

impl PrimaryInstance {
    pub fn elect(state_root: &Path, request: &LaunchRequest) -> io::Result<ElectionResult> {
        std::fs::create_dir_all(state_root)?;
        let ownership = open_lock_file(&state_root.join("instance.lock"))?;
        let endpoint = endpoint_name(state_root);
        match ownership.try_lock() {
            Ok(()) => Self::become_primary(endpoint, ownership).map(ElectionResult::Primary),
            Err(std::fs::TryLockError::WouldBlock) => forward_until_resolved(&endpoint, request),
            Err(std::fs::TryLockError::Error(error)) => Err(error),
        }
    }

    fn become_primary(endpoint: String, ownership: File) -> io::Result<Self> {
        let listener = transport::bind_listener(&endpoint)?;
        let (launch_tx, launch_rx) = mpsc::sync_channel(INBOX_BOUND);
        Ok(Self {
            endpoint,
            ownership,
            listener: Some(listener),
            launch_tx,
            launch_rx: Some(launch_rx),
            wake_context: Arc::new(Mutex::new(None)),
            pending: Arc::new(AtomicBool::new(false)),
            shutdown: Arc::new(AtomicBool::new(false)),
        })
    }

    pub fn start(mut self, context: &egui::Context) -> io::Result<BrokerInbox> {
        let listener = self.listener.take().ok_or_else(|| {
            io::Error::new(io::ErrorKind::AlreadyExists, "broker already started")
        })?;
        let launch_rx = self.launch_rx.take().ok_or_else(|| {
            io::Error::new(io::ErrorKind::AlreadyExists, "broker inbox already taken")
        })?;
        if let Ok(mut wake_context) = self.wake_context.lock() {
            *wake_context = Some(context.clone());
        }
        spawn_listener(
            listener,
            self.launch_tx.clone(),
            Arc::clone(&self.wake_context),
            Arc::clone(&self.pending),
            Arc::clone(&self.shutdown),
        )?;
        Ok(BrokerInbox {
            endpoint: self.endpoint.clone(),
            ownership: self.ownership.try_clone()?,
            launch_rx,
            pending: Arc::clone(&self.pending),
            shutdown: Arc::clone(&self.shutdown),
        })
    }
}

impl BrokerInbox {
    #[must_use]
    pub fn take_pending(&self) -> bool {
        self.pending.swap(false, Ordering::AcqRel)
    }

    pub fn try_recv(&self) -> Result<LaunchRequest, mpsc::TryRecvError> {
        self.launch_rx.try_recv()
    }
}

impl Drop for BrokerInbox {
    fn drop(&mut self) {
        let _ = &self.ownership;
        self.shutdown.store(true, Ordering::Release);
        let _ = transport::connect(&self.endpoint);
    }
}

fn endpoint_name(state_root: &Path) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    state_root.hash(&mut hasher);
    format!("scratchpad-single-instance-v1-{:016x}", hasher.finish())
}

fn open_lock_file(path: &Path) -> io::Result<File> {
    OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path)
}

fn forward_until_resolved(endpoint: &str, request: &LaunchRequest) -> io::Result<ElectionResult> {
    let deadline = Instant::now() + FORWARD_TIMEOUT;
    loop {
        match transport::connect(endpoint)
            .and_then(|mut stream| transport::send_request(&mut stream, request))
        {
            Ok(BrokerResponse::Accepted) => return Ok(ElectionResult::Forwarded),
            Ok(BrokerResponse::Rejected(reason)) => return Ok(ElectionResult::Rejected(reason)),
            Ok(BrokerResponse::Busy) => {
                return Ok(ElectionResult::Rejected(
                    "Scratchpad is busy processing launch requests. Try again shortly.".to_owned(),
                ));
            }
            Ok(BrokerResponse::UnsupportedProtocol) => {
                return Ok(ElectionResult::Rejected(
                    "The running Scratchpad instance uses an incompatible launch protocol."
                        .to_owned(),
                ));
            }
            Err(_) if Instant::now() < deadline => thread::sleep(RETRY_INTERVAL),
            Err(error) => return Err(error),
        }
    }
}

fn spawn_listener(
    listener: interprocess::local_socket::Listener,
    launch_tx: mpsc::SyncSender<LaunchRequest>,
    wake_context: Arc<Mutex<Option<egui::Context>>>,
    pending: Arc<AtomicBool>,
    shutdown: Arc<AtomicBool>,
) -> io::Result<()> {
    thread::Builder::new()
        .name("scratchpad-instance-broker".to_owned())
        .spawn(move || {
            let mut recent = RecentInvocations::default();
            while !shutdown.load(Ordering::Acquire) {
                let Ok(mut stream) = listener.accept() else {
                    continue;
                };
                if shutdown.load(Ordering::Acquire) {
                    break;
                }
                let response = handle_connection(
                    &mut stream,
                    &launch_tx,
                    &wake_context,
                    &pending,
                    &mut recent,
                );
                let _ = transport::send_response(&mut stream, &response);
            }
        })
        .map(|_| ())
}

fn handle_connection(
    stream: &mut interprocess::local_socket::Stream,
    launch_tx: &mpsc::SyncSender<LaunchRequest>,
    wake_context: &Arc<Mutex<Option<egui::Context>>>,
    pending: &Arc<AtomicBool>,
    recent: &mut RecentInvocations,
) -> BrokerResponse {
    let request = match transport::receive_request(stream) {
        Ok(request) => request,
        Err(error) if error.kind() == io::ErrorKind::Unsupported => {
            return BrokerResponse::UnsupportedProtocol;
        }
        Err(error) => return BrokerResponse::Rejected(error.to_string()),
    };
    if let Err(reason) = request.validate_for_existing_primary() {
        return BrokerResponse::Rejected(reason);
    }
    if recent.contains(request.invocation_id) {
        return BrokerResponse::Accepted;
    }
    match launch_tx.try_send(request.clone()) {
        Ok(()) => {
            recent.insert(request.invocation_id);
            pending.store(true, Ordering::Release);
            if let Ok(context) = wake_context.lock()
                && let Some(context) = context.as_ref()
            {
                context.request_repaint();
            }
            BrokerResponse::Accepted
        }
        Err(mpsc::TrySendError::Full(_)) => BrokerResponse::Busy,
        Err(mpsc::TrySendError::Disconnected(_)) => {
            BrokerResponse::Rejected("Scratchpad is shutting down.".to_owned())
        }
    }
}

#[derive(Default)]
struct RecentInvocations {
    ids: HashSet<u128>,
    order: VecDeque<u128>,
}

impl RecentInvocations {
    fn contains(&self, id: u128) -> bool {
        self.ids.contains(&id)
    }

    fn insert(&mut self, id: u128) {
        if !self.ids.insert(id) {
            return;
        }
        self.order.push_back(id);
        if self.order.len() > RECENT_INVOCATION_LIMIT
            && let Some(oldest) = self.order.pop_front()
        {
            self.ids.remove(&oldest);
        }
    }
}
