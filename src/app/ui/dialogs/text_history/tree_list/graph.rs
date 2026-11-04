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
    let key = egui::Id::unique(("history.graph.viewport", buffer.id));
    let previous = ui.ctx().data(|d| d.get_temp::<Slice>(key));
    let slice = if let Some(slice) =
        previous.filter(|s| Arc::ptr_eq(&s.source, &view.edges) && s.range == range)
    {
        slice
    } else {
        let edges = view
            .edges
            .iter()
            .filter(|e| e.a < range.end && e.b >= range.start)
            .cloned()
            .collect::<Vec<_>>()
            .into();
        let slice = Slice {
            source: view.edges.clone(),
            range,
            edges,
        };
        ui.ctx().data_mut(|d| d.insert_temp(key, slice.clone()));
        slice
    };
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
    for edge in slice.edges.iter().filter(|e| e.a <= index && e.b >= index) {
        let top = if index == edge.a {
            rect.center().y
        } else {
            rect.top()
        };
        let bottom = if index == edge.b {
            rect.center().y
        } else {
            rect.bottom()
        };
        let a = egui::pos2(x(edge.lane), top);
        let b = egui::pos2(
            x(if index == edge.b {
                edge.parent_lane
            } else {
                edge.lane
            }),
            bottom,
        );
        ui.painter()
            .line_segment([a, b], egui::Stroke::new(1.5, color(edge.id)));
    }
    let row = &view.tree.rows[view.indices[index]];
    let center = egui::pos2(x(row.lane), rect.center().y);
    if row.id == document.history_now() {
        ui.painter().circle_filled(
            center,
            7.0,
            theme::tab_selected_accent(ui).gamma_multiply(0.28),
        );
        ui.painter().circle_filled(center, 4.0, color(row.id));
    } else {
        ui.painter().circle(
            center,
            3.0,
            if document.is_history_node_applied(row.id) {
                color(row.id)
            } else {
                ui.visuals().panel_fill
            },
            egui::Stroke::new(1.5, color(row.id)),
        );
    }
    x(view.tree.max_lane) - rect.left() + 9.0
}
