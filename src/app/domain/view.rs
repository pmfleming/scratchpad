use crate::app::domain::{BufferId, RenderedLayout};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_VIEW_ID: AtomicU64 = AtomicU64::new(1);

pub type ViewId = u64;

#[derive(Clone)]
pub struct EditorViewState {
    pub id: ViewId,
    pub buffer_id: BufferId,
    pub show_line_numbers: bool,
    pub show_control_chars: bool,
    pub latest_layout: Option<RenderedLayout>,
}

impl EditorViewState {
    pub fn new(buffer_id: BufferId, show_control_chars: bool) -> Self {
        Self {
            id: next_view_id(),
            buffer_id,
            show_line_numbers: false,
            show_control_chars,
            latest_layout: None,
        }
    }

    pub fn restored(
        id: ViewId,
        buffer_id: BufferId,
        show_line_numbers: bool,
        show_control_chars: bool,
    ) -> Self {
        register_existing_view_id(id);
        Self {
            id,
            buffer_id,
            show_line_numbers,
            show_control_chars,
            latest_layout: None,
        }
    }
}

pub fn next_view_id() -> ViewId {
    NEXT_VIEW_ID.fetch_add(1, Ordering::Relaxed)
}

fn register_existing_view_id(id: ViewId) {
    let next_id = id.saturating_add(1);
    let mut current = NEXT_VIEW_ID.load(Ordering::Relaxed);

    while current < next_id {
        match NEXT_VIEW_ID.compare_exchange(current, next_id, Ordering::Relaxed, Ordering::Relaxed)
        {
            Ok(_) => break,
            Err(observed) => current = observed,
        }
    }
}
