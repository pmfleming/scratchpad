use crate::app::app_state::ScratchpadApp;
use crate::app::domain::WorkspaceTab;
use crate::app::services::settings_store::{TabOrderDirection, TabOrderMode};
use std::cmp::Ordering;

impl ScratchpadApp {
    pub(crate) fn set_tab_order_mode(&mut self, mode: TabOrderMode) {
        if self.state.app_settings.workspace.tab_order_mode == mode {
            return;
        }

        if self.state.app_settings.workspace.tab_order_mode == TabOrderMode::Custom {
            self.remember_current_custom_tab_order();
        }

        self.state.app_settings.workspace.tab_order_mode = mode;
        if mode == TabOrderMode::Custom {
            self.restore_custom_tab_order();
        } else {
            self.apply_current_tab_ordering();
        }
        self.persist_settings_or_error();
    }

    pub(crate) fn set_tab_order_direction(&mut self, direction: TabOrderDirection) {
        if self.state.app_settings.workspace.tab_order_direction == direction {
            return;
        }

        self.state.app_settings.workspace.tab_order_direction = direction;
        self.apply_current_tab_ordering();
        self.persist_settings_or_error();
    }

    pub(crate) fn apply_workspace_tab_order(&mut self, workspace_order: Vec<usize>) {
        if self.apply_workspace_tab_order_internal(workspace_order, false) {
            self.state.app_settings.workspace.tab_order_mode = TabOrderMode::Custom;
            self.remember_current_custom_tab_order();
            self.persist_settings_or_error();
        }
    }

    pub(crate) fn apply_current_tab_ordering(&mut self) -> bool {
        let workspace_order =
            match workspace_tab_order_for_mode(self, self.state.app_settings.tab_order_mode()) {
                Some(order) => order,
                None => return false,
            };
        let reordered = self.apply_workspace_tab_order_internal(workspace_order, false);
        if reordered {
            self.begin_layout_transition();
        }
        reordered
    }

    fn apply_workspace_tab_order_internal(
        &mut self,
        workspace_order: Vec<usize>,
        persist_settings: bool,
    ) -> bool {
        if workspace_order.len() != self.tab_manager.tabs.len() {
            return false;
        }

        let active_workspace_index = self.tab_manager.active_tab_index;
        let current_order = (0..self.tab_manager.tabs.len()).collect::<Vec<_>>();
        if workspace_order == current_order {
            return false;
        }

        let mut tabs = std::mem::take(&mut self.tab_manager.tabs)
            .into_iter()
            .map(Some)
            .collect::<Vec<_>>();
        self.tab_manager.tabs = workspace_order
            .iter()
            .filter_map(|&index| tabs.get_mut(index).and_then(Option::take))
            .collect();
        let reordered_active_tab_index = workspace_order
            .iter()
            .position(|&index| index == active_workspace_index)
            .unwrap_or(0);
        self.tab_manager
            .set_active_tab_index_clamped(reordered_active_tab_index);
        self.tab_manager.rebuild_buffer_tab_index();
        self.ensure_active_tab_slot_selected();
        self.tab_manager.pending_scroll_to_active = true;
        self.tab_manager.mark_session_dirty();
        if persist_settings {
            self.persist_settings_or_error();
        }
        true
    }

    pub(crate) fn remember_current_custom_tab_order(&mut self) {
        self.state.app_settings.workspace.custom_tab_order = self
            .tab_manager
            .tabs
            .as_slice()
            .iter()
            .map(|tab| tab.buffer.id)
            .collect();
    }

    fn restore_custom_tab_order(&mut self) -> bool {
        let workspace_order = workspace_tab_order_from_saved_custom_order(self);
        let reordered = self.apply_workspace_tab_order_internal(workspace_order, false);
        if reordered {
            self.begin_layout_transition();
        }
        reordered
    }
}

fn workspace_tab_order_for_mode(app: &ScratchpadApp, mode: TabOrderMode) -> Option<Vec<usize>> {
    if mode == TabOrderMode::Custom {
        return None;
    }

    let mut order = (0..app.tab_manager.tabs.as_slice().len()).collect::<Vec<_>>();
    let context = TabOrderContext::new(app);
    sort_workspace_tab_order(&mut order, mode, &context);
    Some(order)
}

struct TabOrderContext<'a> {
    app: &'a ScratchpadApp,
    custom_rank: Vec<usize>,
    direction: TabOrderDirection,
}

impl<'a> TabOrderContext<'a> {
    fn new(app: &'a ScratchpadApp) -> Self {
        Self {
            app,
            custom_rank: custom_order_ranks(app),
            direction: app.state.app_settings.tab_order_direction(),
        }
    }

    fn tab(&self, index: usize) -> &WorkspaceTab {
        &self.app.tab_manager.tabs.as_slice()[index]
    }

    fn custom_rank_cmp(&self, left: usize, right: usize) -> Ordering {
        self.custom_rank[left].cmp(&self.custom_rank[right])
    }
}

fn sort_workspace_tab_order(
    order: &mut [usize],
    mode: TabOrderMode,
    context: &TabOrderContext<'_>,
) {
    match mode {
        TabOrderMode::Custom => {}
        TabOrderMode::FileName => {
            order.sort_by(|left, right| compare_file_name_tabs(context, *left, *right))
        }
        TabOrderMode::FileSize => {
            order.sort_by(|left, right| compare_file_size_tabs(context, *left, *right))
        }
        TabOrderMode::FileAge => {
            order.sort_by(|left, right| compare_file_age_tabs(context, *left, *right))
        }
        TabOrderMode::RecentEdit => {
            order.sort_by(|left, right| compare_recent_edit_tabs(context, *left, *right))
        }
    }
}

fn compare_file_name_tabs(context: &TabOrderContext<'_>, left: usize, right: usize) -> Ordering {
    let left_tab = context.tab(left);
    let right_tab = context.tab(right);
    order_direction_cmp(
        context.direction,
        left_tab.buffer.name.to_ascii_lowercase(),
        right_tab.buffer.name.to_ascii_lowercase(),
    )
    .then_with(|| {
        order_direction_cmp(
            context.direction,
            left_tab.buffer.name.as_str(),
            right_tab.buffer.name.as_str(),
        )
    })
    .then_with(|| {
        order_direction_cmp(
            context.direction,
            tab_path_label(left_tab),
            tab_path_label(right_tab),
        )
    })
    .then_with(|| context.custom_rank_cmp(left, right))
}

fn compare_file_size_tabs(context: &TabOrderContext<'_>, left: usize, right: usize) -> Ordering {
    let left_tab = context.tab(left);
    let right_tab = context.tab(right);
    order_direction_cmp(
        context.direction,
        tab_total_size(left_tab),
        tab_total_size(right_tab),
    )
    .then_with(|| {
        order_direction_cmp(
            context.direction,
            left_tab.buffer.name.to_ascii_lowercase(),
            right_tab.buffer.name.to_ascii_lowercase(),
        )
    })
    .then_with(|| context.custom_rank_cmp(left, right))
}

fn compare_file_age_tabs(context: &TabOrderContext<'_>, left: usize, right: usize) -> Ordering {
    order_direction_cmp(
        context.direction,
        tab_saved_millis(context.tab(left)),
        tab_saved_millis(context.tab(right)),
    )
    .then_with(|| context.custom_rank_cmp(left, right))
}

fn compare_recent_edit_tabs(context: &TabOrderContext<'_>, left: usize, right: usize) -> Ordering {
    let left_tab = context.tab(left);
    let right_tab = context.tab(right);
    order_direction_cmp(
        context.direction,
        tab_latest_edit_sequence(left_tab),
        tab_latest_edit_sequence(right_tab),
    )
    .then_with(|| {
        order_direction_cmp(
            context.direction,
            tab_saved_millis(left_tab),
            tab_saved_millis(right_tab),
        )
    })
    .then_with(|| context.custom_rank_cmp(left, right))
}

fn order_direction_cmp<T: Ord>(direction: TabOrderDirection, left: T, right: T) -> Ordering {
    match direction {
        TabOrderDirection::Ascending => left.cmp(&right),
        TabOrderDirection::Descending => right.cmp(&left),
    }
}

fn tab_latest_edit_sequence(tab: &WorkspaceTab) -> u64 {
    tab.buffers()
        .flat_map(|buffer| buffer.document().history_entries())
        .map(|entry| entry.global_seq)
        .max()
        .unwrap_or(0)
}

fn tab_saved_millis(tab: &WorkspaceTab) -> u64 {
    tab.buffer
        .disk_state
        .as_ref()
        .and_then(|state| state.modified_millis)
        .unwrap_or(u64::MAX)
}

fn workspace_tab_order_from_saved_custom_order(app: &ScratchpadApp) -> Vec<usize> {
    let mut order = Vec::with_capacity(app.tab_manager.tabs.as_slice().len());
    for buffer_id in &app.state.app_settings.workspace.custom_tab_order {
        if let Some(index) = app
            .tab_manager
            .tabs
            .as_slice()
            .iter()
            .position(|tab| tab.buffer.id == *buffer_id)
            && !order.contains(&index)
        {
            order.push(index);
        }
    }
    for index in 0..app.tab_manager.tabs.as_slice().len() {
        if !order.contains(&index) {
            order.push(index);
        }
    }
    order
}

fn custom_order_ranks(app: &ScratchpadApp) -> Vec<usize> {
    let saved_order = workspace_tab_order_from_saved_custom_order(app);
    let mut ranks = vec![usize::MAX; app.tab_manager.tabs.as_slice().len()];
    for (rank, index) in saved_order.into_iter().enumerate() {
        ranks[index] = rank;
    }
    ranks
}

fn tab_path_label(tab: &WorkspaceTab) -> String {
    tab.buffer
        .path
        .as_ref()
        .map(|path| path.display().to_string())
        .unwrap_or_default()
}

fn tab_total_size(tab: &WorkspaceTab) -> u64 {
    tab.buffers()
        .map(|buffer| {
            buffer
                .disk_state
                .as_ref()
                .map(|state| state.len)
                .unwrap_or_else(|| buffer.text().len() as u64)
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::{ScratchpadApp, TabOrderMode};
    use crate::app::domain::{BufferState, DiskFileState, PieceSource, TabManager, WorkspaceTab};
    use crate::app::services::session_store::SessionStore;
    use crate::app::services::settings_store::{SettingsStore, TabOrderDirection};
    use crate::app::startup::StartupOptions;
    use crate::app::ui::editor_content::native_editor::{
        CharCursor, CursorRange, EditOperation, OperationRecord,
    };

    #[test]
    fn file_name_order_sorts_workspace_tabs_and_preserves_active_tab() {
        let mut app = test_app(["zeta.txt", "Alpha.txt", "beta.txt"]);
        app.tab_manager.active_tab_index = 0;

        app.set_tab_order_mode(TabOrderMode::FileName);

        assert_eq!(tab_names(&app), ["Alpha.txt", "beta.txt", "zeta.txt"]);
        assert_eq!(
            app.tab_manager.active_tab().unwrap().buffer.name,
            "zeta.txt"
        );
    }

    #[test]
    fn file_age_order_uses_disk_modified_time_and_treats_unsaved_tabs_as_newest() {
        let mut app = test_app(["newer.txt", "untitled", "older.txt"]);
        app.tab_manager.tabs[0].buffer.disk_state = Some(DiskFileState {
            modified_millis: Some(300),
            len: 0,
        });
        app.tab_manager.tabs[2].buffer.disk_state = Some(DiskFileState {
            modified_millis: Some(100),
            len: 0,
        });

        app.set_tab_order_mode(TabOrderMode::FileAge);

        assert_eq!(tab_names(&app), ["older.txt", "newer.txt", "untitled"]);

        app.set_tab_order_direction(TabOrderDirection::Descending);

        assert_eq!(tab_names(&app), ["untitled", "newer.txt", "older.txt"]);
    }

    #[test]
    fn file_size_order_uses_disk_size_and_falls_back_to_buffer_text() {
        let mut app = test_app(["large.txt", "small.txt", "draft.txt"]);
        app.tab_manager.tabs[0].buffer.disk_state = Some(DiskFileState {
            modified_millis: None,
            len: 300,
        });
        app.tab_manager.tabs[1].buffer.disk_state = Some(DiskFileState {
            modified_millis: None,
            len: 10,
        });
        app.tab_manager.tabs[2]
            .buffer
            .replace_text("medium sized draft".to_owned());

        app.set_tab_order_mode(TabOrderMode::FileSize);

        assert_eq!(tab_names(&app), ["small.txt", "draft.txt", "large.txt"]);
    }

    #[test]
    fn recent_edit_order_uses_latest_text_history_sequence_then_save_date() {
        let mut app = test_app(["alpha.txt", "beta.txt", "gamma.txt"]);
        app.tab_manager.tabs[0].buffer.disk_state = Some(DiskFileState {
            modified_millis: Some(300),
            len: 0,
        });
        app.tab_manager.tabs[1].buffer.disk_state = Some(DiskFileState {
            modified_millis: Some(100),
            len: 0,
        });
        app.tab_manager.tabs[2].buffer.disk_state = Some(DiskFileState {
            modified_millis: Some(200),
            len: 0,
        });
        record_edit(&mut app.tab_manager.tabs[1].buffer, "b");
        record_edit(&mut app.tab_manager.tabs[2].buffer, "g");

        app.set_tab_order_mode(TabOrderMode::RecentEdit);

        assert_eq!(tab_names(&app), ["alpha.txt", "beta.txt", "gamma.txt"]);

        app.set_tab_order_direction(TabOrderDirection::Descending);

        assert_eq!(tab_names(&app), ["gamma.txt", "beta.txt", "alpha.txt"]);
    }

    #[test]
    fn manual_display_reorder_switches_back_to_custom_order() {
        let mut app = test_app(["alpha.txt", "beta.txt", "gamma.txt"]);
        app.set_tab_order_mode(TabOrderMode::FileName);

        assert!(app.reorder_display_tab(0, 2));

        assert_eq!(
            app.state.app_settings.tab_order_mode(),
            TabOrderMode::Custom
        );
        assert_eq!(tab_names(&app), ["beta.txt", "gamma.txt", "alpha.txt"]);
    }

    #[test]
    fn custom_order_restores_order_from_before_automatic_sort() {
        let mut app = test_app(["zeta.txt", "Alpha.txt", "beta.txt"]);

        app.set_tab_order_mode(TabOrderMode::FileName);
        assert_eq!(tab_names(&app), ["Alpha.txt", "beta.txt", "zeta.txt"]);

        app.set_tab_order_mode(TabOrderMode::Custom);

        assert_eq!(tab_names(&app), ["zeta.txt", "Alpha.txt", "beta.txt"]);
    }

    #[test]
    fn custom_order_survives_switching_between_automatic_modes() {
        let mut app = test_app(["zeta.txt", "Alpha.txt", "beta.txt"]);
        app.tab_manager.tabs[0].buffer.disk_state = Some(DiskFileState {
            modified_millis: Some(300),
            len: 0,
        });
        app.tab_manager.tabs[1].buffer.disk_state = Some(DiskFileState {
            modified_millis: Some(100),
            len: 0,
        });
        app.tab_manager.tabs[2].buffer.disk_state = Some(DiskFileState {
            modified_millis: Some(200),
            len: 0,
        });

        app.set_tab_order_mode(TabOrderMode::FileName);
        app.set_tab_order_mode(TabOrderMode::FileAge);
        assert_eq!(tab_names(&app), ["Alpha.txt", "beta.txt", "zeta.txt"]);

        app.set_tab_order_mode(TabOrderMode::Custom);

        assert_eq!(tab_names(&app), ["zeta.txt", "Alpha.txt", "beta.txt"]);
    }

    fn test_app<const N: usize>(names: [&str; N]) -> ScratchpadApp {
        let temp_dir = tempfile::tempdir().expect("create temp app root");
        let root = temp_dir.keep();
        let mut app = ScratchpadApp::with_stores_and_startup(
            SessionStore::new(root.clone()),
            SettingsStore::new(root),
            StartupOptions::default(),
        );
        app.set_session_persist_on_drop(false);
        app.tab_manager = TabManager {
            tabs: names.into_iter().map(test_tab).collect(),
            active_tab_index: 0,
            pending_action: None,
            session_dirty: false,
            pending_scroll_to_active: false,
            buffer_tab_index: Default::default(),
            cold_session_tabs: Default::default(),
        };
        app.tab_manager.rebuild_buffer_tab_index();
        app.clear_tab_selection();
        app
    }

    fn test_tab(name: &str) -> WorkspaceTab {
        WorkspaceTab::new(BufferState::new(name.to_owned(), String::new(), None))
    }

    fn tab_names(app: &ScratchpadApp) -> Vec<String> {
        app.tab_manager
            .tabs
            .as_slice()
            .iter()
            .map(|tab| tab.buffer.name.clone())
            .collect()
    }

    fn record_edit(buffer: &mut BufferState, text: &str) {
        buffer.document_mut().insert_direct(0, text);
        buffer.push_text_edit_operation_with_source(
            OperationRecord {
                previous_cursor: CursorRange::one(CharCursor::new(0)),
                next_cursor: CursorRange::one(CharCursor::new(text.chars().count())),
                edits: vec![EditOperation {
                    start_char: 0,
                    deleted_text: String::new(),
                    inserted_text: text.to_owned(),
                    deleted_spans: Vec::new(),
                }],
            },
            PieceSource::Edit,
        );
    }
}
