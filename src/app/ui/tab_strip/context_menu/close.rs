use crate::app::app_state::ScratchpadApp;
use crate::app::commands::AppCommand;

#[derive(Clone, Copy)]
enum CloseDisplayTabs {
    SkipDirty,
    SavedOnly,
}

pub(super) fn close_current_slot(app: &mut ScratchpadApp, slot_index: usize, is_settings: bool) {
    if is_settings {
        app.close_settings();
    } else if let Some(index) = app.workspace_index_for_slot(slot_index) {
        app.handle_command(AppCommand::RequestCloseTab { index });
    }
}

pub(super) fn close_other_slots(app: &mut ScratchpadApp, current_slot: usize) {
    let slots = tab_slots(app)
        .into_iter()
        .filter(|slot_index| *slot_index != current_slot)
        .collect::<Vec<_>>();
    close_display_slots(app, slots, CloseDisplayTabs::SkipDirty, "Close Others");
}

pub(super) fn close_slots_after(app: &mut ScratchpadApp, current_slot: usize) {
    let slots = ((current_slot + 1)..app.total_tab_slots()).collect::<Vec<_>>();
    close_display_slots(app, slots, CloseDisplayTabs::SkipDirty, "Close tabs");
}

pub(super) fn close_saved_slots(app: &mut ScratchpadApp) {
    let slots = tab_slots(app);
    close_display_slots(app, slots, CloseDisplayTabs::SavedOnly, "Close Saved");
}

pub(super) fn close_all_slots(app: &mut ScratchpadApp) {
    let slots = tab_slots(app);
    close_display_slots(app, slots, CloseDisplayTabs::SkipDirty, "Close All");
}

fn tab_slots(app: &ScratchpadApp) -> Vec<usize> {
    (0..app.total_tab_slots()).collect()
}

fn close_display_slots(
    app: &mut ScratchpadApp,
    slots: Vec<usize>,
    mode: CloseDisplayTabs,
    action_name: &str,
) {
    let (mut workspace_indices, close_settings, skipped_dirty) =
        collect_close_targets(app, slots, mode);

    workspace_indices.sort_unstable();
    workspace_indices.dedup();

    let mut closed_count = 0usize;
    for index in workspace_indices.into_iter().rev() {
        if index < app.tabs().len() {
            app.perform_close_tab_no_persist(index);
            closed_count += 1;
        }
    }

    if close_settings {
        app.close_settings();
    }

    if closed_count > 0 || close_settings {
        let _ = app.persist_session_now();
    }

    if skipped_dirty > 0 {
        app.set_warning_status(format!(
            "{action_name} skipped {skipped_dirty} tab(s) with unsaved changes."
        ));
    }
}

fn collect_close_targets(
    app: &ScratchpadApp,
    slots: Vec<usize>,
    mode: CloseDisplayTabs,
) -> (Vec<usize>, bool, usize) {
    let mut workspace_indices = Vec::new();
    let mut close_settings = false;
    let mut skipped_dirty = 0usize;

    for slot_index in slots {
        if app.tab_slot_is_settings(slot_index) {
            close_settings |= matches!(mode, CloseDisplayTabs::SkipDirty);
            continue;
        }

        let Some(index) = app.workspace_index_for_slot(slot_index) else {
            continue;
        };
        let is_dirty = app
            .tabs()
            .get(index)
            .is_some_and(|tab| tab.buffers().any(|buffer| buffer.is_dirty));
        if !is_dirty {
            workspace_indices.push(index);
        } else if matches!(mode, CloseDisplayTabs::SkipDirty) && is_dirty {
            skipped_dirty += 1;
        }
    }

    (workspace_indices, close_settings, skipped_dirty)
}
