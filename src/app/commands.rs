use crate::app::app_state::SearchScope;
use crate::app::domain::{SplitAxis, SplitPath, ViewId};
use crate::app::services::search::SearchMode;

mod dialogs;
mod dispatch;
mod edit;
mod file;
mod search;
mod settings;
mod tab_transfer;
mod workspace;

pub(crate) use dialogs::close_text_history;
pub(crate) use dispatch::handle_command;
pub(crate) use workspace::{activate_pending_view_command, perform_close_view};

pub enum AppCommand {
    Workspace(WorkspaceCommand),
    Search(SearchCommand),
    File(FileCommand),
    Dialog(DialogCommand),
    Settings(SettingsCommand),
    Edit(EditCommand),
}

pub enum WorkspaceCommand {
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
    CombineTabIntoTab {
        source_index: usize,
        target_index: usize,
    },
    CombineTabsIntoTab {
        source_indices: Vec<usize>,
        target_index: usize,
    },
    PromoteViewToTab {
        view_id: ViewId,
    },
    PromoteTabFilesToTabs {
        index: usize,
    },
    NewTab,
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
    SplitActiveView {
        axis: SplitAxis,
        new_view_first: bool,
        ratio: f32,
    },
}

pub enum SearchCommand {
    Open,
    OpenAndReplace,
    Close,
    Toggle,
    SetSearchQuery { query: String },
    SetSearchReplacement { replacement: String },
    SetSearchReplaceOpen { open: bool },
    SetSearchScope { scope: SearchScope },
    SetSearchMode { mode: SearchMode },
    SetSearchMatchCase { enabled: bool },
    SetSearchWholeWord { enabled: bool },
    FocusSearchResultFile { match_index: usize },
    ActivateSearchMatch { match_index: usize },
    NextSearchMatch,
    PreviousSearchMatch,
    ReplaceCurrentMatch,
    ReplaceAllMatches,
}

pub enum FileCommand {
    OpenFile,
    OpenFileHere,
    OpenUserManual,
    ReopenBufferWithEncoding {
        tab_index: usize,
        encoding_name: String,
    },
    SaveFile,
    SaveAllFiles,
    SaveFileAs,
    SaveFileWithEncoding {
        tab_index: usize,
        encoding_name: String,
    },
    SaveConflictOverwrite {
        tab_index: usize,
        view_id: ViewId,
    },
    ReloadBufferFromDisk {
        tab_index: usize,
        view_id: ViewId,
    },
    SaveConflictAsCopy {
        tab_index: usize,
        view_id: ViewId,
    },
}

pub enum DialogCommand {
    OpenTextHistory,
}

pub enum SettingsCommand {
    OpenSettings,
    CloseSettings,
}

pub enum EditCommand {
    UndoActiveBufferTextOperation,
    RedoActiveBufferTextOperation,
}
