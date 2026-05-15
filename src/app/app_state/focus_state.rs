use crate::app::domain::ViewId;

#[derive(Default)]
pub(crate) struct FocusState {
    pending_editor_focus: Option<ViewId>,
}

impl FocusState {
    pub(crate) fn request_focus_for_view(&mut self, view_id: ViewId) {
        self.pending_editor_focus = Some(view_id);
    }

    pub(crate) fn should_focus_view(&self, view_id: ViewId) -> bool {
        self.pending_editor_focus == Some(view_id)
    }

    pub(crate) fn consume_focus_request(&mut self, view_id: ViewId) {
        if self.pending_editor_focus == Some(view_id) {
            self.pending_editor_focus = None;
        }
    }

    pub(crate) fn clear(&mut self) {
        self.pending_editor_focus = None;
    }
}
