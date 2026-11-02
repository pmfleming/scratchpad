//! Active-buffer-only, live history navigation.
mod input;
use super::{
    active_buffer, control_icon_button,
    model::{HistoryAction, row_from_entry},
    snapshots,
    states::States,
};
use crate::app::{
    app_state::ScratchpadApp,
    domain::buffer::DocumentRevision,
    theme,
    ui::{callout, settings, widget_ids},
};
use eframe::egui;
use egui_phosphor::regular::{CARET_LEFT, CARET_RIGHT};
pub(crate) use input::note_editor_history_activity;
pub(super) use input::{close, resume_editor_input};

const SCRUBBER_SCALE: f32 = 1.5;
const SCRUBBER_WIDTH: f32 = 360.0 * SCRUBBER_SCALE;
const SCRUBBER_DEFAULT_HEIGHT: f32 = 205.0;

pub(super) fn focus_id() -> egui::Id {
    egui::Id::unique("history.scrubber.focus")
}
fn drag_id() -> egui::Id {
    egui::Id::unique("history.scrubber.drag")
}

#[derive(Clone)]
struct Drag {
    buffer: u64,
    revision: DocumentRevision,
    history_revision: u64,
    position: usize,
    cancelled: bool,
}
impl Drag {
    fn validate(&mut self, buffer: u64, revision: DocumentRevision, history_revision: u64) {
        self.cancelled |= self.buffer != buffer
            || self.revision != revision
            || self.history_revision != history_revision;
    }
}

pub(crate) fn history_scrubbing(ctx: &egui::Context, app: &ScratchpadApp) -> bool {
    if !app.state.dialogs.history_scrubber.is_open() || !ctx.input(|i| i.pointer.any_down()) {
        return false;
    }
    let Some(mut drag) = ctx.data(|d| d.get_temp::<Drag>(drag_id())) else {
        return false;
    };
    let Some(buffer) = app.tab_manager.active_tab().map(|tab| tab.active_buffer()) else {
        return false;
    };
    drag.validate(
        buffer.id,
        buffer.document().revision_key(),
        buffer.document().history_revision_counter(),
    );
    !drag.cancelled
}

struct Track<'a> {
    id: u64,
    revision: DocumentRevision,
    history_revision: u64,
    states: &'a States<'a>,
    enabled: bool,
    markers: Vec<(usize, String)>,
    rect: egui::Rect,
}
impl Track<'_> {
    fn x(&self, k: usize) -> f32 {
        self.rect.left() + self.rect.width() * k as f32 / self.states.entries.len().max(1) as f32
    }
    fn action(&self, position: usize) -> HistoryAction {
        HistoryAction::State {
            buffer_id: self.id,
            after: self.states.after(position),
        }
    }
}

pub(super) fn show(ctx: &egui::Context, app: &mut ScratchpadApp) {
    input::sync_editor_activity(ctx, app);
    if !app.state.dialogs.history_scrubber.is_open() {
        ctx.data_mut(|d| d.remove::<Drag>(drag_id()));
        return;
    }
    let Some(id) = active_buffer(app) else {
        return;
    };
    let screen = ctx.content_rect();
    let position = egui::pos2(
        (screen.right() - SCRUBBER_WIDTH - 24.0).max(screen.left()),
        (screen.bottom() - SCRUBBER_DEFAULT_HEIGHT - 24.0).max(screen.top()),
    );
    let mut action = None;
    let mut close = false;
    callout::show_floating(
        ctx,
        "history_scrubber_window",
        "Scrubber",
        position,
        SCRUBBER_WIDTH,
        |ui| {
            widget_ids::feature_scope(ui, "history_scrubber", |ui| {
                (close, action) = render(ui, app, id);
            });
        },
    );
    if close {
        input::close(ctx, app);
    }
    if let Some(HistoryAction::State { buffer_id, after }) = action {
        let applied = snapshots::go_to_state(ctx, app, buffer_id, after);
        refresh_drag_after_replay(ctx, app, buffer_id, applied);
    }
}

fn refresh_drag_after_replay(
    ctx: &egui::Context,
    app: &ScratchpadApp,
    buffer_id: u64,
    applied: bool,
) {
    ctx.data_mut(|data| {
        let Some(mut drag) = data.get_temp::<Drag>(drag_id()) else {
            return;
        };
        if applied
            && !drag.cancelled
            && drag.buffer == buffer_id
            && let Some(buffer) = app.tab_manager.active_tab().map(|tab| tab.active_buffer())
            && buffer.id == buffer_id
        {
            // Accept only this drag's replay. Subsequent external edits still
            // invalidate the guard on the next frame.
            drag.revision = buffer.document().revision_key();
            drag.history_revision = buffer.document().history_revision_counter();
        } else {
            drag.cancelled = true;
        }
        data.insert_temp(drag_id(), drag);
    });
}

fn render(ui: &mut egui::Ui, app: &mut ScratchpadApp, id: u64) -> (bool, Option<HistoryAction>) {
    settings::apply_dialog_typography(ui);
    callout::apply_spacing(ui);
    snapshots::poll(ui, app);
    snapshots::hydrate(ui, app);
    // A completed snapshot may switch the active file; retarget next frame.
    if active_buffer(app) != Some(id) {
        return (false, None);
    }
    let entries = snapshots::state_entries(ui.ctx(), app);
    let states = States::new(&entries, id);
    let catalog = snapshots::collect(ui, app, &[id]);
    let buffer = app.tab_manager.active_tab().unwrap().active_buffer();
    let close = callout::header_row(ui, "scrubber.header", "Close scrubber", |ui| {
        super::truncated_label(
            ui,
            &buffer.name,
            ui.available_width(),
            14.0,
            callout::text(ui),
            egui::Sense::hover(),
        );
    });
    let visiting = snapshots::is_browsing(ui.ctx(), app, id);
    let rect = widget_ids::allocate_exact_rect(
        ui,
        egui::vec2(ui.available_width(), 42.0 * SCRUBBER_SCALE),
    );
    let track = Track {
        id,
        revision: buffer.document().revision_key(),
        history_revision: buffer.document().history_revision_counter(),
        states: &states,
        enabled: !states.entries.is_empty() && !visiting,
        markers: snapshots::covered_positions(&catalog, id, &states),
        rect: rect.shrink2(egui::vec2(9.0, 14.0)),
    };
    let response = widget_ids::interact(
        ui,
        rect,
        focus_id(),
        egui::Sense::click_and_drag(),
        "history.scrubber.track",
    );
    let mut action = None;
    let preview = interact_track(ui.ctx(), &response, &track, &mut action);
    response.widget_info(|| {
        egui::WidgetInfo::slider(track.enabled, preview as f64, "History position")
    });
    paint_track(ui, &track, preview);
    if response.has_focus() {
        super::lock_navigation_focus(ui.ctx(), focus_id());
        ui.painter().rect_stroke(
            rect,
            6,
            egui::Stroke::new(1.0, theme::tab_selected_accent(ui)),
            egui::StrokeKind::Inside,
        );
    }
    response.on_hover_text(if visiting { "Viewing a snapshot. Return to latest in History before scrubbing." }
        else { "Drag to move through changes in the file. ←/→ step · Home/End jump · Esc close. Shaded ranges cannot be replayed." });
    render_step_controls(ui, &track, preview, &mut action);
    ui.weak(if visiting {
        "Snapshot".to_owned()
    } else {
        format!("{preview} / {}", states.entries.len())
    });
    snapshots::footer(ui, &catalog);
    (close, action)
}

fn interact_track(
    ctx: &egui::Context,
    response: &egui::Response,
    track: &Track<'_>,
    action: &mut Option<HistoryAction>,
) -> usize {
    let mut drag = ctx.data_mut(|d| {
        let drag = d.get_temp::<Drag>(drag_id());
        d.remove::<Drag>(drag_id());
        drag
    });
    if let Some(drag) = &mut drag {
        drag.validate(track.id, track.revision, track.history_revision);
    }
    if track.enabled && response.drag_started() {
        drag = Some(Drag {
            buffer: track.id,
            revision: track.revision,
            history_revision: track.history_revision,
            position: track.states.position,
            cancelled: false,
        });
        response.request_focus();
    }
    let pointer_position = response.interact_pointer_pos().map(|p| {
        (((p.x - track.rect.left()) / track.rect.width() * track.states.entries.len().max(1) as f32)
            .round()
            .max(0.0) as usize)
            .clamp(track.states.first, track.states.last)
    });
    if let Some(drag) = &mut drag {
        if track.enabled && !drag.cancelled {
            if let Some(position) = pointer_position {
                drag.position = position;
            }
            if drag.position != track.states.position {
                *action = Some(track.action(drag.position));
            }
        } else {
            drag.cancelled = true;
        }
    }
    let preview = drag
        .as_ref()
        .filter(|d| !d.cancelled)
        .map_or(track.states.position, |d| d.position);
    if response.drag_stopped() {
        drag = None;
    } else if response.clicked() {
        response.request_focus();
        if track.enabled
            && let Some(position) = pointer_position
        {
            *action = Some(track.action(position));
        }
    }
    if ctx.input(|i| i.pointer.any_down())
        && let Some(drag) = drag
    {
        ctx.data_mut(|d| d.insert_temp(drag_id(), drag));
    }
    preview
}

fn paint_track(ui: &mut egui::Ui, track: &Track<'_>, preview: usize) {
    let states = track.states;
    let total = states.entries.len().max(1);
    let muted = callout::muted_text(ui);
    let y = track.rect.center().y;
    ui.painter().line_segment(
        [egui::pos2(track.x(0), y), egui::pos2(track.x(total), y)],
        egui::Stroke::new(3.0, theme::border(ui)),
    );
    ui.painter().line_segment(
        [
            egui::pos2(track.x(states.first), y),
            egui::pos2(track.x(states.last), y),
        ],
        egui::Stroke::new(3.0, muted),
    );
    // Bound painting work by track pixels, not retained history length.
    let stride = (total / (track.rect.width() / 7.0).max(1.0) as usize).max(1);
    for k in (0..=states.entries.len()).step_by(stride) {
        let center = egui::pos2(track.x(k), track.rect.top() - 5.0);
        ui.painter().circle(
            center,
            2.0,
            if k <= states.position {
                muted
            } else {
                ui.visuals().window_fill()
            },
            egui::Stroke::new(1.0, muted),
        );
    }
    for (k, time) in &track.markers {
        let center = egui::pos2(track.x(*k), track.rect.top() - 5.0);
        snapshots::diamond(ui, center, false);
        widget_ids::interact(
            ui,
            egui::Rect::from_center_size(center, egui::vec2(10.0, 10.0)),
            widget_ids::local(ui, ("marker", k)),
            egui::Sense::hover(),
            "history.scrubber.marker",
        )
        .on_hover_text(format!("Snapshot · {time}"));
    }
    for (start, end) in [(0, states.first), (states.last, states.entries.len())] {
        let mut left = track.x(start);
        while left < track.x(end) {
            ui.painter().line_segment(
                [
                    egui::pos2(left, y - 4.0),
                    egui::pos2((left + 4.0).min(track.x(end)), y + 4.0),
                ],
                egui::Stroke::new(1.0, theme::border(ui)),
            );
            left += 6.0;
        }
    }
    ui.painter().circle_filled(
        egui::pos2(track.x(preview), y),
        6.0 * SCRUBBER_SCALE,
        if track.enabled {
            theme::tab_selected_accent(ui)
        } else {
            muted
        },
    );
}

fn render_step_controls(
    ui: &mut egui::Ui,
    track: &Track<'_>,
    preview: usize,
    action: &mut Option<HistoryAction>,
) {
    let states = track.states;
    ui.horizontal(|ui| {
        if control_icon_button(
            ui,
            "scrub.previous",
            CARET_LEFT,
            "Undo one step",
            false,
            track.enabled && states.position > states.first,
        )
        .clicked()
        {
            *action = Some(track.action(states.position - 1));
            ui.ctx().memory_mut(|m| m.request_focus(focus_id()));
        }
        let width = (ui.available_width() - 44.0).max(0.0);
        let mut job = egui::text::LayoutJob::default();
        if let Some(entry) = preview.checked_sub(1).and_then(|i| states.entries.get(i)) {
            super::list::append_diff(ui, &mut job, &row_from_entry(entry));
        } else {
            job.append(
                if states.entries.is_empty() {
                    "No changes"
                } else {
                    "Oldest retained state"
                },
                0.0,
                egui::TextFormat {
                    color: callout::muted_text(ui),
                    ..Default::default()
                },
            );
        }
        job.wrap.max_width = width;
        job.wrap.max_rows = 1;
        ui.add_sized(egui::vec2(width, 26.0), egui::Label::new(job).truncate());
        if control_icon_button(
            ui,
            "scrub.next",
            CARET_RIGHT,
            "Redo one step",
            false,
            track.enabled && states.position < states.last,
        )
        .clicked()
        {
            *action = Some(track.action(states.position + 1));
            ui.ctx().memory_mut(|m| m.request_focus(focus_id()));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::domain::TextDocument;
    #[test]
    fn drag_cancellation_is_sticky_across_tab_and_revision_changes() {
        let document = TextDocument::new("text".into());
        let revision = document.revision_key();
        let mut drag = Drag {
            buffer: 1,
            revision,
            history_revision: 4,
            position: 0,
            cancelled: false,
        };
        drag.validate(2, revision, 4);
        drag.validate(1, revision, 4);
        assert!(drag.cancelled);
        drag.cancelled = false;
        drag.validate(1, revision, 5);
        assert!(drag.cancelled);
    }
}
