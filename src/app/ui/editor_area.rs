use crate::app::app_state::ScratchpadApp;
use crate::app::domain::{PaneBranch, PaneNode, SplitAxis, SplitPath, ViewId};
use crate::app::theme::*;
use crate::app::ui::editor_content;
use crate::app::ui::tile_header::{self, SplitPreviewOverlay, TileAction};
use eframe::egui;

const DIVIDER_HIT_THICKNESS: f32 = 18.0;
const DIVIDER_VISUAL_THICKNESS: f32 = 2.0;
const DIVIDER_HANDLE_MAJOR: f32 = 36.0;
const DIVIDER_HANDLE_MINOR: f32 = 20.0;

pub(crate) fn show_editor(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    egui::CentralPanel::default().show_inside(ui, |ui| {
        if app.tabs.is_empty() {
            return;
        }

        let ctx = ui.ctx().clone();
        handle_editor_zoom(&ctx, ui, app);
        app.active_tab_index = app.active_tab_index.min(app.tabs.len() - 1);

        let active_tab_index = app.active_tab_index;
        let pane_tree = app.tabs[active_tab_index].root_pane.clone();
        let active_view_id = app.tabs[active_tab_index].active_view_id;
        let leaf_count = pane_tree.leaf_count();
        let mut actions = Vec::new();
        let mut any_editor_changed = false;
        let mut preview_overlay = None;

        render_pane_node(
            ui,
            app,
            active_tab_index,
            &pane_tree,
            Vec::new(),
            ui.max_rect(),
            active_view_id,
            leaf_count,
            &mut actions,
            &mut any_editor_changed,
            &mut preview_overlay,
        );

        if let Some(preview_overlay) = preview_overlay {
            tile_header::paint_split_preview(ui, &preview_overlay);
        }

        for action in actions {
            match action {
                TileAction::Activate(view_id) => app.activate_view(view_id),
                TileAction::Close(view_id) => app.close_view(view_id),
                TileAction::ResizeSplit { path, ratio } => app.resize_split(path, ratio),
                TileAction::Split {
                    axis,
                    new_view_first,
                    ratio,
                } => app.split_active_view_with_placement(axis, new_view_first, ratio),
            }
        }

        if any_editor_changed {
            apply_editor_change(app, active_tab_index);
        }
    });
}

fn handle_editor_zoom(ctx: &egui::Context, ui: &egui::Ui, app: &mut ScratchpadApp) {
    let panel_rect = ui.max_rect();
    let pointer_over_editor = ui.rect_contains_pointer(panel_rect);
    let zoom_factor = ctx.input(|input| input.zoom_delta());
    if pointer_over_editor && zoom_factor != 1.0 {
        app.font_size = (app.font_size * zoom_factor).clamp(8.0, 72.0);
        app.mark_session_dirty();
    }
}

#[allow(clippy::too_many_arguments)]
fn render_pane_node(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    tab_index: usize,
    node: &PaneNode,
    path: SplitPath,
    rect: egui::Rect,
    active_view_id: ViewId,
    leaf_count: usize,
    actions: &mut Vec<TileAction>,
    any_editor_changed: &mut bool,
    preview_overlay: &mut Option<SplitPreviewOverlay>,
) {
    match node {
        PaneNode::Leaf { view_id } => render_tile(
            ui,
            app,
            tab_index,
            *view_id,
            rect,
            *view_id == active_view_id,
            leaf_count > 1,
            actions,
            any_editor_changed,
            preview_overlay,
        ),
        PaneNode::Split {
            axis,
            ratio,
            first,
            second,
        } => {
            let current_path = path;
            let (first_rect, second_rect) = split_rect(rect, *axis, *ratio);
            let mut first_path = current_path.clone();
            first_path.push(PaneBranch::First);
            render_pane_node(
                ui,
                app,
                tab_index,
                first,
                first_path,
                first_rect,
                active_view_id,
                leaf_count,
                actions,
                any_editor_changed,
                preview_overlay,
            );
            let mut second_path = current_path.clone();
            second_path.push(PaneBranch::Second);
            render_pane_node(
                ui,
                app,
                tab_index,
                second,
                second_path,
                second_rect,
                active_view_id,
                leaf_count,
                actions,
                any_editor_changed,
                preview_overlay,
            );
            render_split_divider(ui, rect, *axis, *ratio, current_path, actions);
        }
    }
}

fn split_rect(rect: egui::Rect, axis: SplitAxis, ratio: f32) -> (egui::Rect, egui::Rect) {
    match axis {
        SplitAxis::Vertical => {
            let gap_half = tile_header::TILE_GAP * 0.5;
            let split_x = rect.left() + rect.width() * ratio.clamp(0.2, 0.8);
            (
                egui::Rect::from_min_max(rect.min, egui::pos2(split_x - gap_half, rect.max.y)),
                egui::Rect::from_min_max(egui::pos2(split_x + gap_half, rect.min.y), rect.max),
            )
        }
        SplitAxis::Horizontal => {
            let gap_half = tile_header::TILE_GAP * 0.5;
            let split_y = rect.top() + rect.height() * ratio.clamp(0.2, 0.8);
            (
                egui::Rect::from_min_max(rect.min, egui::pos2(rect.max.x, split_y - gap_half)),
                egui::Rect::from_min_max(egui::pos2(rect.min.x, split_y + gap_half), rect.max),
            )
        }
    }
}

fn render_split_divider(
    ui: &egui::Ui,
    rect: egui::Rect,
    axis: SplitAxis,
    ratio: f32,
    path: SplitPath,
    actions: &mut Vec<TileAction>,
) {
    let hover_cursor = match axis {
        SplitAxis::Vertical => egui::CursorIcon::ResizeHorizontal,
        SplitAxis::Horizontal => egui::CursorIcon::ResizeVertical,
    };
    let response = ui
        .interact(
            divider_hit_rect(rect, axis, ratio),
            ui.make_persistent_id(("split_divider", &path)),
            egui::Sense::click_and_drag(),
        )
        .on_hover_cursor(hover_cursor);

    if response.dragged()
        && let Some(pointer_pos) = response.interact_pointer_pos()
    {
        actions.push(TileAction::ResizeSplit {
            path,
            ratio: split_ratio_from_pointer(rect, axis, pointer_pos),
        });
    }

    let painter = ui.painter();
    let divider_center = divider_center(rect, axis, ratio);
    let divider_hovered = response.hovered() || response.dragged();
    let line_fill = if divider_hovered {
        egui::Color32::from_rgb(104, 154, 232)
    } else {
        BORDER
    };
    let handle_fill = if divider_hovered {
        egui::Color32::from_rgb(56, 72, 98)
    } else {
        HEADER_BG.gamma_multiply(0.92)
    };
    let handle_rect = divider_handle_rect(divider_center, axis);
    let icon = match axis {
        SplitAxis::Vertical => egui_phosphor::regular::DOTS_SIX_VERTICAL,
        SplitAxis::Horizontal => egui_phosphor::regular::DOTS_SIX,
    };

    match axis {
        SplitAxis::Vertical => {
            let line_rect = egui::Rect::from_center_size(
                divider_center,
                egui::vec2(DIVIDER_VISUAL_THICKNESS, rect.height()),
            );
            painter.rect_filled(line_rect, 0.0, line_fill);
        }
        SplitAxis::Horizontal => {
            let line_rect = egui::Rect::from_center_size(
                divider_center,
                egui::vec2(rect.width(), DIVIDER_VISUAL_THICKNESS),
            );
            painter.rect_filled(line_rect, 0.0, line_fill);
        }
    }

    painter.rect_filled(handle_rect, 6.0, handle_fill);
    painter.rect_stroke(
        handle_rect,
        6.0,
        egui::Stroke::new(1.0, line_fill.gamma_multiply(0.9)),
        egui::StrokeKind::Outside,
    );
    painter.text(
        handle_rect.center(),
        egui::Align2::CENTER_CENTER,
        icon,
        egui::FontId::proportional(14.0),
        TEXT_PRIMARY,
    );
}

fn divider_center(rect: egui::Rect, axis: SplitAxis, ratio: f32) -> egui::Pos2 {
    match axis {
        SplitAxis::Vertical => egui::pos2(
            rect.left() + rect.width() * ratio.clamp(0.2, 0.8),
            rect.center().y,
        ),
        SplitAxis::Horizontal => egui::pos2(
            rect.center().x,
            rect.top() + rect.height() * ratio.clamp(0.2, 0.8),
        ),
    }
}

fn divider_hit_rect(rect: egui::Rect, axis: SplitAxis, ratio: f32) -> egui::Rect {
    let center = divider_center(rect, axis, ratio);
    match axis {
        SplitAxis::Vertical => {
            egui::Rect::from_center_size(center, egui::vec2(DIVIDER_HIT_THICKNESS, rect.height()))
        }
        SplitAxis::Horizontal => {
            egui::Rect::from_center_size(center, egui::vec2(rect.width(), DIVIDER_HIT_THICKNESS))
        }
    }
}

fn divider_handle_rect(center: egui::Pos2, axis: SplitAxis) -> egui::Rect {
    match axis {
        SplitAxis::Vertical => egui::Rect::from_center_size(
            center,
            egui::vec2(DIVIDER_HANDLE_MINOR, DIVIDER_HANDLE_MAJOR),
        ),
        SplitAxis::Horizontal => egui::Rect::from_center_size(
            center,
            egui::vec2(DIVIDER_HANDLE_MAJOR, DIVIDER_HANDLE_MINOR),
        ),
    }
}

fn split_ratio_from_pointer(rect: egui::Rect, axis: SplitAxis, pointer_pos: egui::Pos2) -> f32 {
    match axis {
        SplitAxis::Vertical => ((pointer_pos.x - rect.left()) / rect.width()).clamp(0.2, 0.8),
        SplitAxis::Horizontal => ((pointer_pos.y - rect.top()) / rect.height()).clamp(0.2, 0.8),
    }
}

#[allow(clippy::too_many_arguments)]
fn render_tile(
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

            let body_rect = rect;
            render_tile_body(ui, app, tab_index, view_id, body_rect, any_editor_changed);
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
            let tab = &mut app.tabs[tab_index];
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

fn apply_editor_change(app: &mut ScratchpadApp, active_tab_index: usize) {
    let tab = &mut app.tabs[active_tab_index];
    tab.buffer.refresh_text_metadata();
    let has_control_chars = tab.buffer.artifact_summary.has_control_chars();
    for view in &mut tab.views {
        if !has_control_chars {
            view.show_control_chars = false;
        }
    }
    tab.buffer.is_dirty = true;
    app.status_message = tab
        .buffer
        .artifact_summary
        .status_text()
        .map(|message| format!("Formatting artifacts remain: {message}"));
    app.mark_session_dirty();
}
