use super::{ScratchpadApp, StatusDomain};
use crate::app::domain::buffer::SnapshotRevision;
use crate::app::services::chunk_store::Health;

#[derive(Default)]
pub(crate) struct StorageStatus {
    last: (u8, u8),
    last_fault: u64,
    refusal: Option<(Option<(u64, SnapshotRevision)>, String)>,
    new_refusal: bool,
    pub warning: Option<StorageWarning>,
}
pub(crate) struct StorageWarning {
    pub message: String,
    pub detail: String,
    pub red: bool,
    pub unavailable: bool,
}
pub(crate) fn poll_storage_health(app: &mut ScratchpadApp) {
    let health = app.state.persistence.session_store.storage_health();
    let kind = match health {
        Health::Healthy => 0,
        Health::Failing(_) => 1,
        Health::Unavailable(_) => 2,
    };
    let previous = app.state.persistence.storage.last;
    let store = app.state.persistence.session_store.opened_chunk_store();
    if let Some(store) = &store {
        store.set_failure_limit(app.state.app_settings.history.failure_ram.bytes as usize);
    }
    if kind == 0 && previous.0 == 0 {
        let fault = store.as_ref().map_or(0, |store| store.fault_epoch());
        if fault != app.state.persistence.storage.last_fault {
            app.state.persistence.storage.last_fault = fault;
            for tab in app.tab_manager.tabs.as_mut_slice() {
                for buffer in tab.buffers_mut() {
                    if buffer.document().piece_tree().read_fault().is_some() {
                        buffer.update_storage_attention(false);
                    }
                }
            }
        }
        // Read faults are discovered by the active editor. Healthy stores do not
        // scan every tab or count evictable text on each frame.
        if let Some(tab) = app.tab_manager.active_tab_mut() {
            for buffer in tab.buffers_mut() {
                if buffer.document().piece_tree().read_fault().is_some() {
                    buffer.update_storage_attention(false);
                }
            }
        }
        return;
    }
    let tails = count_ram_and_attention(app, store.as_ref(), kind, previous.0);
    let (used, limit) = store.as_ref().map_or(
        (
            tails,
            app.state.app_settings.history.failure_ram.bytes as usize,
        ),
        |store| {
            store.reap_abandoned();
            store.set_other_ram_bytes(tails);
            (store.ram_only_bytes(), store.failure_limit())
        },
    );
    let pressure = if app.state.persistence.storage.refusal.is_some() {
        3
    } else if used >= limit {
        2
    } else {
        u8::from(used >= limit.saturating_mul(4) / 5)
    };
    let warning = if kind == 0 {
        None
    } else {
        let cause = match &health {
            Health::Failing(cause) | Health::Unavailable(cause) => cause.as_str(),
            Health::Healthy => "",
        };
        let message = app.state.persistence.storage.refusal.as_ref().map_or_else(
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
            unavailable: kind == 2,
            detail: format!(
                "{cause}\nRAM-only text: {used} bytes; limit: {limit} bytes.\nText held only in memory will be lost on a crash or exit unless saved to a file. Large pastes and file opens are refused at the limit. Save to files, free disk space, or raise the limit in Settings."
            ),
        })
    };
    let key = (kind, pressure);
    if kind == 0 {
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

fn count_ram_and_attention(
    app: &mut ScratchpadApp,
    store: Option<&crate::app::services::chunk_store::ChunkStoreHandle>,
    kind: u8,
    previous: u8,
) -> usize {
    let mut tails = app
        .state
        .persistence
        .pending_closed_tabs
        .iter()
        .map(crate::app::services::session_store::CapturedClosedTab::ram_tail_bytes)
        .sum::<usize>();
    let mut new_refusal = std::mem::take(&mut app.state.persistence.storage.new_refusal);
    for tab in app.tab_manager.tabs.as_mut_slice() {
        for buffer in tab.buffers_mut() {
            if let Some(store) = store {
                let _ = buffer.document_mut().bind_store(store.clone());
            }
            tails = tails.saturating_add(buffer.document().piece_tree().ram_tail_bytes());
            if let Some(cause) = buffer.storage_refusal.take() {
                new_refusal = true;
                app.state.persistence.storage.refusal =
                    Some((Some((buffer.id, buffer.storage_revision())), cause));
            }
            buffer.update_storage_attention(kind != 0);
        }
        for view in tab.layout.views() {
            tails = tails.saturating_add(
                view.deferred_editor_events
                    .iter()
                    .map(|event| match event {
                        eframe::egui::Event::Text(text)
                        | eframe::egui::Event::Paste(text)
                        | eframe::egui::Event::Ime(eframe::egui::ImeEvent::Commit(text)) => {
                            text.len()
                        }
                        _ => 0,
                    })
                    .sum::<usize>(),
            );
        }
    }
    if let Some((owner, _)) = &app.state.persistence.storage.refusal {
        let unchanged = owner.as_ref().map_or_else(
            || app.tab_manager.tabs.is_empty(),
            |(id, revision)| {
                app.tab_manager
                    .tabs
                    .as_slice()
                    .iter()
                    .flat_map(|tab| tab.buffers())
                    .find(|buffer| buffer.id == *id)
                    .is_some_and(|buffer| buffer.storage_revision() == *revision)
            },
        );
        if !unchanged || kind == 0 || (kind != previous && !new_refusal) {
            app.state.persistence.storage.refusal = None;
        }
    }
    tails
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
