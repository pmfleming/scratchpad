use super::super::{ScratchpadApp, StatusDomain};
use crate::app::app_state::RECENTLY_CLOSED_FILE_LIMIT;
use crate::app::app_state::settings_controller;
use crate::app::app_state::workspace::accessors as workspace_accessors;
use crate::app::diagnostics;
use crate::app::domain::{BufferId, ViewId, WorkspaceTab};
use crate::app::services::file_controller::FileController;
use crate::app::services::settings_store::{FileOpenDisposition, NewTabPlacement};
use crate::app::utils::pluralize;
use std::collections::HashSet;
use std::path::PathBuf;

pub(crate) fn new_tab(app: &mut ScratchpadApp) {
    append_created_tab(app, WorkspaceTab::untitled());
}

pub(crate) fn append_created_tab(app: &mut ScratchpadApp, tab: WorkspaceTab) {
    create_workspace_tab(
        app,
        tab,
        Some(crate::app::services::session_store::activity::ActivityKind::Created),
    );
}

pub(crate) fn open_file(app: &mut ScratchpadApp) {
    if matches!(
        app.state.app_settings.file_open_disposition(),
        FileOpenDisposition::CurrentTab
    ) {
        FileController::open_file_here(app);
    } else {
        FileController::open_file(app);
    }
}

pub(crate) fn open_file_here(app: &mut ScratchpadApp) {
    FileController::open_file_here(app);
}

pub(crate) fn open_user_manual(app: &mut ScratchpadApp) {
    let path = workspace_accessors::user_manual_path(app).to_path_buf();
    if !path.is_file() {
        diagnostics::record_io_error(
            "open_user_manual",
            Some(&path),
            "workspace::lifecycle",
            &"User manual not found",
        );
        app.state.status.set_error_status_with_detail(
            StatusDomain::File,
            "Could not open the user manual.",
            path.display().to_string(),
        );
        return;
    }

    settings_controller::activate_workspace_surface(app);
    FileController::open_paths_async(app, vec![path]);
}

pub(crate) fn save_file(app: &mut ScratchpadApp) {
    FileController::save_file(app);
}

pub(crate) fn save_all_files(app: &mut ScratchpadApp) {
    let active_tab_index = app.tab_manager.active_tab_index;
    let active_view_ids = app
        .tab_manager
        .tabs
        .as_slice()
        .iter()
        .map(|tab| tab.layout.active_view_id)
        .collect::<Vec<_>>();
    let targets = save_all_targets(app);
    let had_targets = !targets.is_empty();
    let mut queued_count = 0usize;

    for (tab_index, view_id) in targets {
        app.tab_manager.set_active_tab_index_clamped(tab_index);
        if let Some(tab) = app.tab_manager.tabs.as_mut_slice().get_mut(tab_index) {
            tab.activate_view(view_id);
        }
        if FileController::save_file_at(app, tab_index) {
            queued_count += 1;
        }
        if crate::app::app_state::workspace::accessors::pending_action(app).is_some() {
            break;
        }
    }

    app.tab_manager
        .set_active_tab_index_clamped(active_tab_index);
    for (tab, view_id) in app
        .tab_manager
        .tabs
        .as_mut_slice()
        .iter_mut()
        .zip(active_view_ids)
    {
        tab.activate_view(view_id);
    }
    workspace_accessors::request_focus_for_active_view(app);
    if had_targets {
        if queued_count > 0
            && crate::app::app_state::workspace::accessors::pending_action(app).is_none()
        {
            app.state.status.set_info_status_in_domain(
                StatusDomain::File,
                format!("Saving {}.", pluralize(queued_count, "file")),
            );
        }
        app.tab_manager.mark_session_dirty();
        crate::app::app_state::workspace::accessors::request_session_persist(app);
    }
}

pub(crate) fn save_file_at(app: &mut ScratchpadApp, index: usize) -> bool {
    FileController::save_file_at(app, index)
}

pub(crate) fn save_file_as(app: &mut ScratchpadApp) {
    FileController::save_file_as(app);
}

pub(crate) fn perform_close_tab(app: &mut ScratchpadApp, index: usize) {
    perform_close_tab_no_persist(app, index);
}

pub(crate) fn perform_close_tab_no_persist(app: &mut ScratchpadApp, index: usize) {
    if index >= app.tab_manager.tabs.len() {
        return;
    }
    if let Err(error) = crate::app::services::session_manager::persist_tab_close(app, index) {
        if error.kind() != std::io::ErrorKind::WouldBlock {
            app.state.status.report_session_save_failed(error);
        }
        return;
    }
    close_tab_internal(app, index);
}

pub(crate) fn reopen_archived_tab(app: &mut ScratchpadApp, token: &str) {
    if app.startup_restore_pending() || app.state.persistence.session_restore_failed {
        return;
    }
    app.queue_retained_open(
        crate::app::services::session_store::RetainedTarget::Archive(token.to_owned()),
        None,
        None,
    );
}

pub(crate) fn install_archive(app: &mut ScratchpadApp, mut tab: WorkspaceTab) -> bool {
    let live_documents: HashSet<_> = app
        .tab_manager
        .tabs
        .as_slice()
        .iter()
        .flat_map(WorkspaceTab::buffers)
        .map(|live| live.temp_id.clone())
        .collect();
    if tab
        .buffers()
        .all(|buffer| live_documents.contains(&buffer.temp_id))
    {
        return false;
    }
    let shared_views: Vec<_> = tab
        .layout
        .views()
        .iter()
        .filter(|view| {
            tab.buffer_by_id(view.buffer_id)
                .is_some_and(|buffer| live_documents.contains(&buffer.temp_id))
        })
        .map(|view| view.id)
        .collect();
    for view in shared_views {
        tab.close_view(view);
    }
    for buffer in tab.buffers_mut() {
        // A restored split must not steal a path from an already-open tab.
        if buffer
            .path
            .as_ref()
            .is_some_and(|path| app.tab_manager.find_tab_by_path(path).is_some())
        {
            if let Some(document) = buffer.pending_reopen_original.take() {
                *buffer.document_mut() = *document;
                buffer.pending_reopen_external = None;
                buffer.refresh_text_metadata();
            }
            buffer.recover_as_untitled("recovered");
        }
    }
    create_workspace_tab(
        app,
        tab,
        Some(crate::app::services::session_store::activity::ActivityKind::Opened),
    );
    true
}

pub(crate) fn append_tab(app: &mut ScratchpadApp, tab: WorkspaceTab) {
    create_workspace_tab(
        app,
        tab,
        Some(crate::app::services::session_store::activity::ActivityKind::Opened),
    );
}

pub(crate) fn append_transferred_tab(app: &mut ScratchpadApp, tab: WorkspaceTab) {
    create_workspace_tab(app, tab, None);
}

fn create_workspace_tab(
    app: &mut ScratchpadApp,
    tab: WorkspaceTab,
    kind: Option<crate::app::services::session_store::activity::ActivityKind>,
) {
    let documents: Vec<_> = tab
        .buffers()
        .filter(|b| !b.is_loading_preview)
        .map(|b| b.temp_id.clone())
        .collect();
    app.reload_settings_before_workspace_change();
    crate::app::app_state::frame::begin_layout_transition(app);
    let index = new_tab_insert_index(app);
    app.tab_manager.insert_tab(index, tab);
    if let Some(kind) = kind {
        crate::app::services::session_store::activity::record_installed(app, &documents, kind);
    }
    app.apply_history_budget_to_open_buffers();
    app.apply_current_tab_ordering();
    settings_controller::activate_workspace_surface(app);
    crate::app::app_state::workspace::display_tabs::select_only_tab_slot(
        app,
        crate::app::app_state::workspace::display_tabs::active_tab_slot_index(app),
    );
    crate::app::app_state::search_runtime::mark_search_dirty(app);
    workspace_accessors::request_focus_for_active_view(app);
}

fn new_tab_insert_index(app: &ScratchpadApp) -> usize {
    match app.state.app_settings.new_tab_placement() {
        NewTabPlacement::Start => 0,
        NewTabPlacement::End => app.tab_manager.tabs.as_slice().len(),
        NewTabPlacement::BeforeSelection => selected_workspace_tab_range(app).0,
        NewTabPlacement::AfterSelection => selected_workspace_tab_range(app).1 + 1,
    }
    .min(app.tab_manager.tabs.as_slice().len())
}

fn selected_workspace_tab_range(app: &ScratchpadApp) -> (usize, usize) {
    let mut first = None::<usize>;
    let mut last = None::<usize>;
    for workspace_index in app
        .state
        .workspace_selection
        .selected_slots()
        .filter_map(|slot_index| {
            crate::app::app_state::workspace::display_tabs::workspace_index_for_slot(
                app, slot_index,
            )
        })
    {
        first = Some(first.map_or(workspace_index, |first| first.min(workspace_index)));
        last = Some(last.map_or(workspace_index, |last| last.max(workspace_index)));
    }

    if let (Some(first), Some(last)) = (first, last) {
        (first, last)
    } else {
        let active = app
            .tab_manager
            .active_tab_index
            .min(app.tab_manager.tabs.as_slice().len().saturating_sub(1));
        (active, active)
    }
}

pub(crate) fn close_tab_internal(app: &mut ScratchpadApp, index: usize) {
    let buffers: Vec<_> = app.tab_manager.tabs.as_slice()[index]
        .buffers()
        .map(|b| b.id)
        .collect();
    cancel_loading_previews(app, &buffers);
    let closed_file_paths = tab_file_paths(app, index);
    let settings_refresh = app.settings_toml_refresh_on_tab_close(index);
    crate::app::app_state::frame::begin_layout_transition(app);
    app.tab_manager.close_tab_internal(index);
    record_recently_closed_file_paths(app, closed_file_paths);
    crate::app::app_state::workspace::display_tabs::ensure_active_tab_slot_selected(app);
    crate::app::app_state::search_runtime::mark_search_dirty(app);
    workspace_accessors::request_focus_for_active_view(app);
    app.apply_settings_toml_refresh(settings_refresh);
}

pub(crate) fn record_recently_closed_file_paths(app: &mut ScratchpadApp, paths: Vec<PathBuf>) {
    if !app.state.app_settings.recent_files_enabled() {
        return;
    }

    let closed_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .min(i64::MAX as u64) as i64;
    for path in paths {
        app.state
            .recently_closed_files
            .retain(|existing| !crate::app::paths_match(&existing.path, &path));
        app.state.recently_closed_files.push_front(
            crate::app::services::settings_store::RecentlyClosedFile {
                path,
                closed_at_unix_seconds: Some(closed_at),
            },
        );
    }

    while app.state.recently_closed_files.len() > RECENTLY_CLOSED_FILE_LIMIT {
        app.state.recently_closed_files.pop_back();
    }

    app.state.app_settings.workspace.recently_closed_files =
        app.state.recently_closed_files.iter().cloned().collect();
    if let Err(error) = crate::app::app_state::settings_state::persist_settings_now(app) {
        app.state.status.report_settings_save_failed(error);
    }
}

pub(crate) fn cancel_loading_previews(app: &mut ScratchpadApp, buffers: &[BufferId]) {
    let cancelled: Vec<_> = app
        .tab_manager
        .tabs
        .as_slice()
        .iter()
        .flat_map(WorkspaceTab::buffers)
        .filter(|buffer| buffer.is_loading_preview && buffers.contains(&buffer.id))
        .filter_map(|buffer| {
            buffer
                .path
                .clone()
                .map(|path| (path, buffer.path_key.clone()))
        })
        .collect();
    for (path, key) in cancelled {
        if let Some(tokens) = app.state.background_io.path_imports.remove(&path) {
            for token in tokens {
                if let Some(token) = token.upgrade() {
                    token.store(true, std::sync::atomic::Ordering::Release);
                }
            }
        }
        app.state.cancelled_loading_previews.insert(path);
        if let Some(key) = key {
            app.state.pending_open_file_paths.remove(&key);
        }
    }
}

fn tab_file_paths(app: &ScratchpadApp, index: usize) -> Vec<PathBuf> {
    app.tab_manager
        .tabs
        .as_slice()
        .get(index)
        .map(|tab| {
            tab.buffers()
                .filter_map(|buffer| buffer.path.clone())
                .collect()
        })
        .unwrap_or_default()
}

fn save_all_targets(app: &ScratchpadApp) -> Vec<(usize, ViewId)> {
    app.tab_manager
        .tabs
        .as_slice()
        .iter()
        .enumerate()
        .flat_map(|(tab_index, tab)| {
            let mut seen = HashSet::<BufferId>::new();
            tab.layout.views.iter().filter_map(move |view| {
                if !seen.insert(view.buffer_id) {
                    return None;
                }
                let buffer = tab.buffer_by_id(view.buffer_id)?;
                if !buffer.is_dirty {
                    return None;
                }
                Some((tab_index, view.id))
            })
        })
        .collect()
}
