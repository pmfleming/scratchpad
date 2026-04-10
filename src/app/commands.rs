use crate::app::app_state::ScratchpadApp;
use crate::app::domain::{PendingAction, SplitAxis, SplitPath, ViewId, WorkspaceTab};
use crate::app::logging::LogLevel;

mod dispatch;

struct TabCombineContext {
    source_description: String,
    target_description: String,
    adjusted_target_index: usize,
}

pub enum AppCommand {
    ActivateTab {
        index: usize,
    },
    ActivateView {
        view_id: ViewId,
    },
    CloseTab {
        index: usize,
    },
    CloseView {
        view_id: ViewId,
    },
    CloseSettings,
    CombineTabIntoTab {
        source_index: usize,
        target_index: usize,
    },
    PromoteViewToTab {
        view_id: ViewId,
    },
    PromoteTabFilesToTabs {
        index: usize,
    },
    NewTab,
    OpenFile,
    OpenFileHere,
    OpenSettings,
    ReorderTab {
        from_index: usize,
        to_index: usize,
    },
    ReorderDisplayTab {
        from_index: usize,
        to_index: usize,
    },
    RequestCloseTab {
        index: usize,
    },
    ResizeSplit {
        path: SplitPath,
        ratio: f32,
    },
    SaveFile,
    SaveFileAs,
    SplitActiveView {
        axis: SplitAxis,
        new_view_first: bool,
        ratio: f32,
    },
}

impl ScratchpadApp {
    fn activate_tab(&mut self, index: usize) {
        if index >= self.tabs().len() {
            return;
        }

        self.reload_settings_from_active_settings_tab();
        let tab_description = self.describe_tab_at(index);
        self.activate_workspace_surface();
        self.tab_manager_mut().active_tab_index = index;
        self.tab_manager_mut().pending_scroll_to_active = true;
        self.request_focus_for_active_view();
        self.mark_session_dirty();
        self.log_event(
            LogLevel::Info,
            format!("Activated tab index {index}: {tab_description}"),
        );
    }

    fn activate_view_command(&mut self, view_id: ViewId) {
        if self
            .active_tab()
            .is_some_and(|tab| tab.active_view_id != view_id)
        {
            self.reload_settings_from_active_settings_tab();
        }

        let index = self.active_tab_index();
        let tab_name = self
            .tabs()
            .get(index)
            .map(|tab| tab.active_buffer().name.clone())
            .unwrap_or_else(|| "<missing>".to_owned());
        if let Some(tab) = self.tabs_mut().get_mut(index)
            && tab.activate_view(view_id)
        {
            let previous_view_id = tab.active_view_id;
            self.request_focus_for_view(view_id);
            self.mark_session_dirty();
            self.log_event(
                LogLevel::Info,
                format!(
                    "Activated view {view_id} in tab '{tab_name}' (previous active view={previous_view_id})"
                ),
            );
        }
    }

    fn close_view_command(&mut self, view_id: ViewId) {
        if self
            .active_tab()
            .is_some_and(|tab| tab.active_view_id == view_id)
        {
            self.reload_settings_from_active_settings_tab();
        }

        let index = self.active_tab_index();
        let tab_name = self
            .tabs()
            .get(index)
            .map(|tab| tab.active_buffer().name.clone())
            .unwrap_or_else(|| "<missing>".to_owned());
        if let Some(tab) = self.tabs_mut().get_mut(index)
            && tab.close_view(view_id)
        {
            let next_active_view = tab.active_view_id;
            let remaining_views = tab.views.len();
            self.request_focus_for_view(next_active_view);
            self.mark_session_dirty();
            self.log_event(
                LogLevel::Info,
                format!(
                    "Closed view {view_id} in tab '{tab_name}' (remaining views={remaining_views}, active view={next_active_view})"
                ),
            );
        }
    }

    fn request_close_tab(&mut self, index: usize) {
        if index < self.tabs().len() {
            let tab_description = self.describe_tab_at(index);
            self.set_pending_action(Some(PendingAction::CloseTab(index)));
            self.log_event(
                LogLevel::Info,
                format!("Requested close for tab index {index}: {tab_description}"),
            );
        }
    }

    fn reorder_tab_command(&mut self, from_index: usize, to_index: usize) {
        let moved_tab_description = self.describe_tab_at(from_index);
        if !self.tab_manager_mut().reorder_tab(from_index, to_index) {
            return;
        }
        self.log_event(
            LogLevel::Info,
            format!(
                "Reordered tab from index {from_index} to {to_index}: {moved_tab_description} (active tab index={})",
                self.active_tab_index()
            ),
        );
    }

    fn reorder_display_tab_command(&mut self, from_index: usize, to_index: usize) {
        let moved_tab_description = self
            .display_tab_name_at_slot(from_index)
            .unwrap_or_else(|| format!("tab#{from_index}<missing>"));
        if !self.reorder_display_tab(from_index, to_index) {
            return;
        }

        self.log_event(
            LogLevel::Info,
            format!(
                "Reordered displayed tab from slot {from_index} to {to_index}: {moved_tab_description}"
            ),
        );
    }

    fn resize_split_command(&mut self, path: SplitPath, ratio: f32) {
        let index = self.active_tab_index();
        let tab_name = self
            .tabs()
            .get(index)
            .map(|tab| tab.active_buffer().name.clone())
            .unwrap_or_else(|| "<missing>".to_owned());
        let path_description = format!("{:?}", path);
        if let Some(tab) = self.tabs_mut().get_mut(index)
            && tab.resize_split(path, ratio)
        {
            self.mark_session_dirty();
            self.log_event(
                LogLevel::Info,
                format!(
                    "Resized split in tab '{tab_name}' at path {path_description} to ratio {:.3}",
                    ratio.clamp(0.2, 0.8)
                ),
            );
        }
    }

    fn split_active_view_command(&mut self, axis: SplitAxis, new_view_first: bool, ratio: f32) {
        let index = self.active_tab_index();
        let tab_name = self
            .tabs()
            .get(index)
            .map(|tab| tab.active_buffer().name.clone())
            .unwrap_or_else(|| "<missing>".to_owned());
        if let Some(tab) = self.tabs_mut().get_mut(index)
            && tab
                .split_active_view_with_placement(axis, new_view_first, ratio)
                .is_some()
        {
            let new_active_view = tab.active_view_id;
            let total_views = tab.views.len();
            self.request_focus_for_view(new_active_view);
            self.mark_session_dirty();
            self.log_event(
                LogLevel::Info,
                format!(
                    "Split active view in tab '{tab_name}' with axis={axis:?}, new_view_first={new_view_first}, ratio={:.3}; new active view={new_active_view}, total views={total_views}",
                    ratio.clamp(0.2, 0.8)
                ),
            );
        }
    }

    fn combine_tab_into_tab_command(&mut self, source_index: usize, target_index: usize) {
        if !Self::can_combine_tabs(self.tabs().len(), source_index, target_index) {
            return;
        }

        if source_index == self.active_tab_index() || target_index == self.active_tab_index() {
            self.reload_settings_from_active_settings_tab();
        }

        let (context, source_tab) = self.remove_source_tab_for_combine(source_index, target_index);
        let mut source_tab = Some(source_tab);
        if !self.try_combine_tabs(context.adjusted_target_index, &mut source_tab) {
            self.rollback_combined_tab(
                source_index,
                source_tab.expect("source tab should remain available on combine failure"),
            );
            return;
        }

        self.finish_combined_tab(source_index, target_index, context);
    }

    fn promote_view_to_tab_command(&mut self, view_id: ViewId) {
        self.reload_settings_from_active_settings_tab();

        let source_index = self.active_tab_index();
        let source_description = self.describe_tab_at(source_index);
        let promoted_tab = if let Some(tab) = self.tabs_mut().get_mut(source_index) {
            tab.promote_view_to_new_tab(view_id)
        } else {
            None
        };

        let Some(promoted_tab) = promoted_tab else {
            return;
        };

        let promoted_description = promoted_tab.describe();
        self.append_tab(promoted_tab);
        let promoted_index = self.active_tab_index();
        self.log_event(
            LogLevel::Info,
            format!(
                "Promoted view {view_id} from tab index {source_index} into new tab index {promoted_index}: source={source_description}, promoted={promoted_description}"
            ),
        );
        let _ = self.persist_session_now();
    }

    fn promote_tab_files_to_tabs_command(&mut self, index: usize) {
        if index >= self.tabs().len() {
            return;
        }

        if index == self.active_tab_index() {
            self.reload_settings_from_active_settings_tab();
        }

        let source_description = self.describe_tab_at(index);
        let source_tab = self.tab_manager_mut().tabs.remove(index);
        if !source_tab.can_promote_all_files() {
            self.tab_manager_mut().tabs.insert(index, source_tab);
            return;
        }

        let active_buffer_id = source_tab.active_buffer().id;
        let promoted_tabs = source_tab.into_tabs_per_file();
        if promoted_tabs.len() <= 1 {
            self.tab_manager_mut().tabs.insert(
                index,
                promoted_tabs
                    .into_iter()
                    .next()
                    .unwrap_or_else(crate::app::domain::WorkspaceTab::untitled),
            );
            return;
        }

        let active_tab_offset = promoted_tabs
            .iter()
            .position(|tab| tab.active_buffer().id == active_buffer_id)
            .unwrap_or(0);
        let promoted_count = promoted_tabs.len();
        for (offset, tab) in promoted_tabs.into_iter().enumerate() {
            self.tab_manager_mut().tabs.insert(index + offset, tab);
        }
        self.tab_manager_mut().active_tab_index = index + active_tab_offset;
        self.tab_manager_mut().pending_scroll_to_active = true;
        self.request_focus_for_active_view();
        self.mark_session_dirty();
        self.log_event(
            LogLevel::Info,
            format!(
                "Promoted all files from tab index {index} into {promoted_count} tabs: source={source_description}"
            ),
        );
        let _ = self.persist_session_now();
    }

    fn can_combine_tabs(tab_count: usize, source_index: usize, target_index: usize) -> bool {
        source_index != target_index && source_index < tab_count && target_index < tab_count
    }

    fn remove_source_tab_for_combine(
        &mut self,
        source_index: usize,
        target_index: usize,
    ) -> (TabCombineContext, WorkspaceTab) {
        let source_description = self.describe_tab_at(source_index);
        let target_description = self.describe_tab_at(target_index);
        let adjusted_target_index = Self::adjusted_target_index(source_index, target_index);
        let source_tab = self.tab_manager_mut().tabs.remove(source_index);

        (
            TabCombineContext {
                source_description,
                target_description,
                adjusted_target_index,
            },
            source_tab,
        )
    }

    fn adjusted_target_index(source_index: usize, target_index: usize) -> usize {
        if source_index < target_index {
            target_index.saturating_sub(1)
        } else {
            target_index
        }
    }

    fn try_combine_tabs(
        &mut self,
        adjusted_target_index: usize,
        source_tab: &mut Option<WorkspaceTab>,
    ) -> bool {
        self.tab_manager_mut()
            .tabs
            .get_mut(adjusted_target_index)
            .map(|target_tab| {
                target_tab
                    .combine_with_tab(
                        source_tab
                            .take()
                            .expect("source tab removed before combine"),
                        SplitAxis::Vertical,
                        false,
                        0.5,
                    )
                    .is_some()
            })
            .unwrap_or(false)
    }

    fn rollback_combined_tab(&mut self, source_index: usize, source_tab: WorkspaceTab) {
        let reinsertion_index = source_index.min(self.tabs().len());
        self.tab_manager_mut()
            .tabs
            .insert(reinsertion_index, source_tab);
    }

    fn finish_combined_tab(
        &mut self,
        source_index: usize,
        target_index: usize,
        context: TabCombineContext,
    ) {
        self.tab_manager_mut().active_tab_index = context.adjusted_target_index;
        self.tab_manager_mut().pending_scroll_to_active = true;
        self.request_focus_for_active_view();
        self.mark_session_dirty();
        self.log_event(
            LogLevel::Info,
            format!(
                "Combined tab index {source_index} into tab index {target_index}: source={}, target={}",
                context.source_description, context.target_description
            ),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::AppCommand;
    use crate::app::app_state::ScratchpadApp;
    use crate::app::domain::{BufferState, SplitAxis, WorkspaceTab};
    use crate::app::services::session_store::SessionStore;

    fn test_app() -> ScratchpadApp {
        let session_root = tempfile::tempdir().expect("create session dir");
        let session_store = SessionStore::new(session_root.path().to_path_buf());
        ScratchpadApp::with_session_store(session_store)
    }

    #[test]
    fn promote_view_to_tab_creates_a_new_active_tab() {
        let mut app = test_app();
        app.tabs_mut()[0].buffer.name = "alpha.txt".to_owned();
        app.tabs_mut()[0].buffer.content = "alpha".to_owned();
        let promoted_view_id = app.tabs_mut()[0]
            .split_active_view(SplitAxis::Vertical)
            .expect("split should succeed");
        let first_view_id = app.tabs()[0].views[0].id;
        app.tabs_mut()[0].activate_view(first_view_id);
        app.tabs_mut()[0]
            .open_buffer_as_split(
                BufferState::new("beta.txt".to_owned(), "beta".to_owned(), None),
                SplitAxis::Horizontal,
                false,
                0.5,
            )
            .expect("open buffer split should succeed");

        app.handle_command(AppCommand::PromoteViewToTab {
            view_id: promoted_view_id,
        });

        assert_eq!(app.tabs().len(), 2);
        assert_eq!(app.active_tab_index(), 1);
        assert_eq!(app.tabs()[1].views.len(), 2);
        assert_eq!(app.tabs()[1].active_view_id, promoted_view_id);
        assert_eq!(app.tabs()[1].active_buffer().name, "alpha.txt");
        assert_eq!(app.tabs()[0].views.len(), 1);
        assert_eq!(app.tabs()[0].active_buffer().name, "beta.txt");
        assert_eq!(app.pending_editor_focus, Some(promoted_view_id));
    }

    #[test]
    fn promote_tab_files_to_tabs_splits_workspace_into_individual_tabs() {
        let mut app = test_app();
        app.tabs_mut()[0].buffer.name = "one.txt".to_owned();
        app.tabs_mut()[0].buffer.content = "one".to_owned();

        for (name, content) in [("two.txt", "two"), ("three.txt", "three")] {
            app.tabs_mut()[0]
                .open_buffer_as_split(
                    BufferState::new(name.to_owned(), content.to_owned(), None),
                    SplitAxis::Vertical,
                    false,
                    0.5,
                )
                .expect("open buffer split should succeed");
        }

        assert!(app.tabs()[0].can_promote_all_files());
        let active_name = app.tabs()[0].active_buffer().name.clone();

        app.handle_command(AppCommand::PromoteTabFilesToTabs { index: 0 });

        assert_eq!(app.tabs().len(), 3);
        assert!(app.tabs().iter().all(|tab| tab.file_group_count() == 1));
        assert_eq!(
            app.tabs()[app.active_tab_index()].active_buffer().name,
            active_name
        );
        assert_eq!(
            app.pending_editor_focus,
            Some(app.tabs()[app.active_tab_index()].active_view_id)
        );
    }

    #[test]
    fn activating_a_tab_queues_focus_for_its_active_view() {
        let mut app = test_app();
        app.append_tab(WorkspaceTab::untitled());
        app.pending_editor_focus = None;

        let first_tab_view_id = app.tabs()[0].active_view_id;
        app.handle_command(AppCommand::ActivateTab { index: 0 });

        assert_eq!(app.active_tab_index(), 0);
        assert_eq!(app.pending_editor_focus, Some(first_tab_view_id));
    }

    #[test]
    fn activating_a_view_queues_focus_for_that_view() {
        let mut app = test_app();
        let second_view_id = app.tabs_mut()[0]
            .split_active_view(SplitAxis::Vertical)
            .expect("split should succeed");
        app.pending_editor_focus = None;

        app.handle_command(AppCommand::ActivateView {
            view_id: second_view_id,
        });

        assert_eq!(app.tabs()[0].active_view_id, second_view_id);
        assert_eq!(app.pending_editor_focus, Some(second_view_id));
    }

    #[test]
    fn settings_commands_switch_between_workspace_and_settings_surface() {
        let mut app = test_app();

        app.handle_command(AppCommand::OpenSettings);
        assert!(app.showing_settings());
        assert_eq!(app.settings_slot_index(), Some(app.tabs().len()));

        app.handle_command(AppCommand::CloseSettings);
        assert!(!app.showing_settings());
        assert_eq!(app.settings_slot_index(), None);
        assert_eq!(
            app.pending_editor_focus,
            Some(app.tabs()[app.active_tab_index()].active_view_id)
        );
    }

    #[test]
    fn reorder_display_tab_moves_settings_slot_between_workspace_tabs() {
        let mut app = test_app();
        app.tabs_mut()[0].buffer.name = "one.txt".to_owned();
        app.append_tab(WorkspaceTab::untitled());
        app.tabs_mut()[1].buffer.name = "two.txt".to_owned();
        app.append_tab(WorkspaceTab::untitled());
        app.tabs_mut()[2].buffer.name = "three.txt".to_owned();

        app.handle_command(AppCommand::OpenSettings);
        let settings_slot = app
            .settings_slot_index()
            .expect("settings slot should exist");
        assert_eq!(settings_slot, 3);

        app.handle_command(AppCommand::ReorderDisplayTab {
            from_index: settings_slot,
            to_index: 1,
        });

        assert_eq!(app.settings_slot_index(), Some(1));
        assert_eq!(app.workspace_index_for_slot(0), Some(0));
        assert_eq!(app.workspace_index_for_slot(2), Some(1));
        assert_eq!(app.workspace_index_for_slot(3), Some(2));
        assert_eq!(app.display_tab_name_at_slot(1).as_deref(), Some("Settings"));
    }

    #[test]
    fn settings_surface_state_persists_across_restart() {
        let session_root = tempfile::tempdir().expect("create session dir");
        let session_path = session_root.path().to_path_buf();

        let mut original =
            ScratchpadApp::with_session_store(SessionStore::new(session_path.clone()));
        original.tabs_mut()[0].buffer.name = "one.txt".to_owned();
        original.append_tab(WorkspaceTab::untitled());
        original.tabs_mut()[1].buffer.name = "two.txt".to_owned();
        original.append_tab(WorkspaceTab::untitled());
        original.tabs_mut()[2].buffer.name = "three.txt".to_owned();

        original.handle_command(AppCommand::OpenSettings);
        original.handle_command(AppCommand::ReorderDisplayTab {
            from_index: 3,
            to_index: 1,
        });

        let persisted = original
            .settings_store
            .load()
            .expect("load persisted settings")
            .expect("settings should exist");
        assert!(persisted.settings_tab_open);
        assert_eq!(persisted.settings_tab_index, Some(1));

        drop(original);

        let restored = ScratchpadApp::with_session_store(SessionStore::new(session_path));
        assert!(restored.showing_settings());
        assert_eq!(restored.settings_slot_index(), Some(1));
        assert_eq!(
            restored.display_tab_name_at_slot(1).as_deref(),
            Some("Settings")
        );
    }

    #[test]
    fn activating_a_workspace_tab_keeps_the_settings_tab_open() {
        let mut app = test_app();
        app.tabs_mut()[0].buffer.name = "one.txt".to_owned();
        app.append_tab(WorkspaceTab::untitled());
        app.tabs_mut()[1].buffer.name = "two.txt".to_owned();

        app.handle_command(AppCommand::OpenSettings);
        let settings_slot = app
            .settings_slot_index()
            .expect("settings slot should exist after open");

        app.handle_command(AppCommand::ActivateTab { index: 1 });

        assert!(!app.showing_settings());
        assert_eq!(app.settings_slot_index(), Some(settings_slot));
        assert_eq!(app.active_tab_index(), 1);
        assert_eq!(
            app.display_tab_name_at_slot(settings_slot).as_deref(),
            Some("Settings")
        );
    }

    #[test]
    fn opening_settings_file_keeps_settings_tab_open() {
        let mut app = test_app();
        app.handle_command(AppCommand::OpenSettings);
        let settings_slot = app
            .settings_slot_index()
            .expect("settings slot should exist after open");
        let settings_path = app.settings_path();

        app.open_settings_file_tab();

        assert!(!app.showing_settings());
        assert_eq!(app.settings_slot_index(), Some(settings_slot));
        assert_eq!(
            app.tabs()[app.active_tab_index()]
                .active_buffer()
                .path
                .as_deref(),
            Some(settings_path.as_path())
        );
        assert!(
            app.tabs()[app.active_tab_index()]
                .active_buffer()
                .is_settings_file
        );
    }

    #[test]
    fn creating_a_new_tab_from_dirty_settings_file_applies_toml_edits() {
        let mut app = test_app();
        app.handle_command(AppCommand::OpenSettings);
        app.open_settings_file_tab();
        let settings_tab_index = app.active_tab_index();

        app.tabs_mut()[settings_tab_index]
            .active_buffer_mut()
            .content = [
            "font_size = 19.0",
            "word_wrap = false",
            "logging_enabled = false",
            "editor_font = \"roboto\"",
            "settings_tab_open = true",
            "settings_tab_index = 1",
            "",
        ]
        .join("\n");
        app.tabs_mut()[settings_tab_index]
            .active_buffer_mut()
            .is_dirty = true;
        app.note_settings_toml_edit(settings_tab_index);

        app.new_tab();

        assert_eq!(app.font_size(), 19.0);
        assert!(!app.word_wrap());
        assert!(!app.logging_enabled());
        assert_eq!(
            app.editor_font(),
            crate::app::fonts::EditorFontPreset::Roboto
        );
        assert_eq!(
            app.tabs()[app.active_tab_index()].active_buffer().name,
            "Untitled"
        );
    }

    #[test]
    fn activating_away_from_settings_file_applies_toml_edits() {
        let mut app = test_app();
        app.handle_command(AppCommand::OpenSettings);
        app.open_settings_file_tab();
        let settings_tab_index = app.active_tab_index();
        app.tabs_mut()[settings_tab_index]
            .active_buffer_mut()
            .content = [
            "font_size = 22.0",
            "word_wrap = false",
            "logging_enabled = false",
            "editor_font = \"roboto\"",
            "settings_tab_open = true",
            "settings_tab_index = 1",
            "",
        ]
        .join("\n");
        app.tabs_mut()[settings_tab_index]
            .active_buffer_mut()
            .is_dirty = true;

        app.handle_command(AppCommand::ActivateTab { index: 0 });

        assert_eq!(app.font_size(), 22.0);
        assert!(!app.word_wrap());
        assert!(!app.logging_enabled());
        assert_eq!(
            app.editor_font(),
            crate::app::fonts::EditorFontPreset::Roboto
        );
        assert_eq!(app.settings_slot_index(), Some(1));
        assert_eq!(app.active_tab_index(), 0);
        assert!(!app.showing_settings());
    }

    #[test]
    fn activating_view_away_from_settings_file_applies_toml_edits() {
        let mut app = test_app();
        app.handle_command(AppCommand::OpenSettings);
        app.open_settings_file_tab();
        let settings_tab_index = app.active_tab_index();
        let settings_view_id = app.tabs()[settings_tab_index].active_view_id;
        let normal_view_id = app.tabs_mut()[settings_tab_index]
            .open_buffer_as_split(
                BufferState::new("notes.txt".to_owned(), "notes".to_owned(), None),
                SplitAxis::Vertical,
                false,
                0.5,
            )
            .expect("open normal split");

        app.handle_command(AppCommand::ActivateView {
            view_id: settings_view_id,
        });
        let settings_tab_index = app.active_tab_index();
        app.tabs_mut()[settings_tab_index]
            .active_buffer_mut()
            .content = [
            "font_size = 23.0",
            "word_wrap = false",
            "logging_enabled = false",
            "editor_font = \"roboto\"",
            "settings_tab_open = false",
            "",
        ]
        .join("\n");
        app.tabs_mut()[settings_tab_index]
            .active_buffer_mut()
            .is_dirty = true;
        app.note_settings_toml_edit(settings_tab_index);

        app.handle_command(AppCommand::ActivateView {
            view_id: normal_view_id,
        });

        assert_eq!(app.font_size(), 23.0);
        assert!(!app.word_wrap());
        assert_eq!(
            app.tabs()[app.active_tab_index()].active_view_id,
            normal_view_id
        );
    }

    #[test]
    fn editing_other_buffer_in_settings_workspace_does_not_mark_settings_toml_pending() {
        let mut app = test_app();
        app.handle_command(AppCommand::OpenSettings);
        app.open_settings_file_tab();
        let settings_path = app.settings_path();
        let settings_tab_index = app.active_tab_index();
        app.tabs_mut()[settings_tab_index]
            .open_buffer_as_split(
                BufferState::new("notes.txt".to_owned(), "notes".to_owned(), None),
                SplitAxis::Vertical,
                false,
                0.5,
            )
            .expect("open normal split");
        let settings_tab_index = app.active_tab_index();

        let settings_buffer = app.tabs_mut()[settings_tab_index]
            .extra_buffers
            .iter_mut()
            .find(|buffer| {
                buffer
                    .path
                    .as_ref()
                    .is_some_and(|path| crate::app::paths_match(path, &settings_path))
            })
            .expect("settings buffer should remain in the workspace");
        settings_buffer.content = [
            "font_size = 31.0",
            "word_wrap = false",
            "logging_enabled = false",
            "editor_font = \"roboto\"",
            "",
        ]
        .join("\n");

        app.tabs_mut()[settings_tab_index]
            .active_buffer_mut()
            .content = "changed notes".to_owned();
        app.tabs_mut()[settings_tab_index]
            .active_buffer_mut()
            .is_dirty = true;
        app.note_settings_toml_edit(settings_tab_index);
        app.append_tab(WorkspaceTab::untitled());
        let other_tab_index = app.active_tab_index();

        app.handle_command(AppCommand::ActivateTab {
            index: other_tab_index,
        });

        assert_eq!(app.font_size(), 14.0);
        assert!(app.word_wrap());
        assert!(app.logging_enabled());
    }

    #[test]
    fn closing_saved_settings_file_applies_pending_toml_edits() {
        let mut app = test_app();
        app.handle_command(AppCommand::OpenSettings);
        app.open_settings_file_tab();
        let settings_tab_index = app.active_tab_index();
        app.tabs_mut()[settings_tab_index]
            .active_buffer_mut()
            .content = [
            "font_size = 20.0",
            "word_wrap = false",
            "logging_enabled = false",
            "editor_font = \"roboto\"",
            "settings_tab_open = true",
            "settings_tab_index = 0",
            "",
        ]
        .join("\n");
        app.tabs_mut()[settings_tab_index]
            .active_buffer_mut()
            .is_dirty = true;
        app.note_settings_toml_edit(settings_tab_index);

        assert!(app.save_file_at(settings_tab_index));

        app.handle_command(AppCommand::CloseTab {
            index: settings_tab_index,
        });

        assert_eq!(app.font_size(), 20.0);
        assert!(!app.word_wrap());
        assert!(!app.logging_enabled());
        assert_eq!(
            app.editor_font(),
            crate::app::fonts::EditorFontPreset::Roboto
        );
        assert!(app.showing_settings());
        assert_eq!(app.settings_slot_index(), Some(0));
    }

    #[test]
    fn closing_dirty_settings_file_applies_buffered_toml_edits() {
        let mut app = test_app();
        app.handle_command(AppCommand::OpenSettings);
        app.open_settings_file_tab();
        let settings_tab_index = app.active_tab_index();

        app.tabs_mut()[settings_tab_index]
            .active_buffer_mut()
            .content = [
            "font_size = 26.0",
            "word_wrap = false",
            "logging_enabled = false",
            "editor_font = \"roboto\"",
            "settings_tab_open = true",
            "settings_tab_index = 0",
            "",
        ]
        .join("\n");
        app.tabs_mut()[settings_tab_index]
            .active_buffer_mut()
            .is_dirty = true;
        app.note_settings_toml_edit(settings_tab_index);

        app.handle_command(AppCommand::CloseTab {
            index: settings_tab_index,
        });

        assert_eq!(app.font_size(), 26.0);
        assert!(!app.word_wrap());
        assert!(!app.logging_enabled());
        assert_eq!(
            app.editor_font(),
            crate::app::fonts::EditorFontPreset::Roboto
        );
        assert!(app.showing_settings());
        assert_eq!(app.settings_slot_index(), Some(0));
    }
}
