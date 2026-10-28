use super::{ScratchpadApp, restore_text_history_selection, tab_index_for_buffer};
use crate::app::domain::{BufferId, CursorRange, EditorViewState, TextDocument};

pub(crate) fn install_history_document(
    app: &mut ScratchpadApp,
    id: BufferId,
    document: TextDocument,
    follow: bool,
) -> bool {
    let Some(index) = tab_index_for_buffer(app, id) else {
        return false;
    };
    let tab = &mut app.tab_manager.tabs.as_mut_slice()[index];
    let Some(buffer) = tab.buffer_by_id_mut(id) else {
        return false;
    };
    buffer.install_history_document(document);
    // Snapshot roots have different anchor registries and can share generation
    // numbers. Invalidate every view, including split views, not just the active one.
    for view in tab
        .layout
        .views
        .iter_mut()
        .filter(|view| view.buffer_id == id)
    {
        let rtl = view.right_to_left_reading_order;
        *view = EditorViewState::restored(view.id, id, view.show_line_numbers);
        view.right_to_left_reading_order = rtl;
    }
    if follow {
        restore_text_history_selection(app, index, id, CursorRange::default());
    }
    app.state.text_history_cache = crate::app::text_history::TextHistoryCache::default();
    crate::app::app_state::search_runtime::mark_search_dirty(app);
    app.tab_manager.mark_session_dirty();
    app.apply_current_tab_ordering();
    true
}
