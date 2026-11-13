use crate::app::app_state::{ScratchpadApp, StatusDomain, workspace::display_tabs};
use crate::app::commands::{AppCommand, SettingsCommand, WorkspaceCommand};
use crate::app::utils::pluralize;

#[derive(Clone, Copy)]
enum CloseDisplayTabs {
    SkipDirty,
    SavedOnly,
}

pub(super) fn close_current_slot(app: &mut ScratchpadApp, slot_index: usize, is_settings: bool) {
    if is_settings {
        crate::app::commands::handle_command(
            app,
            AppCommand::Settings(SettingsCommand::CloseSettings),
        );
    } else if let Some(index) = display_tabs::workspace_index_for_slot(app, slot_index) {
        crate::app::commands::handle_command(
            app,
            AppCommand::Workspace(WorkspaceCommand::RequestCloseTab { index }),
        );
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
    let slots = ((current_slot + 1)..display_tabs::total_tab_slots(app)).collect::<Vec<_>>();
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
    (0..display_tabs::total_tab_slots(app)).collect()
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
        if index < app.tab_manager.tabs.as_slice().len() {
            crate::app::app_state::workspace_controller::perform_close_tab_no_persist(app, index);
            closed_count += 1;
        }
    }

    if close_settings {
        crate::app::commands::handle_command(
            app,
            AppCommand::Settings(SettingsCommand::CloseSettings),
        );
    }

    if closed_count > 0 || close_settings {
        app.tab_manager.mark_session_dirty();
    }

    if skipped_dirty > 0 {
        app.state.status.set_warning_status_in_domain(
            StatusDomain::File,
            format!(
                "{action_name} skipped {} with unsaved changes.",
                pluralize(skipped_dirty, "tab")
            ),
        );
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
        if display_tabs::tab_slot_is_settings(app, slot_index) {
            close_settings |= matches!(mode, CloseDisplayTabs::SkipDirty);
            continue;
        }

        let Some(index) = display_tabs::workspace_index_for_slot(app, slot_index) else {
            continue;
        };
        let is_dirty = app
            .tab_manager
            .tabs
            .as_slice()
            .get(index)
            .is_some_and(|tab| tab.buffers().any(|buffer| buffer.is_dirty));
        if !is_dirty {
            workspace_indices.push(index);
        } else if matches!(mode, CloseDisplayTabs::SkipDirty) {
            skipped_dirty += 1;
        }
    }

    (workspace_indices, close_settings, skipped_dirty)
}

#[cfg(test)]
mod tests {
    use super::{
        close_all_slots, close_current_slot, close_other_slots, close_saved_slots,
        close_slots_after,
    };
    use crate::app::app_state::{ScratchpadApp, settings_state, test_support::test_app};
    use crate::app::domain::{BufferState, WorkspaceTab};

    /// Slots: 0 `a`, 1 `b` (unsaved), 2 `c`, 3 Settings.
    fn reset_slots(app: &mut ScratchpadApp) {
        let tabs = ["a", "b", "c"].map(|name| {
            let mut buffer = BufferState::new(name.into(), String::new(), None);
            buffer.is_dirty = name == "b";
            WorkspaceTab::new(buffer)
        });
        app.tab_manager.set_tabs(tabs.into(), 0);
        app.state.app_settings.ui.settings_tab_open = true;
        app.state.settings_tab_index = 3;
        // Close synchronously in RAM instead of archiving through the session store.
        app.state.persistence.session_restore_failed = true;
        app.state.status.current = None;
    }

    #[test]
    fn bulk_closes_skip_unsaved_tabs_and_report_them() {
        type Close = fn(&mut ScratchpadApp);
        let cases: [(Close, &[&str], bool, Option<&str>); 5] = [
            (
                |app| close_other_slots(app, 0),
                &["a", "b"],
                false,
                Some("Close Others skipped 1 tab with unsaved changes."),
            ),
            (|app| close_slots_after(app, 1), &["a", "b"], false, None),
            (close_saved_slots, &["b"], true, None),
            (
                close_all_slots,
                &["b"],
                false,
                Some("Close All skipped 1 tab with unsaved changes."),
            ),
            (
                |app| close_current_slot(app, 3, true),
                &["a", "b", "c"],
                false,
                None,
            ),
        ];
        let root = tempfile::tempdir().unwrap();
        let mut app = test_app(root.path());
        for (index, (close, remaining, settings_open, warning)) in cases.into_iter().enumerate() {
            reset_slots(&mut app);
            close(&mut app);
            let names: Vec<_> = app
                .tab_manager
                .tabs
                .as_slice()
                .iter()
                .map(|tab| tab.buffers.buffer.name.as_str())
                .collect();
            assert_eq!(names, remaining, "case {index}");
            assert_eq!(
                settings_state::settings_tab_open(&app),
                settings_open,
                "case {index}"
            );
            let status = app.state.status.current.as_ref();
            assert_eq!(status.map(|s| s.text.as_str()), warning, "case {index}");
        }
    }
}
