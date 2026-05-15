use super::super::{ScratchpadApp, StatusDomain};
use crate::app::domain::CursorRevealMode;
use crate::app::ui::editor_content::native_editor::{
    CharCursor, CursorRange, cut_selected_text, delete_selected_text, select_all_cursor,
    selected_text,
};

pub(crate) fn active_buffer_transaction_label(app: &ScratchpadApp) -> Option<String> {
    app.tab_manager.active_tab().map(|tab| {
        tab.active_buffer()
            .path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| tab.active_buffer().name.clone())
    })
}

pub(crate) fn active_buffer_can_undo_text_operation(app: &ScratchpadApp) -> bool {
    app.tab_manager
        .active_tab()
        .is_some_and(|tab| tab.active_buffer().document().operation_undo_depth() > 0)
}

pub(crate) fn active_buffer_can_redo_text_operation(app: &ScratchpadApp) -> bool {
    app.tab_manager
        .active_tab()
        .is_some_and(|tab| tab.active_buffer().document().operation_redo_depth() > 0)
}

pub(crate) fn undo_active_buffer_text_operation(app: &mut ScratchpadApp) -> bool {
    apply_active_buffer_text_operation(app, true)
}

pub(crate) fn redo_active_buffer_text_operation(app: &mut ScratchpadApp) -> bool {
    apply_active_buffer_text_operation(app, false)
}

pub(crate) fn select_all_in_active_view(app: &mut ScratchpadApp) -> bool {
    let active_tab_index = app.tab_manager.active_tab_index;
    let (total_chars, active_view_id) = match app.tab_manager.active_tab() {
        Some(tab) => (
            tab.active_buffer().current_file_length().chars,
            tab.layout.active_view_id,
        ),
        None => return false,
    };
    let selection = select_all_cursor(total_chars);

    let tab = &mut app.tab_manager.tabs.as_mut_slice()[active_tab_index];
    let Some((buffer, view)) = tab.buffer_and_view_mut(active_view_id) else {
        return false;
    };
    view.set_cursor_range_anchored(buffer, selection);
    view.set_pending_cursor_range_anchored(buffer, selection);
    view.request_cursor_reveal(CursorRevealMode::Center);
    tab.active_buffer_mut().active_selection =
        (!selection.is_empty()).then_some(selection.as_sorted_char_range());
    true
}

pub(crate) fn copy_selected_text_in_active_view(app: &ScratchpadApp) -> Option<String> {
    let tab = app.tab_manager.active_tab()?;
    let view = tab.active_view()?;
    let buffer = tab.buffer_for_view(view.id)?;
    selected_text(buffer, view.cursor_range?)
}

pub(crate) fn cut_selected_text_in_active_view(app: &mut ScratchpadApp) -> Option<String> {
    let active_tab_index = app.tab_manager.active_tab_index;
    let active_view_id = app.tab_manager.active_tab()?.layout.active_view_id;
    let (next_selection, selected_text) =
        cut_active_view_selection(app, active_tab_index, active_view_id)?;

    crate::app::app_state::workspace::mutation::finalize_active_buffer_text_mutation(
        app,
        active_tab_index,
    );
    crate::app::app_state::search_runtime::refresh_search_state(app);
    crate::app::app_state::search_visual::select_next_active_buffer_match_from(
        app,
        next_selection.primary.index,
    );
    Some(selected_text)
}

pub(crate) fn delete_selected_text_in_active_view(app: &mut ScratchpadApp) -> bool {
    let active_tab_index = app.tab_manager.active_tab_index;
    let active_view_id = match app.tab_manager.active_tab() {
        Some(tab) => tab.layout.active_view_id,
        None => return false,
    };
    let Some(next_selection) = delete_active_view_selection(app, active_tab_index, active_view_id)
    else {
        return false;
    };

    crate::app::app_state::workspace::mutation::finalize_active_buffer_text_mutation(
        app,
        active_tab_index,
    );
    crate::app::app_state::search_runtime::refresh_search_state(app);
    crate::app::app_state::search_visual::select_next_active_buffer_match_from(
        app,
        next_selection.primary.index,
    );
    true
}

pub(crate) fn insert_text_in_active_view(app: &mut ScratchpadApp, text: &str) -> bool {
    let active_tab_index = app.tab_manager.active_tab_index;
    let active_view_id = match app.tab_manager.active_tab() {
        Some(tab) => tab.layout.active_view_id,
        None => return false,
    };
    let inserted_chars = text.chars().count();
    let next_selection = {
        let tab = &mut app.tab_manager.tabs.as_mut_slice()[active_tab_index];
        let Some((buffer, view)) = tab.buffer_and_view_mut(active_view_id) else {
            return false;
        };
        let total_chars = buffer.current_file_length().chars;
        let current_selection = view
            .cursor_range
            .unwrap_or_else(|| CursorRange::one(CharCursor::new(total_chars)));
        let (start, end) = current_selection.sorted_indices();
        let next_selection = CursorRange::one(CharCursor::new(start + inserted_chars));
        let replacements = [(start..end, text.to_owned())];
        if buffer
            .replace_char_ranges_with_undo(&replacements, current_selection, next_selection)
            .is_err()
        {
            return false;
        }
        view.set_cursor_range_anchored(buffer, next_selection);
        view.set_pending_cursor_range_anchored(buffer, next_selection);
        view.request_cursor_reveal(CursorRevealMode::KeepVisible);
        buffer.active_selection = None;
        next_selection
    };

    crate::app::app_state::workspace::mutation::finalize_active_buffer_text_mutation(
        app,
        active_tab_index,
    );
    crate::app::app_state::search_runtime::refresh_search_state(app);
    crate::app::app_state::search_visual::select_next_active_buffer_match_from(
        app,
        next_selection.primary.index,
    );
    true
}

fn cut_active_view_selection(
    app: &mut ScratchpadApp,
    active_tab_index: usize,
    active_view_id: crate::app::domain::ViewId,
) -> Option<(
    crate::app::ui::editor_content::native_editor::CursorRange,
    String,
)> {
    let tab = &mut app.tab_manager.tabs.as_mut_slice()[active_tab_index];
    let (buffer, view) = tab.buffer_and_view_mut(active_view_id)?;
    let current_selection = view.cursor_range?;
    let (next_selection, selected_text) = cut_selected_text(buffer, current_selection)?;
    view.set_cursor_range_anchored(buffer, next_selection);
    view.set_pending_cursor_range_anchored(buffer, next_selection);
    view.request_cursor_reveal(CursorRevealMode::KeepVisible);
    buffer.active_selection = None;
    Some((next_selection, selected_text))
}

fn delete_active_view_selection(
    app: &mut ScratchpadApp,
    active_tab_index: usize,
    active_view_id: crate::app::domain::ViewId,
) -> Option<crate::app::ui::editor_content::native_editor::CursorRange> {
    let tab = &mut app.tab_manager.tabs.as_mut_slice()[active_tab_index];
    let (buffer, view) = tab.buffer_and_view_mut(active_view_id)?;
    let current_selection = view.cursor_range?;
    let next_selection = delete_selected_text(buffer, current_selection)?;
    view.set_cursor_range_anchored(buffer, next_selection);
    view.set_pending_cursor_range_anchored(buffer, next_selection);
    view.request_cursor_reveal(CursorRevealMode::KeepVisible);
    buffer.active_selection = None;
    Some(next_selection)
}

fn apply_active_buffer_text_operation(app: &mut ScratchpadApp, undo: bool) -> bool {
    let active_tab_index = app.tab_manager.active_tab_index;
    let active_buffer_label = match active_buffer_transaction_label(app) {
        Some(label) => label,
        None => return false,
    };

    let selection = {
        let tab = &mut app.tab_manager.tabs.as_mut_slice()[active_tab_index];
        let Some(selection) = ({
            let buffer = &mut tab.buffers.buffer;
            if undo {
                buffer.undo_last_text_operation()
            } else {
                buffer.redo_last_text_operation()
            }
        }) else {
            return false;
        };

        let active_view_id = tab.layout.active_view_id;
        if let Some((buffer, view)) = tab.buffer_and_view_mut(active_view_id) {
            view.set_cursor_range_anchored(buffer, selection);
            view.set_pending_cursor_range_anchored(buffer, selection);
            view.request_cursor_reveal(CursorRevealMode::Center);
        }
        selection
    };

    crate::app::app_state::workspace::mutation::finalize_active_buffer_text_mutation(
        app,
        active_tab_index,
    );
    crate::app::app_state::search_runtime::refresh_search_state(app);
    crate::app::app_state::search_visual::select_next_active_buffer_match_from(
        app,
        selection.primary.index,
    );
    let action = if undo { "Undid" } else { "Redid" };
    app.state.status.set_info_status_in_domain(
        StatusDomain::History,
        format!("{action} last text operation in {active_buffer_label}."),
    );
    true
}
