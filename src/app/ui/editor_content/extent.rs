use crate::app::ui::scrolling::DisplaySnapshot;

const ROW_HEIGHT_EPSILON: f32 = 0.01;

pub(crate) fn eof_tail_height(viewport_height: f32, row_height: f32) -> f32 {
    let _ = (viewport_height, row_height);
    0.0
}

pub(crate) fn content_height(
    line_count: usize,
    row_height: f32,
    measured_display_height: f32,
    viewport_height: f32,
) -> f32 {
    let row_height = row_height.max(0.0);
    let logical_height = (line_count.max(1) as f32 * row_height).max(row_height);
    (logical_height.max(measured_display_height.max(1.0))
        + eof_tail_height(viewport_height, row_height))
    .ceil()
}

pub(crate) fn based_slice_height(
    logical_line_base: usize,
    row_height: f32,
    slice_height: f32,
) -> f32 {
    logical_line_base as f32 * row_height.max(0.0) + slice_height.max(1.0)
}

pub(crate) fn scroll_content_height(
    line_count: usize,
    row_height: f32,
    viewport_height: f32,
    display_snapshot: Option<&DisplaySnapshot>,
) -> f32 {
    // Soft-wrapped rows are only known after text layout, so the scroll frame
    // uses the latest display snapshot as a lower bound while the native editor
    // uses the same content-height policy for its allocated text rect. Keeping
    // both layers on this shared extent prevents wheel, scrollbar, reveal, and
    // drag-autoscroll paths from clamping to different EOF positions.
    let measured_display_height = display_snapshot
        .and_then(|snapshot| snapshot_height(snapshot, row_height))
        .unwrap_or(0.0);
    content_height(
        line_count,
        row_height,
        measured_display_height,
        viewport_height,
    )
}

fn snapshot_height(snapshot: &DisplaySnapshot, row_height: f32) -> Option<f32> {
    ((snapshot.row_height() - row_height).abs() < ROW_HEIGHT_EPSILON)
        .then_some(snapshot.content_height())
        .filter(|height| height.is_finite() && *height > 0.0)
}

#[cfg(test)]
mod tests {
    use super::{based_slice_height, content_height, eof_tail_height};

    #[test]
    fn eof_tail_does_not_create_blank_scroll_page() {
        assert_eq!(eof_tail_height(600.0, 20.0), 0.0);
    }

    #[test]
    fn based_slice_height_adds_logical_line_offset() {
        assert_eq!(based_slice_height(10, 20.0, 55.0), 255.0);
    }

    #[test]
    fn content_height_uses_measured_display_height_when_it_exceeds_logical_lines() {
        assert_eq!(content_height(10, 20.0, 235.2, 200.0), 236.0);
    }

    #[test]
    fn content_height_falls_back_to_logical_lines_without_measurement() {
        assert_eq!(content_height(10, 20.0, 0.0, 200.0), 200.0);
    }
}
