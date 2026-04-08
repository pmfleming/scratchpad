use crate::app::app_state::ScratchpadApp;
use crate::app::domain::ViewId;
use crate::app::theme::*;
use crate::app::ui::editor_content;
use crate::app::ui::tile_header::{self, SplitPreviewOverlay, TileAction};
use eframe::egui;

#[allow(clippy::too_many_arguments)]
pub fn render_tile(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    tab_index: usize,
    view_id: ViewId,
    rect: egui::Rect,
    is_active: bool,
    can_close: bool,
    actions: &mut Vec<TileAction>,
    any_editor_changed: &mut bool,
    preview_overlay: &mut Option<SplitPreviewOverlay>,
) {
    ui.scope_builder(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::top_down(egui::Align::Min)),
        |ui| {
            let tile_response = ui.interact(
                rect,
                ui.make_persistent_id(("tile", tab_index, view_id)),
                egui::Sense::click(),
            );
            if tile_response.clicked() {
                actions.push(TileAction::Activate(view_id));
            }

            let bg = if is_active { HEADER_BG } else { EDITOR_BG };
            ui.painter().rect_filled(rect, 4.0, bg);
            ui.painter().rect_stroke(
                rect,
                4.0,
                egui::Stroke::new(
                    1.0,
                    if is_active {
                        egui::Color32::LIGHT_BLUE
                    } else {
                        BORDER
                    },
                ),
                egui::StrokeKind::Outside,
            );

            render_tile_body(ui, app, tab_index, view_id, rect, any_editor_changed);
            tile_header::render_tile_header(
                ui,
                app,
                tab_index,
                view_id,
                rect,
                can_close,
                actions,
                preview_overlay,
            );
        },
    );
}

fn render_tile_body(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    tab_index: usize,
    view_id: ViewId,
    rect: egui::Rect,
    any_editor_changed: &mut bool,
) {
    ui.scope_builder(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::top_down(egui::Align::Min)),
        |ui| {
            let editor_font_id = egui::FontId::monospace(app.font_size);
            let word_wrap = app.word_wrap;
            let tab = &mut app.tabs_mut()[tab_index];
            let previous_layout = tab
                .view_mut(view_id)
                .and_then(|view| view.latest_layout.take());

            let changed = egui::ScrollArea::both()
                .id_salt(("editor_scroll", tab_index, view_id))
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let (buffer, views) = (&mut tab.buffer, &mut tab.views);
                    if let Some(view) = views.iter_mut().find(|view| view.id == view_id) {
                        editor_content::render_editor_content(
                            ui,
                            buffer,
                            view,
                            previous_layout.as_ref(),
                            word_wrap,
                            &editor_font_id,
                        )
                    } else {
                        false
                    }
                })
                .inner;

            if tab
                .view(view_id)
                .is_some_and(|view| view.latest_layout.is_none())
                && let Some(view) = tab.view_mut(view_id)
            {
                view.latest_layout = previous_layout;
            }

            *any_editor_changed |= changed;
        },
    );
}
