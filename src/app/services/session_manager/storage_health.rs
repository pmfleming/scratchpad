use super::{ScratchpadApp, StatusDomain};
use crate::app::domain::buffer::SnapshotRevision;
use crate::app::domain::{BufferState, WorkspaceTab};
use crate::app::services::chunk_store::{ChunkStoreHandle, Health};
use crate::app::services::session_store::CapturedClosedTab;
use eframe::egui::{Event, ImeEvent};

const HEALTHY: u8 = 0;
const UNAVAILABLE: u8 = 2;
const REFUSED: u8 = 3;

/// The buffer (id and revision) a refusal was raised for, and the message shown.
type Refusal = (Option<(u64, SnapshotRevision)>, String);

#[derive(Default)]
pub(crate) struct StorageStatus {
    /// Last announced `(health kind, pressure)`.
    last: (u8, u8),
    last_fault: u64,
    refusal: Option<Refusal>,
    new_refusal: bool,
    pub warning: Option<StorageWarning>,
}
pub(crate) struct StorageWarning {
    pub message: String,
    pub detail: String,
    pub red: bool,
    pub unavailable: bool,
}

fn health_kind(health: &Health) -> u8 {
    match health {
        Health::Healthy => HEALTHY,
        Health::Failing(_) => 1,
        Health::Unavailable(_) => UNAVAILABLE,
    }
}

pub(crate) fn poll_storage_health(app: &mut ScratchpadApp) {
    let health = app.state.persistence.session_store.storage_health();
    let kind = health_kind(&health);
    let previous = app.state.persistence.storage.last;
    let failure_ram = app.state.app_settings.history.failure_ram.bytes as usize;
    let store = app.state.persistence.session_store.opened_chunk_store();
    if let Some(store) = &store {
        store.set_failure_limit(failure_ram);
    }
    if kind == HEALTHY && previous.0 == HEALTHY {
        refresh_read_fault_attention(app, store.as_ref());
        return;
    }
    let tails = count_ram_and_attention(app, store.as_ref(), kind, previous.0);
    let (used, limit) = store.as_ref().map_or((tails, failure_ram), |store| {
        store.reap_abandoned();
        store.set_other_ram_bytes(tails);
        (store.ram_only_bytes(), store.failure_limit())
    });
    let refusal = app.state.persistence.storage.refusal.as_ref();
    let pressure = pressure_level(refusal.is_some(), used, limit);
    let warning = storage_warning(&health, refusal, pressure, used, limit);
    let key = (kind, pressure);
    if kind == HEALTHY {
        app.state.persistence.storage.refusal = None;
        app.state
            .status
            .set_info_status_in_domain(StatusDomain::Session, "Session saving has resumed");
    } else if key != previous
        && let Some(warning) = &warning
    {
        app.state.status.set_warning_status_with_detail(
            StatusDomain::Session,
            warning.message.clone(),
            warning.detail.clone(),
        );
    }
    app.state.persistence.storage.last = key;
    app.state.persistence.storage.warning = warning;
}

/// While storage stays healthy, only buffers with read faults need attention:
/// every tab after a new fault epoch, otherwise just the active tab. Healthy
/// stores do not scan every tab or count evictable text on each frame.
fn refresh_read_fault_attention(app: &mut ScratchpadApp, store: Option<&ChunkStoreHandle>) {
    let fault = store.map_or(0, ChunkStoreHandle::fault_epoch);
    if fault != app.state.persistence.storage.last_fault {
        app.state.persistence.storage.last_fault = fault;
        let buffers = app.tab_manager.tabs.as_mut_slice().iter_mut();
        clear_read_fault_attention(buffers.flat_map(|tab| tab.buffers_mut()));
    }
    if let Some(tab) = app.tab_manager.active_tab_mut() {
        clear_read_fault_attention(tab.buffers_mut());
    }
}

fn clear_read_fault_attention<'a>(buffers: impl Iterator<Item = &'a mut BufferState>) {
    for buffer in buffers {
        if buffer.document().piece_tree().read_fault().is_some() {
            buffer.update_storage_attention(false);
        }
    }
}

/// 0 = comfortable, 1 = at least 80% of the limit, 2 = at the limit, 3 = a refusal is shown.
fn pressure_level(refused: bool, used: usize, limit: usize) -> u8 {
    if refused {
        REFUSED
    } else if used >= limit {
        2
    } else {
        u8::from(used >= limit.saturating_mul(4) / 5)
    }
}

fn storage_warning(
    health: &Health,
    refusal: Option<&Refusal>,
    pressure: u8,
    used: usize,
    limit: usize,
) -> Option<StorageWarning> {
    let (Health::Failing(cause) | Health::Unavailable(cause)) = health else {
        return None;
    };
    let message = refusal.map_or_else(
        || match pressure {
            2 => "Session not saving. Memory limit reached.".to_owned(),
            1 => "Session not saving. Memory nearly full.".to_owned(),
            _ => "Session not saving. Text is in memory only.".to_owned(),
        },
        |(_, cause)| cause.clone(),
    );
    Some(StorageWarning {
        message,
        red: pressure >= 2,
        unavailable: health_kind(health) == UNAVAILABLE,
        detail: format!(
            "{cause}\nRAM-only text: {used} bytes; limit: {limit} bytes.\nText held only in memory will be lost on a crash or exit unless saved to a file. Large pastes and file opens are refused at the limit. Save to files, free disk space, or raise the limit in Settings."
        ),
    })
}

/// Binds every buffer to the store, refreshes its storage attention, collects
/// new refusals and returns the RAM-only bytes held outside the chunk store.
fn count_ram_and_attention(
    app: &mut ScratchpadApp,
    store: Option<&ChunkStoreHandle>,
    kind: u8,
    previous: u8,
) -> usize {
    let persistence = &mut app.state.persistence;
    let mut tails = persistence
        .pending_closed_tabs
        .iter()
        .map(CapturedClosedTab::ram_tail_bytes)
        .sum::<usize>();
    let mut new_refusal = std::mem::take(&mut persistence.storage.new_refusal);
    for tab in app.tab_manager.tabs.as_mut_slice() {
        for buffer in tab.buffers_mut() {
            if let Some(store) = store {
                let _ = buffer.document_mut().bind_store(store.clone());
            }
            tails = tails.saturating_add(buffer.document().piece_tree().ram_tail_bytes());
            if let Some(cause) = buffer.storage_refusal.take() {
                new_refusal = true;
                persistence.storage.refusal =
                    Some((Some((buffer.id, buffer.storage_revision())), cause));
            }
            buffer.update_storage_attention(kind != HEALTHY);
        }
        for view in tab.layout.views() {
            let deferred = view.deferred_editor_events.iter().map(deferred_text_bytes);
            tails = tails.saturating_add(deferred.sum());
        }
    }
    if let Some((owner, _)) = &persistence.storage.refusal {
        let unchanged = refusal_owner_unchanged(app.tab_manager.tabs.as_slice(), owner.as_ref());
        if !unchanged || kind == HEALTHY || (kind != previous && !new_refusal) {
            app.state.persistence.storage.refusal = None;
        }
    }
    tails
}

fn deferred_text_bytes(event: &Event) -> usize {
    match event {
        Event::Text(text) | Event::Paste(text) | Event::Ime(ImeEvent::Commit(text)) => text.len(),
        _ => 0,
    }
}

/// A refusal stays relevant while its buffer is unchanged (or, for an
/// ownerless refusal, while no tabs are open).
fn refusal_owner_unchanged(tabs: &[WorkspaceTab], owner: Option<&(u64, SnapshotRevision)>) -> bool {
    owner.map_or_else(
        || tabs.is_empty(),
        |(id, revision)| {
            tabs.iter()
                .flat_map(|tab| tab.buffers())
                .find(|buffer| buffer.id == *id)
                .is_some_and(|buffer| buffer.storage_revision() == *revision)
        },
    )
}

pub(crate) fn note_file_refusal(app: &mut ScratchpadApp, path: &std::path::Path, cause: &str) {
    if app.state.persistence.session_store.storage_health() == Health::Healthy
        || !(cause.contains("limit") || cause.contains("memory"))
    {
        return;
    }
    let owner = app.tab_manager.active_tab().map(|tab| {
        let buffer = tab.active_buffer();
        (buffer.id, buffer.storage_revision())
    });
    app.state.persistence.storage.refusal = Some((
        owner,
        format!("Could not open {}. Memory limit reached.", path.display()),
    ));
    app.state.persistence.storage.new_refusal = true;
}
