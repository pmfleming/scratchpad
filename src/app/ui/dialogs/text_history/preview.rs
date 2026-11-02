//! A starting marker and a preview marker on the existing history. The live
//! buffer stays untouched until confirmation; only the editor projection changes.
use super::{model::HistoryAction, states::States};
use crate::app::{
    app_state::{
        ScratchpadApp,
        workspace::{accessors, mutation},
    },
    domain::{
        BufferState, CursorRange, CursorRevealMode, EditorViewState, ViewId, WorkspaceTab,
        buffer::DocumentRevision,
    },
    text_history::entries_for_buffer,
};
use eframe::egui;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
mod snapshots;
pub(super) use snapshots::confirming;
pub(super) use snapshots::poll;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Target {
    State(Option<u64>),
    Snapshot(i64),
}

#[derive(Clone)]
struct Preview {
    buffer_id: u64,
    revision: DocumentRevision,
    history_revision: u64,
    target: Target,
    tab: Arc<Mutex<HistoryPreview>>,
}

/// Separate view state for each visible split, without altering the real views'
/// selections, layout caches, or scroll positions during a preview.
pub(crate) struct HistoryPreview {
    pub tab: WorkspaceTab,
    views: HashMap<ViewId, ViewId>,
}

impl HistoryPreview {
    fn new(buffer: BufferState) -> Self {
        Self {
            tab: WorkspaceTab::new(buffer),
            views: HashMap::new(),
        }
    }

    pub(crate) fn view_for(&mut self, source: &EditorViewState) -> ViewId {
        let id = if let Some(&id) = self.views.get(&source.id) {
            id
        } else if self.views.is_empty() {
            self.tab.layout.active_view_id()
        } else {
            let selection = self
                .tab
                .layout
                .active_view()
                .and_then(|view| view.cursor_range)
                .unwrap_or_default();
            let mut view = EditorViewState::new(self.tab.active_buffer().id);
            view.set_pending_cursor_range_anchored(self.tab.active_buffer_mut(), selection);
            view.request_cursor_reveal(CursorRevealMode::Center);
            let id = view.id;
            self.tab.layout.views.push(view);
            id
        };
        self.views.insert(source.id, id);
        if let Some(view) = self.tab.layout.view_mut(id) {
            view.show_line_numbers = source.show_line_numbers;
            view.right_to_left_reading_order = source.right_to_left_reading_order;
        }
        id
    }
}

fn key() -> egui::Id {
    egui::Id::unique("history.preview")
}
fn open_key() -> egui::Id {
    egui::Id::unique("history.preview.open")
}
pub(super) fn rect_key() -> egui::Id {
    egui::Id::unique("history.preview.rect")
}

impl Preview {
    fn valid(&self, buffer: &BufferState) -> bool {
        self.buffer_id == buffer.id
            && self.revision == buffer.document().revision_key()
            && self.history_revision == buffer.document().history_revision_counter()
    }
}

pub(crate) fn history_preview(
    ctx: &egui::Context,
    buffer: &BufferState,
) -> Option<Arc<Mutex<HistoryPreview>>> {
    ctx.data(|d| d.get_temp::<Preview>(key()))
        .filter(|p| p.valid(buffer))
        .map(|p| p.tab)
}

pub(super) fn position(ctx: &egui::Context, id: u64, states: &States<'_>) -> usize {
    ctx.data(|d| d.get_temp::<Preview>(key()))
        .filter(|p| p.buffer_id == id)
        .and_then(|p| match p.target {
            Target::State(after) => states.index(after),
            Target::Snapshot(_) => None,
        })
        .unwrap_or(states.position)
}

pub(super) fn clear(ctx: &egui::Context) {
    snapshots::cancel(ctx);
    ctx.data_mut(|d| d.remove::<Preview>(key()));
    super::list::request_reveal(ctx);
    ctx.request_repaint();
}

pub(super) fn close(ctx: &egui::Context, app: &mut ScratchpadApp) {
    clear(ctx);
    ctx.data_mut(|d| d.remove::<bool>(open_key()));
    crate::app::commands::close_text_history(&mut app.state.dialogs);
    accessors::request_focus_for_active_view(app);
}

/// Return true for the first render after opening, including reopening at the
/// same history position. This is independent of whether the rail owns focus.
pub(super) fn begin(ctx: &egui::Context) -> bool {
    ctx.data_mut(|d| {
        let first = d.get_temp::<bool>(open_key()) != Some(true);
        d.insert_temp(open_key(), true);
        first
    })
}

pub(super) fn prepare(ctx: &egui::Context, app: &mut ScratchpadApp) {
    if !app.state.dialogs.text_history.is_open() {
        snapshots::cancel(ctx);
        ctx.data_mut(|d| {
            d.remove::<Preview>(key());
            d.remove::<bool>(open_key());
        });
        return;
    }
    if let Some(preview) = ctx.data(|d| d.get_temp::<Preview>(key())) {
        let valid = app
            .tab_manager
            .tabs
            .as_slice()
            .iter()
            .find_map(|tab| tab.buffer_by_id(preview.buffer_id))
            .is_some_and(|buffer| preview.valid(buffer));
        if !valid {
            clear(ctx);
        }
    }
    if app.state.dialogs.any_modal_open()
        || egui::Popup::is_any_open(ctx)
        || accessors::pending_action(app).is_some()
    {
        return;
    }
    let outside = ctx
        .data(|d| d.get_temp::<egui::Rect>(rect_key()))
        .is_some_and(|rect| {
            ctx.input(|i| {
                i.pointer.any_pressed()
                    && i.pointer.interact_pos().is_some_and(|p| !rect.contains(p))
            })
        });
    if outside {
        close(ctx, app);
    }
}

pub(super) fn select(
    ctx: &egui::Context,
    app: &mut ScratchpadApp,
    id: u64,
    after: Option<u64>,
) -> bool {
    let Some(buffer) = app
        .tab_manager
        .tabs
        .as_slice()
        .iter()
        .find_map(|tab| tab.buffer_by_id(id))
    else {
        return false;
    };
    snapshots::cancel(ctx);
    let existing = ctx
        .data(|d| d.get_temp::<Preview>(key()))
        .filter(|p| p.valid(buffer) && matches!(p.target, Target::State(_)));
    if existing
        .as_ref()
        .is_some_and(|p| p.target == Target::State(after))
    {
        return true;
    }
    let mut preview = existing.unwrap_or_else(|| Preview {
        buffer_id: id,
        revision: buffer.document().revision_key(),
        history_revision: buffer.document().history_revision_counter(),
        target: Target::State(after),
        tab: Arc::new(Mutex::new(HistoryPreview::new(
            super::snapshots::preview_buffer(ctx, buffer),
        ))),
    });
    {
        let Ok(mut surface) = preview.tab.lock() else {
            return false;
        };
        let tab = &mut surface.tab;
        let entries = entries_for_buffer(tab.active_buffer());
        let states = States::new(&entries, id);
        let Some(position) = states.index(after).filter(|&k| states.reachable(k)) else {
            return false;
        };
        let selection = if let Some(action) = states.target(position) {
            let buffer = tab.active_buffer_mut();
            let result = if position < states.position {
                buffer.apply_text_history_undo(action.entry_id)
            } else {
                buffer.apply_text_history_redo(action.entry_id)
            };
            let Ok(selection) = result else {
                return false;
            };
            selection
        } else {
            CursorRange::default()
        };
        tab.active_buffer_mut().mark_as_loading_preview();
        let ids: Vec<_> = tab.layout.views.iter().map(|view| view.id).collect();
        for id in ids {
            if let Some((buffer, view)) = tab.buffer_and_view_mut(id) {
                view.set_cursor_range_anchored(buffer, selection);
                view.set_pending_cursor_range_anchored(buffer, selection);
                view.request_cursor_reveal(CursorRevealMode::Center);
            }
        }
    }
    preview.target = Target::State(after);
    ctx.data_mut(|d| d.insert_temp(key(), preview));
    if super::read_follow_focus(ctx) && super::active_buffer(app) != Some(id) {
        super::navigation::activate_file(app, id);
    }
    ctx.memory_mut(|m| m.request_focus(super::list::focus_id()));
    super::navigation::retain_tool_focus(ctx, app);
    ctx.request_repaint();
    true
}

pub(super) fn confirm(ctx: &egui::Context, app: &mut ScratchpadApp, id: u64, after: Option<u64>) {
    if !select(ctx, app, id, after) {
        return;
    }
    commit(ctx, app);
}

fn commit(ctx: &egui::Context, app: &mut ScratchpadApp) {
    let Some(preview) = ctx.data(|d| d.get_temp::<Preview>(key())) else {
        return;
    };
    let snapshot = {
        let Ok(tab) = preview.tab.lock() else {
            return;
        };
        tab.tab.active_buffer().document_snapshot()
    };
    let id = preview.buffer_id;
    let Some(index) = mutation::tab_index_for_buffer(app, id) else {
        return;
    };
    let tab = &mut app.tab_manager.tabs.as_mut_slice()[index];
    let selection = tab
        .layout
        .views
        .iter()
        .find(|view| view.buffer_id == id)
        .and_then(|view| view.cursor_range)
        .unwrap_or_default();
    let Some(buffer) = tab.buffer_by_id_mut(id) else {
        return;
    };
    if !preview.valid(buffer) {
        clear(ctx);
        return;
    }
    if buffer.commit_history_preview(&snapshot, selection) {
        let refresh = buffer.text_metadata_refresh_needed().then(|| {
            (
                buffer.id,
                buffer.document_revision(),
                buffer.document_snapshot(),
                buffer.format.clone(),
            )
        });
        mutation::record_pending_text_history_event(app, index, id);
        if let Some((id, revision, snapshot, format)) = refresh {
            app.queue_background_text_metadata_refresh(id, revision, snapshot, format);
        }
        app.state.text_history_cache = crate::app::text_history::TextHistoryCache::default();
        app.tab_manager.mark_session_dirty();
        crate::app::app_state::search_runtime::mark_search_dirty(app);
        app.enforce_aggregate_text_history_budget();
        app.apply_current_tab_ordering();
    }
    clear(ctx);
}

pub(super) fn execute(ctx: &egui::Context, app: &mut ScratchpadApp, action: HistoryAction) {
    match action {
        HistoryAction::Preview { buffer_id, after } => {
            select(ctx, app, buffer_id, after);
        }
        HistoryAction::State { buffer_id, after } => confirm(ctx, app, buffer_id, after),
        HistoryAction::PreviewSnapshot {
            buffer_id,
            version_id,
        } => snapshots::request(ctx, app, buffer_id, version_id, false),
        HistoryAction::Snapshot {
            buffer_id,
            version_id,
        } => snapshots::request(ctx, app, buffer_id, version_id, true),
        other @ HistoryAction::Latest { .. } => {
            clear(ctx);
            super::snapshots::execute(ctx, app, other);
        }
    }
}

pub(super) fn snapshot_id(ctx: &egui::Context, id: u64) -> Option<i64> {
    ctx.data(|d| d.get_temp::<Preview>(key()))
        .filter(|p| p.buffer_id == id)
        .and_then(|p| match p.target {
            Target::Snapshot(id) => Some(id),
            Target::State(_) => None,
        })
}
