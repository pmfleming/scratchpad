//! Missing dirty startup documents recover as untitled; disk conflicts keep their path.
use super::super::ScratchpadApp;
use crate::app::domain::{BufferFreshness, WorkspaceTab};

pub(crate) fn refresh_startup_restore_conflicts(app: &mut ScratchpadApp) {
    let mut changed = false;
    for buffer in app
        .tab_manager
        .tabs
        .as_mut_slice()
        .iter_mut()
        .flat_map(WorkspaceTab::buffers_mut)
    {
        let recover = buffer.freshness == BufferFreshness::MissingOnDisk && buffer.is_dirty;
        if !recover || buffer.path.is_none() {
            continue;
        }
        buffer.recover_as_untitled("recovered");
        changed = true;
    }
    if changed {
        app.tab_manager.rebuild_buffer_tab_index();
        app.tab_manager.mark_session_dirty();
    }
}
