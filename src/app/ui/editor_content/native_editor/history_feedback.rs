//! Small, paint-only feedback for live history navigation. No document styling,
//! selection changes or full-text diffs; normal editing invalidates the marker.
use super::{
    CharCursor,
    painting::{EditorFrame, galley_screen_offset},
};
use crate::app::domain::{BufferState, buffer::DocumentRevision};
use eframe::egui;
use std::time::Duration;

const LIFETIME: f64 = 1.4;

#[derive(Clone)]
struct Feedback {
    revision: DocumentRevision,
    offset: usize,
    started: f64,
}

fn key(buffer_id: u64) -> egui::Id {
    egui::Id::unique(("history.editor.feedback", buffer_id))
}

pub(crate) fn mark(ctx: &egui::Context, buffer: &BufferState, offset: usize) {
    let feedback = Feedback {
        revision: buffer.document().revision_key(),
        offset: offset.min(buffer.current_file_length().chars),
        started: ctx.input(|input| input.time),
    };
    ctx.data_mut(|data| data.insert_temp(key(buffer.id), feedback));
    ctx.request_repaint();
}

fn current(ctx: &egui::Context, buffer: &BufferState) -> Option<(usize, f32)> {
    let feedback = ctx.data(|data| data.get_temp::<Feedback>(key(buffer.id)))?;
    let elapsed = ctx.input(|input| input.time) - feedback.started;
    if feedback.revision != buffer.document().revision_key() || elapsed >= LIFETIME {
        ctx.data_mut(|data| data.remove::<Feedback>(key(buffer.id)));
        return None;
    }
    Some((feedback.offset, (1.0 - elapsed.max(0.0) / LIFETIME) as f32))
}

#[cfg(test)]
pub(crate) fn active_offset(ctx: &egui::Context, buffer: &BufferState) -> Option<usize> {
    current(ctx, buffer).map(|(offset, _)| offset)
}

pub(super) fn paint(ui: &egui::Ui, frame: &EditorFrame<'_>) {
    let Some((offset, opacity)) = current(ui.ctx(), frame.buffer) else {
        return;
    };
    // Off-screen edits must not tint the edge of a virtualized slice.
    if offset < frame.char_offset_base
        || offset > frame.char_offset_base.saturating_add(frame.slice_chars)
    {
        return;
    }
    let local = offset - frame.char_offset_base;
    let displayed = frame
        .display_map
        .map_or(local, |map| map.doc_to_display_cursor(local));
    let cursor = frame
        .galley
        .pos_from_cursor(CharCursor::new(displayed).to_egui_ccursor())
        .translate(galley_screen_offset(frame.galley, frame.galley_pos).to_vec2());
    let clip = ui.clip_rect().intersect(frame.rect);
    let line = egui::Rect::from_min_max(
        egui::pos2(clip.left(), cursor.top()),
        egui::pos2(clip.right(), cursor.bottom()),
    )
    .intersect(clip);
    if line.is_positive() {
        ui.painter_at(clip).rect_filled(
            line,
            2.0,
            crate::app::theme::tab_selected_accent(ui).gamma_multiply(0.10 * opacity),
        );
        ui.ctx().request_repaint_after(Duration::from_millis(33));
    }
}

#[cfg(test)]
mod tests {
    use super::{BufferState, current, egui, mark};

    #[test]
    fn feedback_is_buffer_scoped_revision_bound_and_temporary() {
        let ctx = egui::Context::default();
        let mut buffer = BufferState::new("note".into(), "λ\ntext".into(), None);
        let other = BufferState::new("other".into(), "other".into(), None);
        mark(&ctx, &buffer, usize::MAX);
        assert_eq!(current(&ctx, &buffer), Some((6, 1.0)));
        assert!(current(&ctx, &other).is_none());
        buffer.document_mut().insert_direct(0, "changed");
        assert!(current(&ctx, &buffer).is_none());
        mark(&ctx, &buffer, 0);
        ctx.run_ui(
            egui::RawInput {
                time: Some(2.0),
                ..Default::default()
            },
            |_| {},
        )
        .textures_delta
        .clear();
        assert!(current(&ctx, &buffer).is_none());
    }
}
