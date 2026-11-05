//! A virtualized activity list, independent of per-document branch navigation.
mod model;
#[cfg(test)]
mod tests;

use super::{HistoryUi, presentation, style};
use crate::app::{
    app_state::ScratchpadApp,
    services::session_store::{
        SessionStore,
        timeline::{TIMELINE_PAGE_SIZE, TimelineCursor, TimelineId, TimelinePage, TimelineRow},
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
type PendingPage = Arc<Mutex<Option<Result<TimelinePage, String>>>>;

#[derive(Clone, PartialEq, Eq)]
struct Source {
    loaded: Vec<String>,
    search: String,
    epoch: u64,
}

enum Row {
    Date(String),
    Edit(TimelineRow),
}

struct State {
    live: model::LiveRows,
    source: Option<Source>,
    persisted: BTreeMap<TimelineCursor, TimelineRow>,
    pending: Option<PendingPage>,
    next: Option<TimelineCursor>,
    fetched: bool,
    error: Option<String>,
    rows: Vec<Row>,
    limit: usize,
    selected: Option<TimelineId>,
    offset: f32,
    anchor: Option<(TimelineId, f32)>,
    scroll_to_selection: bool,
}

impl Default for State {
    fn default() -> Self {
        Self {
            live: model::LiveRows::default(),
            source: None,
            persisted: BTreeMap::new(),
            pending: None,
            next: None,
            fetched: false,
            error: None,
            rows: Vec::new(),
            limit: TIMELINE_PAGE_SIZE,
            selected: None,
            offset: 0.0,
            anchor: None,
            scroll_to_selection: false,
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
        let source = Source {
            loaded,
            search: search.to_owned(),
            epoch: app.state.persistence.session_store.history_epoch(),
        };
        if self.source.as_ref() != Some(&source) {
            if self
                .source
                .as_ref()
                .is_some_and(|old| old.epoch != source.epoch)
            {
                self.selected = None;
                self.anchor = None;
            }
            self.source = Some(source);
            self.persisted.clear();
            self.pending = None;
            self.next = None;
            self.fetched = false;
            self.error = None;
            changed = true;
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
                        self.persisted.insert(row.cursor.clone(), row);
                    }
                    self.next = page.older;
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
        let mut rows = Vec::new();
        let mut day = String::new();
        for _ in 0..self.limit {
            let row = match (memory.peek(), disk.peek()) {
                (Some((a, _)), Some((b, _))) if a >= b => memory.next().map(|(_, r)| r),
                (_, Some(_)) => disk.next().map(|(_, r)| r),
                (Some(_), None) => memory.next().map(|(_, r)| r),
                (None, None) => None,
            };
            let Some(row) = row else {
                break;
            };
            if self.next.as_ref().is_some_and(|bound| row.cursor < *bound) {
                break;
            }
            let date = date(row.timestamp);
            if date != day {
                day.clone_from(&date);
                rows.push(Row::Date(date));
            }
            rows.push(Row::Edit(row.clone()));
        }
        self.rows = rows;
        if let Some((id, fraction)) = &self.anchor
            && let Some(index) = self
                .rows
                .iter()
                .position(|r| matches!(r,Row::Edit(row) if row.id()==*id))
        {
            self.offset = index as f32 * ROW_HEIGHT + fraction;
        }
    }
}

fn date(timestamp: u64) -> String {
    chrono::DateTime::from_timestamp(timestamp as i64, 0).map_or_else(
        || "Unknown date".into(),
        |d| {
            d.with_timezone(&chrono::Local)
                .format("%A, %-d %B %Y")
                .to_string()
        },
    )
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
        .filter_map(|(i, r)| matches!(r, Row::Edit(_)).then_some(i))
        .collect();
    if indices.is_empty() {
        ctx.input_mut(|i| {
            i.consume_key(egui::Modifiers::NONE, egui::Key::Enter);
        });
        return None;
    }
    let mut position = indices
        .iter()
        .position(|&i| matches!(&state.rows[i],Row::Edit(r) if Some(r.id())==state.selected));
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
        let Row::Edit(row) = &state.rows[indices[position?]] else {
            return None;
        };
        let action = Action {
            document: row.cursor.document.clone(),
            node: Some(row.cursor.node),
            inspect: false,
        };
        state.selected = Some(row.id());
        state.scroll_to_selection = true;
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
    ui.horizontal(|ui| {
        if ui.button("Latest").clicked() {
            state.offset = 0.0;
            state.anchor = None;
        }
        if state.pending.is_some() {
            ui.spinner();
            ui.label("Loading history…");
        }
        if let Some(error) = state.error.clone() {
            ui.colored_label(ui.visuals().error_fg_color, error);
            if ui.button("Retry").clicked() {
                state.fetch(
                    app.state.persistence.session_store.clone(),
                    ui.ctx().clone(),
                );
            }
        }
    });
    if state.scroll_to_selection {
        if let Some(index) = state
            .rows
            .iter()
            .position(|r| matches!(r,Row::Edit(row) if Some(row.id())==state.selected))
        {
            state.offset = (index as f32 * ROW_HEIGHT - height * 0.5).max(0.0);
        }
        state.scroll_to_selection = false;
    }
    let mut near_end = false;
    let output = ui
        .scope(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            egui::ScrollArea::vertical()
                .id_salt("history.timeline.scroll")
                .max_height((height - 32.0).max(ROW_HEIGHT))
                .auto_shrink([false, false])
                .vertical_scroll_offset(state.offset)
                .show_rows(ui, ROW_HEIGHT, state.rows.len().max(1), |ui, range| {
                    near_end = range.end >= state.rows.len();
                    action = paint_visible(ui, &mut state, range, history);
                })
        })
        .inner;
    state.offset = output.state.clamped_offset().y;
    let index = (state.offset / ROW_HEIGHT).floor() as usize;
    state.anchor = state
        .rows
        .iter()
        .enumerate()
        .skip(index)
        .find_map(|(i, r)| match r {
            Row::Edit(row) => Some((row.id(), state.offset - i as f32 * ROW_HEIGHT)),
            Row::Date(_) => None,
        });
    if near_end && state.pending.is_none() && state.error.is_none() {
        if state.next.is_some() {
            state.limit += TIMELINE_PAGE_SIZE;
            state.fetch(
                app.state.persistence.session_store.clone(),
                ui.ctx().clone(),
            );
            changed = true;
        } else if state
            .rows
            .iter()
            .filter(|r| matches!(r, Row::Edit(_)))
            .count()
            >= state.limit
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
    action
}

fn paint_visible(
    ui: &mut egui::Ui,
    state: &mut State,
    range: std::ops::Range<usize>,
    history: &HistoryUi,
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
        match &state.rows[index] {
            Row::Date(date) => {
                let rect = widget_ids::allocate_exact_rect(
                    ui,
                    egui::vec2(ui.available_width(), ROW_HEIGHT),
                );
                style::text(
                    ui,
                    rect.shrink2(egui::vec2(8.0, 0.0)),
                    date,
                    11.5,
                    theme::text_muted(ui),
                );
            }
            Row::Edit(row) => {
                let row = row.clone();
                let selected = state.selected.as_ref() == Some(&row.id());
                let closed = state
                    .source
                    .as_ref()
                    .is_none_or(|s| s.loaded.binary_search(&row.cursor.document).is_err());
                if let Some(inspect) = paint_row(ui, &row, selected, closed) {
                    state.selected = Some(row.id());
                    action = Some(Action {
                        document: row.cursor.document.clone(),
                        node: (!inspect).then_some(row.cursor.node),
                        inspect,
                    });
                }
            }
        }
    }
    action
}

fn paint_row(ui: &mut egui::Ui, row: &TimelineRow, selected: bool, closed: bool) -> Option<bool> {
    let rect = widget_ids::allocate_exact_rect(ui, egui::vec2(ui.available_width(), ROW_HEIGHT));
    let response = widget_ids::interact(
        ui,
        rect,
        egui::Id::unique(("history.timeline.row", row.id())),
        egui::Sense::click(),
        "timeline-row",
    );
    let label = format!(
        "{} · {}{}",
        row.summary,
        row.name,
        if closed { " · Closed" } else { "" }
    );
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::Role::ListBoxOption, true, selected, &label)
    });
    if selected || response.hovered() {
        ui.painter().rect_filled(
            rect,
            0,
            if selected {
                theme::tab_selected_bg(ui)
            } else {
                theme::action_bg(ui)
            },
        );
    }
    paint_labels(ui, row, rect, closed);
    let right = rect.right() - 80.0;
    let button = widget_ids::scope(ui, ("history.timeline.file", row.id()), |ui| {
        ui.put(
            egui::Rect::from_min_max(egui::pos2(right, rect.top() + 21.0), rect.max),
            egui::Button::new(egui::RichText::new("This file").size(10.5)).frame(false),
        )
    })
    .inner;
    button.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::Role::Button,
            true,
            format!("Inspect branches for {}", row.name),
        )
    });
    if button.clicked() {
        return Some(true);
    }
    response
        .on_hover_text(format!(
            "{}\n{}\n{}",
            row.summary,
            row.label(),
            presentation::full_time(row.timestamp)
        ))
        .clicked()
        .then_some(false)
}

fn paint_labels(ui: &egui::Ui, row: &TimelineRow, rect: egui::Rect, closed: bool) {
    let icon = match row.source {
        crate::app::domain::buffer::PieceSource::External => {
            egui_phosphor::regular::ARROWS_CLOCKWISE
        }
        _ => egui_phosphor::regular::PENCIL_SIMPLE,
    };
    style::text(
        ui,
        egui::Rect::from_min_size(
            rect.min + egui::vec2(8.0, 0.0),
            egui::vec2(20.0, ROW_HEIGHT),
        ),
        icon,
        14.0,
        theme::tab_selected_accent(ui),
    );
    let right = rect.right() - 80.0;
    style::text(
        ui,
        egui::Rect::from_min_max(
            rect.min + egui::vec2(32.0, 2.0),
            egui::pos2(right, rect.top() + 23.0),
        ),
        &row.summary,
        13.0,
        theme::text_primary(ui),
    );
    let file = format!("{}{}", row.label(), if closed { " · Closed" } else { "" });
    style::text_end(
        ui,
        egui::Rect::from_min_max(
            rect.min + egui::vec2(32.0, 22.0),
            egui::pos2(right, rect.bottom() - 2.0),
        ),
        &file,
        11.0,
        theme::text_muted(ui),
    );
    let time = chrono::DateTime::from_timestamp(row.timestamp as i64, 0)
        .map_or_else(String::new, |d| {
            d.with_timezone(&chrono::Local).format("%H:%M").to_string()
        });
    style::text_right(
        ui,
        egui::Rect::from_min_max(
            egui::pos2(right, rect.top()),
            egui::pos2(rect.right() - 8.0, rect.top() + 21.0),
        ),
        &time,
        11.0,
        theme::text_muted(ui),
    );
}
