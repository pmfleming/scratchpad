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
            let preserve = self
                .source
                .as_ref()
                .is_some_and(|old| old.epoch == source.epoch && old.search == source.search);
            if self
                .source
                .as_ref()
                .is_some_and(|old| old.epoch != source.epoch)
            {
                self.selected = None;
                self.anchor = None;
                self.overlay.clear();
            }
            self.overlay.retain(|_, row| {
                row.name.to_lowercase().contains(search)
                    || row.label().to_lowercase().contains(search)
            });
            if preserve {
                self.persisted.append(&mut self.live.retired);
                self.persisted.retain(|_, row| {
                    row.node().is_none()
                        || source.loaded.binary_search(&row.cursor.document).is_err()
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
            changed = true;
        }
        for event in &app.state.persistence.pending_activity {
            let row = event.timeline_row();
            if (row.name.to_lowercase().contains(search)
                || row.label().to_lowercase().contains(search))
                && !self.overlay.contains_key(&row.cursor)
            {
                self.overlay.insert(row.cursor.clone(), row);
                changed = true;
            }
        }
        let reply = self
            .pending
            .as_ref()
            .and_then(|p| p.lock().unwrap_or_else(|e| e.into_inner()).take());
        if let Some(reply) = reply {
            self.pending = None;
            match reply {
                Ok(page) => {
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
                    changed = true;
                }
                Err(error) => self.error = Some(error),
            }
        }
        if !self.fetched && self.pending.is_none() && self.error.is_none() {
            self.fetch(app.state.persistence.session_store.clone(), ctx.clone());
        }
        if changed {
            self.rebuild();
        }
    }

    fn rebuild(&mut self) {
        let mut memory = self.live.rows.iter().rev().peekable();
        let mut disk = self.persisted.iter().rev().peekable();
        let mut overlay = self.overlay.iter().rev().peekable();
        let mut rows = Vec::new();
        let mut count = 0;
        let mut anchored = self.anchor.is_none();
        while count < self.limit || !anchored || self.jump_end == Some(navigation::End::Oldest) {
            let key = [
                memory.peek().map(|(k, _)| *k),
                overlay.peek().map(|(k, _)| *k),
                disk.peek().map(|(k, _)| *k),
            ]
            .into_iter()
            .flatten()
            .max()
            .cloned();
            let Some(key) = key else {
                break;
            };
            let mut row = None;
            if memory.peek().is_some_and(|(k, _)| **k == key) {
                row = memory.next().map(|(_, r)| r);
            }
            if overlay.peek().is_some_and(|(k, _)| **k == key) {
                let pending = overlay.next().map(|(_, r)| r);
                if row.is_none() {
                    row = pending;
                }
            }
            if disk.peek().is_some_and(|(k, _)| **k == key) {
                let persisted = disk.next().map(|(_, r)| r);
                if row.is_none() {
                    row = persisted;
                }
            }
            let Some(row) = row else {
                break;
            };
            count += 1;
            anchored |= self.anchor.as_ref().is_some_and(|(id, _)| row.id() == *id);
            if self.next.as_ref().is_some_and(|bound| row.cursor < *bound) {
                break;
            }
            rows.push(row.clone());
        }
        self.rows = rows;
        if self.reveal_now.is_none()
            && let Some((id, fraction)) = &self.anchor
            && let Some(index) = self.rows.iter().position(|row| row.id() == *id)
        {
            self.offset = index as f32 * ROW_HEIGHT + fraction;
        }
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
                    action =
                        paint_visible(ui, &mut state, range, history, now.as_ref().map(|p| &p.id));
                })
        })
        .inner;
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
) -> Option<Action> {
    let mut action = None;
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
    for index in range {
        let row = &state.rows[index];
        let visuals = RowState {
            selected: state.selected.as_ref() == Some(&row.id()),
            closed: state
                .source
                .as_ref()
                .is_none_or(|s| s.opened.binary_search(&row.cursor.document).is_err()),
            current: now == Some(&row.id()),
            previous_time: index.checked_sub(1).map(|i| state.rows[i].timestamp),
        };
        if let Some(inspect) = paint_row(ui, row, &visuals) {
            state.selected = Some(row.id());
            action = Some(Action {
                document: row.cursor.document.clone(),
                node: if inspect { None } else { row.node() },
                inspect,
            });
            state.jump_end = None;
            state.reveal_now = None;
        }
    }
    action
}
