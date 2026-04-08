use crate::app::domain::{SplitAxis, SplitPath};
use crate::app::theme::*;
use crate::app::ui::tile_header::{TileAction, TILE_GAP};
use eframe::egui;

pub const DIVIDER_HIT_THICKNESS: f32 = 18.0;
pub const DIVIDER_VISUAL_THICKNESS: f32 = 2.0;
pub const DIVIDER_HANDLE_MAJOR: f32 = 36.0;
pub const DIVIDER_HANDLE_MINOR: f32 = 20.0;

pub fn render_split_divider(
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

pub fn split_rect(rect: egui::Rect, axis: SplitAxis, ratio: f32) -> (egui::Rect, egui::Rect) {
    match axis {
        SplitAxis::Vertical => {
            let gap_half = TILE_GAP * 0.5;
            let split_x = rect.left() + rect.width() * ratio.clamp(0.2, 0.8);
            (
                egui::Rect::from_min_max(rect.min, egui::pos2(split_x - gap_half, rect.max.y)),
                egui::Rect::from_min_max(egui::pos2(split_x + gap_half, rect.min.y), rect.max),
            )
        }
        SplitAxis::Horizontal => {
            let gap_half = TILE_GAP * 0.5;
            let split_y = rect.top() + rect.height() * ratio.clamp(0.2, 0.8);
            (
                egui::Rect::from_min_max(rect.min, egui::pos2(rect.max.x, split_y - gap_half)),
                egui::Rect::from_min_max(egui::pos2(rect.min.x, split_y + gap_half), rect.max),
            )
        }
    }
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
