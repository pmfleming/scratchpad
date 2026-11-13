//! A virtualized activity list, independent of per-document branch navigation.
mod model;
mod navigation;
mod row;
#[cfg(test)]
use row::row_change;
use row::{RowState, paint_row};
#[cfg(test)]
mod tests;

use super::{HistoryUi, presentation, style};
use crate::app::{
    app_state::ScratchpadApp,
    services::session_store::{
        SessionStore,
        activity::ActivityKind,
        timeline::{
            TIMELINE_PAGE_SIZE, TimelineCursor, TimelineId, TimelineKind, TimelinePage, TimelineRow,
        },
    },
    theme,
    ui::widget_ids,
};
use eframe::egui;
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

const ROW_HEIGHT: f32 = 44.0;
const RAIL_X: f32 = 11.0;
type PendingPage = Arc<Mutex<Option<Result<TimelinePage, String>>>>;

#[derive(Clone, PartialEq, Eq)]
struct Source {
    loaded: Vec<String>,
    opened: Vec<String>,
    search: String,
    epoch: u64,
    commit: u64,
}

impl Source {
    fn capture(app: &ScratchpadApp, search: &str, loaded: Vec<String>) -> Self {
        Self {
            loaded,
            opened: app
                .tab_manager
                .tabs
                .as_slice()
                .iter()
                .flat_map(|tab| tab.buffers())
                .filter(|buffer| !buffer.is_loading_preview)
                .map(|buffer| buffer.temp_id.clone())
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect(),
            search: search.to_owned(),
            epoch: app.state.persistence.session_store.history_epoch(),
            commit: app.state.persistence.session_store.commit_epoch(),
        }
    }
}

enum CachedTail {
    More(TimelineCursor),
    Complete,
}

struct State {
    live: model::LiveRows,
    source: Option<Source>,
    persisted: BTreeMap<TimelineCursor, TimelineRow>,
    overlay: BTreeMap<TimelineCursor, TimelineRow>,
    pending: Option<PendingPage>,
    next: Option<TimelineCursor>,
    resume_older: Option<CachedTail>,
    fetched: bool,
    error: Option<String>,
    rows: Vec<TimelineRow>,
    limit: usize,
    selected: Option<TimelineId>,
    offset: f32,
    anchor: Option<(TimelineId, f32)>,
    scroll_to_selection: bool,
    reveal_now: Option<TimelineId>,
    viewport_height: f32,
    jump_end: Option<navigation::End>,
    expanded_operations: std::collections::HashSet<u64>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            live: model::LiveRows::default(),
            source: None,
            persisted: BTreeMap::new(),
            overlay: BTreeMap::new(),
            pending: None,
            next: None,
            resume_older: None,
            fetched: false,
            error: None,
            rows: Vec::new(),
            limit: TIMELINE_PAGE_SIZE,
            selected: None,
            offset: 0.0,
            anchor: None,
            scroll_to_selection: false,
            reveal_now: None,
            viewport_height: 0.0,
            jump_end: None,
            expanded_operations: std::collections::HashSet::default(),
        }
    }
}

fn key() -> egui::Id {
    egui::Id::unique("history.timeline.model")
}
fn shared(ctx: &egui::Context) -> Arc<Mutex<State>> {
    ctx.data_mut(|d| {
        d.get_temp_mut_or_default::<Arc<Mutex<State>>>(key())
            .clone()
    })
}

#[derive(Clone)]
pub(super) struct Action {
    pub operation: Option<(u64, usize)>,
    pub document: String,
    pub node: Option<u64>,
    pub inspect: bool,
}

impl State {
    fn fetch(&mut self, store: SessionStore, ctx: egui::Context) {
        if self.pending.is_some() {
            return;
        }
        let Some(source) = self.source.clone() else {
            return;
        };
        let before = if self.fetched {
            self.next.clone()
        } else {
            None
        };
        if self.fetched && before.is_none() {
            return;
        }
        self.error = None;
        let pending = Arc::new(Mutex::new(None));
        self.pending = Some(pending.clone());
        if let Err(error) = std::thread::Builder::new()
            .name("history-timeline".into())
            .spawn(move || {
                let result = store
                    .timeline_page(before, source.loaded, source.search)
                    .map_err(|e| e.to_string());
                *pending.lock().unwrap_or_else(|e| e.into_inner()) = Some(result);
                ctx.request_repaint();
            })
        {
            self.pending = None;
            self.error = Some(error.to_string());
        }
    }

    fn refresh(&mut self, app: &ScratchpadApp, search: &str, ctx: &egui::Context) {
        let (loaded, mut changed) = self.live.update(app, search);
        let source = Source::capture(app, search, loaded);
        if self.source.as_ref() != Some(&source) {
            self.restart(source, search);
            changed = true;
        }
        changed |= self.take_pending_activity(app, search);
        changed |= self.receive_page();
        if !self.fetched && self.pending.is_none() && self.error.is_none() {
            self.fetch(app.state.persistence.session_store.clone(), ctx.clone());
        }
        if changed {
            self.rebuild();
        }
    }

    /// Starts paging `source` from the newest row. Pages already loaded for the same
    /// history and search are kept, and paging resumes below the oldest of them.
    fn restart(&mut self, source: Source, search: &str) {
        let old = self.source.as_ref();
        let same_history = old.is_some_and(|old| old.epoch == source.epoch);
        let preserve = same_history && old.is_some_and(|old| old.search == source.search);
        if old.is_some() && !same_history {
            self.selected = None;
            self.anchor = None;
            self.overlay.clear();
        }
        self.overlay.retain(|_, row| matches_search(row, search));
        if preserve {
            self.persisted.append(&mut self.live.retired);
            self.persisted.retain(|_, row| {
                row.node().is_none() || source.loaded.binary_search(&row.cursor.document).is_err()
            });
            if self.fetched {
                self.resume_older = Some(
                    self.next
                        .clone()
                        .map_or(CachedTail::Complete, CachedTail::More),
                );
            }
        } else {
            self.reveal_now = None;
            self.jump_end = None;
            self.persisted.clear();
            self.live.retired.clear();
            self.resume_older = None;
        }
        self.source = Some(source);
        self.pending = None;
        self.next = None;
        self.fetched = false;
        self.error = None;
    }

    /// Shows activity that is not yet saved. Returns true when a row was added.
    fn take_pending_activity(&mut self, app: &ScratchpadApp, search: &str) -> bool {
        let mut changed = false;
        for event in &app.state.persistence.pending_activity {
            let row = event.timeline_row();
            if matches_search(&row, search) && !self.overlay.contains_key(&row.cursor) {
                self.overlay.insert(row.cursor.clone(), row);
                changed = true;
            }
        }
        changed
    }

    /// Merges a finished page fetch. Returns true when rows arrived.
    fn receive_page(&mut self) -> bool {
        let Some(reply) = self
            .pending
            .as_ref()
            .and_then(|p| p.lock().unwrap_or_else(|e| e.into_inner()).take())
        else {
            return false;
        };
        self.pending = None;
        let page = match reply {
            Ok(page) => page,
            Err(error) => {
                self.error = Some(error);
                return false;
            }
        };
        for row in page.rows {
            self.overlay.remove(&row.cursor);
            self.persisted.insert(row.cursor.clone(), row);
        }
        self.next = match self.resume_older.take() {
            Some(CachedTail::Complete) => None,
            Some(CachedTail::More(old)) => page.older.map(|new| old.min(new)),
            None => page.older,
        };
        self.fetched = true;
        true
    }

    fn rebuild(&mut self) {
        let operations: std::collections::HashSet<_> = self
            .overlay
            .values()
            .chain(self.persisted.values())
            .filter_map(|row| replace_all(row).map(|(operation_id, _)| operation_id))
            .collect();
        // Live rows win over unsaved activity, which wins over saved pages.
        let mut sources = [
            self.live.rows.iter().rev().peekable(),
            self.overlay.iter().rev().peekable(),
            self.persisted.iter().rev().peekable(),
        ];
        let mut rows = Vec::new();
        let mut count = 0;
        let mut anchored = self.anchor.is_none();
        while count < self.limit || !anchored || self.jump_end == Some(navigation::End::Oldest) {
            let Some(row) = next_newest(&mut sources) else {
                break;
            };
            if self.folded_into_operation(row, &operations) {
                continue;
            }
            count += 1;
            anchored |= self.anchor.as_ref().is_some_and(|(id, _)| row.id() == *id);
            if self.next.as_ref().is_some_and(|bound| row.cursor < *bound) {
                break;
            }
            rows.push(row.clone());
        }
        rows.sort_by(|a, b| {
            b.cursor
                .sequence
                .cmp(&a.cursor.sequence)
                .then_with(|| a.node().is_some().cmp(&b.node().is_some()))
                .then_with(|| b.cursor.cmp(&a.cursor))
        });
        self.rows = rows;
        if self.reveal_now.is_none()
            && let Some((id, fraction)) = &self.anchor
            && let Some(index) = self.rows.iter().position(|row| row.id() == *id)
        {
            self.offset = index as f32 * ROW_HEIGHT + fraction;
        }
    }

    /// An edit made by a Replace all stays hidden until its operation row is expanded.
    fn folded_into_operation(
        &self,
        row: &TimelineRow,
        operations: &std::collections::HashSet<u64>,
    ) -> bool {
        row.node().is_some()
            && operations.contains(&row.cursor.sequence)
            && !self.expanded_operations.contains(&row.cursor.sequence)
    }
}

type NewestFirst<'a> = std::iter::Peekable<
    std::iter::Rev<std::collections::btree_map::Iter<'a, TimelineCursor, TimelineRow>>,
>;

/// The newest row across `sources`; a row present in several is taken from the first.
fn next_newest<'a>(sources: &mut [NewestFirst<'a>]) -> Option<&'a TimelineRow> {
    let key = sources
        .iter_mut()
        .filter_map(|source| source.peek().map(|(key, _)| *key))
        .max()?;
    let mut newest = None;
    for source in sources {
        if source.peek().is_some_and(|(next, _)| *next == key) {
            let row = source.next().map(|(_, row)| row);
            newest = newest.or(row);
        }
    }
    newest
}

fn matches_search(row: &TimelineRow, search: &str) -> bool {
    row.name.to_lowercase().contains(search) || row.label().to_lowercase().contains(search)
}

/// The operation and file count of a Replace all row.
fn replace_all(row: &TimelineRow) -> Option<(u64, usize)> {
    match row.kind {
        TimelineKind::Activity(ActivityKind::ReplaceAll {
            operation_id,
            files,
            ..
        }) => Some((operation_id, files)),
        _ => None,
    }
}

pub(super) fn keyboard(ctx: &egui::Context, app: &ScratchpadApp) -> Option<Action> {
    let shared = shared(ctx);
    let mut state = shared.lock().unwrap_or_else(|e| e.into_inner());
    state.refresh(
        app,
        &ctx.data(|d| d.get_temp::<HistoryUi>(super::state_key()))
            .map_or_else(String::new, |s| s.search.to_lowercase()),
        ctx,
    );
    let indices: Vec<_> = state
        .rows
        .iter()
        .enumerate()
        .filter_map(|(i, row)| row.node().is_some().then_some(i))
        .collect();
    if indices.is_empty() {
        ctx.input_mut(|i| {
            for key in [
                egui::Key::ArrowUp,
                egui::Key::ArrowDown,
                egui::Key::Home,
                egui::Key::End,
                egui::Key::Enter,
            ] {
                i.consume_key(egui::Modifiers::NONE, key);
            }
        });
        return None;
    }
    let mut position = indices
        .iter()
        .position(|&i| Some(state.rows[i].id()) == state.selected);
    let mut changed = false;
    let mut select = false;
    ctx.input_mut(|input| {
        for key in [
            egui::Key::ArrowUp,
            egui::Key::ArrowDown,
            egui::Key::Home,
            egui::Key::End,
            egui::Key::Enter,
        ] {
            if input.consume_key(egui::Modifiers::NONE, key) {
                position = Some(match key {
                    egui::Key::ArrowUp => position.unwrap_or(1).saturating_sub(1),
                    egui::Key::ArrowDown => position.map_or(0, |p| (p + 1).min(indices.len() - 1)),
                    egui::Key::Home => 0,
                    egui::Key::End => indices.len() - 1,
                    _ => position.unwrap_or(0),
                });
                changed = true;
                select = true;
            }
        }
    });
    if changed {
        let row = &state.rows[indices[position?]];
        let action = Action {
            operation: None,
            document: row.cursor.document.clone(),
            node: row.node(),
            inspect: false,
        };
        state.selected = Some(row.id());
        state.scroll_to_selection = true;
        state.jump_end = None;
        state.reveal_now = None;
        if select {
            return Some(action);
        }
    }
    None
}

pub(super) fn apply(ctx: &egui::Context, app: &mut ScratchpadApp, action: Action) {
    if let Some((operation, files)) = action.operation {
        crate::app::app_state::search_replace::transaction::undo(app, operation, files);
        ctx.request_repaint();
        return;
    }
    let location = app
        .tab_manager
        .tabs
        .as_slice()
        .iter()
        .enumerate()
        .find_map(|(index, tab)| {
            tab.buffers()
                .find(|b| b.temp_id == action.document)
                .map(|b| (index, b.id))
        });
    if let Some((index, buffer)) =
        location.filter(|(index, _)| !app.tab_manager.cold_session_tabs().contains_key(index))
    {
        let _ = index;
        super::selection::activate(ctx, app, buffer);
        if let Some(node) = action.node {
            super::navigate(ctx, app, buffer, node);
        }
        ctx.data_mut(|d| {
            let state = d.get_temp_mut_or_default::<HistoryUi>(super::state_key());
            state.selected = Some(buffer);
            if action.inspect {
                state.all_files = false;
                state.search.clear();
                state.forget = None;
            }
        });
    } else {
        let target = location
            .and_then(|(index, _)| app.tab_manager.cold_session_tabs().get(&index).cloned())
            .map_or_else(
                || {
                    crate::app::services::session_store::RetainedTarget::Document(
                        action.document.clone(),
                    )
                },
                crate::app::services::session_store::RetainedTarget::Cold,
            );
        app.queue_retained_open(target, Some(action.document.clone()), action.node);
        ctx.data_mut(|d| {
            let state = d.get_temp_mut_or_default::<HistoryUi>(super::state_key());
            state.pending_file = Some(action.document);
            state.open_error = false;
            if action.inspect {
                state.all_files = false;
                state.search.clear();
                state.forget = None;
            }
        });
    }
    ctx.request_repaint();
}

pub(super) fn show(
    ui: &mut egui::Ui,
    app: &ScratchpadApp,
    history: &HistoryUi,
    height: f32,
) -> Option<Action> {
    let shared = shared(ui.ctx());
    let mut state = shared.lock().unwrap_or_else(|e| e.into_inner());
    state.refresh(app, &history.search.to_lowercase(), ui.ctx());
    let mut action = None;
    let mut changed = false;
    let now = navigation::active_now(app);
    let list_height = (height - 64.0).max(ROW_HEIGHT);
    let rail = ui.painter().add(egui::Shape::Noop);
    let top = navigation::header(ui, app, &mut state, now.as_ref());
    if state.scroll_to_selection {
        if let Some(index) = state
            .rows
            .iter()
            .position(|row| Some(row.id()) == state.selected)
        {
            state.offset = (index as f32 * ROW_HEIGHT - height * 0.5).max(0.0);
        }
        state.scroll_to_selection = false;
    }
    navigation::reveal(ui.ctx(), app, &mut state, now.as_ref());
    let mut near_end = false;
    let output = ui
        .scope(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            egui::ScrollArea::vertical()
                .id_salt("history.timeline.scroll")
                .max_height(list_height)
                .auto_shrink([false, false])
                .vertical_scroll_offset(state.offset)
                .show_rows(ui, ROW_HEIGHT, state.rows.len().max(1), |ui, range| {
                    navigation::scroll_target(ui, &state, range.start);
                    near_end = range.end >= state.rows.len();
                    action = paint_visible(
                        ui,
                        &mut state,
                        range,
                        history,
                        now.as_ref().map(|p| &p.id),
                        app,
                    );
                })
        })
        .inner;
    if ui
        .data_mut(|d| d.remove_temp::<bool>(egui::Id::unique("history.operations.rebuild")))
        .unwrap_or(false)
    {
        state.rebuild();
    }
    state.offset = output.state.clamped_offset().y;
    state.viewport_height = output.inner_rect.height();
    navigation::finish_scroll(&mut state);
    let index = (state.offset / ROW_HEIGHT).floor() as usize;
    if state.reveal_now.is_none() && state.jump_end.is_none() {
        state.anchor = state
            .rows
            .get(index)
            .map(|row| (row.id(), state.offset - index as f32 * ROW_HEIGHT));
    }
    if near_end && state.pending.is_none() && state.error.is_none() {
        if state.next.is_some() {
            state.limit += TIMELINE_PAGE_SIZE;
            state.fetch(
                app.state.persistence.session_store.clone(),
                ui.ctx().clone(),
            );
            changed = true;
        } else if state.rows.len() >= state.limit
            && state.live.rows.len() + state.persisted.len() > state.limit
        {
            state.limit += TIMELINE_PAGE_SIZE;
            changed = true;
        }
    }
    if changed {
        state.rebuild();
        ui.ctx().request_repaint();
    }
    let bottom = ui
        .horizontal(|ui| navigation::end_button(ui, &mut state, navigation::End::Oldest))
        .inner;
    if !state.rows.is_empty() {
        ui.painter().set(
            rail,
            egui::Shape::line_segment(
                [
                    egui::pos2(top.center().x, top.center().y + 8.0),
                    egui::pos2(bottom.center().x, bottom.center().y - 8.0),
                ],
                egui::Stroke::new(1.5, theme::tab_selected_accent(ui)),
            ),
        );
    }
    action
}

fn paint_visible(
    ui: &mut egui::Ui,
    state: &mut State,
    range: std::ops::Range<usize>,
    history: &HistoryUi,
    now: Option<&TimelineId>,
    app: &ScratchpadApp,
) -> Option<Action> {
    if state.rows.is_empty() {
        if state.fetched && state.error.is_none() {
            ui.label(if history.search.is_empty() {
                "No recorded changes"
            } else {
                "No history matches your search"
            });
        }
        return None;
    }
    let mut action = None;
    for index in range {
        let picked = match replace_all(&state.rows[index]) {
            Some((operation, files)) => {
                let row = state.rows[index].clone();
                operation_row(ui, state, &row, (operation, files), app)
            }
            None => change_row(ui, state, index, now),
        };
        action = picked.or(action);
    }
    action
}

/// A Replace all row: expands to its edits and offers Undo all.
fn operation_row(
    ui: &mut egui::Ui,
    state: &mut State,
    row: &TimelineRow,
    (operation_id, files): (u64, usize),
    app: &ScratchpadApp,
) -> Option<Action> {
    let expanded = state.expanded_operations.contains(&operation_id);
    let allowed =
        crate::app::app_state::search_replace::transaction::can_undo(app, operation_id, files);
    let undone = operation_undone(app, operation_id, files);
    let mut toggle = false;
    let mut action = None;
    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), ROW_HEIGHT),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            let marker = if expanded { "▾" } else { "▸" };
            let suffix = if undone { " · Undone" } else { "" };
            toggle = ui
                .selectable_label(expanded, format!("{marker} {}{suffix}", row.summary))
                .clicked();
            if ui
                .add_enabled(allowed, egui::Button::new("Undo all"))
                .on_hover_text(UNDO_ALL_HINT)
                .clicked()
            {
                action = Some(Action {
                    operation: Some((operation_id, files)),
                    document: row.cursor.document.clone(),
                    node: None,
                    inspect: false,
                });
            }
        },
    );
    if toggle {
        if expanded {
            state.expanded_operations.remove(&operation_id);
        } else {
            state.expanded_operations.insert(operation_id);
        }
        ui.ctx().request_repaint();
        // Rebuild after the visible range has been consumed.
        ui.data_mut(|d| d.insert_temp(egui::Id::unique("history.operations.rebuild"), true));
    }
    action
}

const UNDO_ALL_HINT: &str = "All affected files must be open and at this edit. Later edits are kept; use per-file History to navigate them first.";

/// Whether every file of the operation now sits just before its edit.
fn operation_undone(app: &ScratchpadApp, operation_id: u64, files: usize) -> bool {
    app.tab_manager
        .tabs
        .as_slice()
        .iter()
        .flat_map(|tab| tab.buffers())
        .filter(|buffer| {
            let document = buffer.document();
            document
                .history_entries()
                .iter()
                .find(|entry| entry.global_seq == operation_id)
                .is_some_and(|entry| document.history_now() == entry.parent_id)
        })
        .count()
        == files
}

/// An edit or activity row. Clicking selects it and asks to show or inspect it.
fn change_row(
    ui: &mut egui::Ui,
    state: &mut State,
    index: usize,
    now: Option<&TimelineId>,
) -> Option<Action> {
    let row = &state.rows[index];
    let id = row.id();
    let visuals = RowState {
        selected: state.selected.as_ref() == Some(&id),
        closed: state
            .source
            .as_ref()
            .is_none_or(|s| s.opened.binary_search(&row.cursor.document).is_err()),
        current: now == Some(&id),
        previous_time: index.checked_sub(1).map(|i| state.rows[i].timestamp),
    };
    let inspect = paint_row(ui, row, &visuals)?;
    let action = Action {
        operation: None,
        document: row.cursor.document.clone(),
        node: if inspect { None } else { row.node() },
        inspect,
    };
    state.selected = Some(id);
    state.jump_end = None;
    state.reveal_now = None;
    Some(action)
}
