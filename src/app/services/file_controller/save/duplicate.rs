use super::{FileController, ScratchpadApp, StatusDomain};
use crate::app::domain::{BufferState, WorkspaceTab};
use std::path::PathBuf;

impl FileController {
    pub(crate) fn save_conflict_as_copy(app: &mut ScratchpadApp, index: usize) -> bool {
        let Some(tab) = app.tab_manager.tabs.as_slice().get(index) else {
            return false;
        };
        let name = super::default_save_as_file_name(&tab.active_buffer().name);
        let Some(path) = crate::app::platform_file::pick_save_as_path(&name) else {
            return false;
        };
        Self::save_copy_to_path(app, index, path)
    }

    pub(super) fn save_copy_to_path(app: &mut ScratchpadApp, index: usize, path: PathBuf) -> bool {
        if Self::activate_existing_path(app, &path).is_some() {
            app.state.status.set_warning_status_in_domain(
                StatusDomain::File,
                "Choose a different path for the copy.",
            );
            return false;
        }
        let Some(tab) = app.tab_manager.tabs.as_slice().get(index) else {
            return false;
        };
        let original = tab.active_buffer();
        let mut copy = BufferState::new(original.name.clone(), original.text(), None);
        copy.replace_format_without_text_change(original.format.clone());
        copy.show_control_chars = original.show_control_chars;
        copy.right_to_left_reading_order = original.right_to_left_reading_order;
        copy.is_dirty = true;
        crate::app::app_state::workspace::lifecycle::append_created_tab(
            app,
            WorkspaceTab::new(copy),
        );
        Self::save_buffer_to_path(app, app.tab_manager.active_tab_index, path, true, None)
    }
}
