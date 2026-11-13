use super::{Arc, BufferState, Edge, Prepared, egui};
use crate::app::theme;
use std::ops::Range;

#[derive(Clone)]
struct Slice {
    source: Arc<[Edge]>,
    range: Range<usize>,
    edges: Arc<[Edge]>,
}

pub(super) fn paint(
    ui: &egui::Ui,
    buffer: &BufferState,
    prepared: &Prepared,
    index: usize,
    range: Range<usize>,
    rect: egui::Rect,
) -> f32 {
    let view = &prepared.view;
    let edges = visible_edges(ui.ctx(), buffer, &view.edges, range);
    let step = (96.0 / view.tree.max_lane.max(1) as f32).min(12.0);
    let x = |lane| rect.left() + 11.0 + lane as f32 * step;
    let document = buffer.document();
    let color = |id| {
        if document.is_history_node_applied(id) {
            theme::tab_selected_accent(ui)
        } else if document.is_on_history_route(id) {
            theme::text_muted(ui)
        } else {
            theme::border(ui)
        }
    };
    for edge in edges.iter().filter(|e| e.a <= index && e.b >= index) {
        // An edge starts at its child's dot and bends into its parent's lane at the parent.
        let top = if index == edge.a {
            rect.center().y
        } else {
            rect.top()
        };
        let (bottom, bottom_lane) = if index == edge.b {
            (rect.center().y, edge.parent_lane)
        } else {
            (rect.bottom(), edge.lane)
        };
        ui.painter().line_segment(
            [
                egui::pos2(x(edge.lane), top),
                egui::pos2(x(bottom_lane), bottom),
            ],
            egui::Stroke::new(1.5, color(edge.id)),
        );
    }
    let row = &view.tree.rows[view.indices[index]];
    let center = egui::pos2(x(row.lane), rect.center().y);
    let painter = ui.painter();
    if row.id == document.history_now() {
        painter.circle_filled(
            center,
            7.0,
            theme::tab_selected_accent(ui).gamma_multiply(0.28),
        );
        painter.circle_filled(center, 4.0, color(row.id));
    } else {
        let fill = if document.is_history_node_applied(row.id) {
            color(row.id)
        } else {
            ui.visuals().panel_fill
        };
        painter.circle(center, 3.0, fill, egui::Stroke::new(1.5, color(row.id)));
    }
    x(view.tree.max_lane) - rect.left() + 9.0
}

/// The edges crossing the visible rows, cached until the rows or the graph change.
fn visible_edges(
    ctx: &egui::Context,
    buffer: &BufferState,
    all: &Arc<[Edge]>,
    range: Range<usize>,
) -> Arc<[Edge]> {
    let key = egui::Id::unique(("history.graph.viewport", buffer.id));
    if let Some(slice) = ctx
        .data(|d| d.get_temp::<Slice>(key))
        .filter(|s| Arc::ptr_eq(&s.source, all) && s.range == range)
    {
        return slice.edges;
    }
    let edges: Arc<[Edge]> = all
        .iter()
        .filter(|e| e.a < range.end && e.b >= range.start)
        .cloned()
        .collect::<Vec<_>>()
        .into();
    let slice = Slice {
        source: all.clone(),
        range,
        edges: edges.clone(),
    };
    ctx.data_mut(|d| d.insert_temp(key, slice));
    edges
}
