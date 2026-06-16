use crate::app::platform::PlatformProfile;
use eframe::egui;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ShortcutAction {
    OpenUserManual,
    OpenSearch,
    OpenReplace,
    OpenSettings,
    CloseSettings,
    CloseSearch,
    RenameTab,
    OpenTextHistory,
    OpenEncodingDialog,
    OpenStatusHistory,
    CopyActivePath,
    RevealActivePath,
    ToggleTabListAutoHide,
    ToggleReadingOrder,
    ToggleControlChars,
    TraverseRegionForward,
    TraverseRegionBackward,
    OpenFileHere,
    NewTab,
    OpenFile,
    SaveFileAs,
    SaveFile,
    IncreaseFontSize,
    DecreaseFontSize,
    ToggleLineNumbers,
    CloseTab,
    PromoteTileToTab,
    PromoteTabFilesToTabs,
    CloseTile,
    SplitUp,
    SplitDown,
    SplitLeft,
    SplitRight,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyBinding {
    pub modifiers: egui::Modifiers,
    pub key: egui::Key,
}

impl KeyBinding {
    const fn new(modifiers: egui::Modifiers, key: egui::Key) -> Self {
        Self { modifiers, key }
    }
}

const CTRL_SHIFT: egui::Modifiers = egui::Modifiers {
    alt: false,
    ctrl: true,
    shift: true,
    mac_cmd: false,
    command: false,
};
const CTRL_ALT: egui::Modifiers = egui::Modifiers {
    alt: true,
    ctrl: true,
    shift: false,
    mac_cmd: false,
    command: false,
};

const OPEN_USER_MANUAL: [KeyBinding; 1] = [KeyBinding::new(egui::Modifiers::NONE, egui::Key::F1)];
const OPEN_SEARCH: [KeyBinding; 1] = [KeyBinding::new(egui::Modifiers::CTRL, egui::Key::F)];
const OPEN_REPLACE: [KeyBinding; 1] = [KeyBinding::new(egui::Modifiers::CTRL, egui::Key::H)];
const OPEN_SETTINGS: [KeyBinding; 1] = [KeyBinding::new(egui::Modifiers::CTRL, egui::Key::Comma)];
const CLOSE_SETTINGS: [KeyBinding; 1] = [KeyBinding::new(egui::Modifiers::NONE, egui::Key::Escape)];
const CLOSE_SEARCH: [KeyBinding; 1] = [KeyBinding::new(egui::Modifiers::NONE, egui::Key::Escape)];
const RENAME_TAB: [KeyBinding; 1] = [KeyBinding::new(egui::Modifiers::NONE, egui::Key::F2)];
const OPEN_TEXT_HISTORY: [KeyBinding; 1] = [KeyBinding::new(CTRL_SHIFT, egui::Key::H)];
const OPEN_ENCODING_DIALOG: [KeyBinding; 1] = [KeyBinding::new(CTRL_SHIFT, egui::Key::E)];
const OPEN_STATUS_HISTORY: [KeyBinding; 1] = [KeyBinding::new(CTRL_SHIFT, egui::Key::M)];
const COPY_ACTIVE_PATH: [KeyBinding; 1] = [KeyBinding::new(CTRL_SHIFT, egui::Key::C)];
const REVEAL_ACTIVE_PATH: [KeyBinding; 1] = [KeyBinding::new(CTRL_SHIFT, egui::Key::R)];
const TOGGLE_TAB_LIST_AUTO_HIDE: [KeyBinding; 1] = [KeyBinding::new(CTRL_SHIFT, egui::Key::B)];
const TOGGLE_READING_ORDER: [KeyBinding; 1] = [KeyBinding::new(CTRL_ALT, egui::Key::R)];
const TOGGLE_CONTROL_CHARS: [KeyBinding; 1] = [KeyBinding::new(CTRL_ALT, egui::Key::C)];
const TRAVERSE_REGION_FORWARD: [KeyBinding; 1] =
    [KeyBinding::new(egui::Modifiers::NONE, egui::Key::F6)];
const TRAVERSE_REGION_BACKWARD: [KeyBinding; 1] =
    [KeyBinding::new(egui::Modifiers::SHIFT, egui::Key::F6)];
const OPEN_FILE_HERE: [KeyBinding; 1] = [KeyBinding::new(CTRL_SHIFT, egui::Key::O)];
const NEW_TAB: [KeyBinding; 1] = [KeyBinding::new(egui::Modifiers::CTRL, egui::Key::N)];
const OPEN_FILE: [KeyBinding; 1] = [KeyBinding::new(egui::Modifiers::CTRL, egui::Key::O)];
const SAVE_FILE_AS: [KeyBinding; 1] = [KeyBinding::new(CTRL_SHIFT, egui::Key::S)];
const SAVE_FILE: [KeyBinding; 1] = [KeyBinding::new(egui::Modifiers::CTRL, egui::Key::S)];
const INCREASE_FONT_SIZE: [KeyBinding; 2] = [
    KeyBinding::new(egui::Modifiers::CTRL, egui::Key::Equals),
    KeyBinding::new(egui::Modifiers::CTRL, egui::Key::Plus),
];
const DECREASE_FONT_SIZE: [KeyBinding; 1] =
    [KeyBinding::new(egui::Modifiers::CTRL, egui::Key::Minus)];
const TOGGLE_LINE_NUMBERS: [KeyBinding; 1] =
    [KeyBinding::new(egui::Modifiers::CTRL, egui::Key::Num0)];
const CLOSE_TAB: [KeyBinding; 1] = [KeyBinding::new(egui::Modifiers::CTRL, egui::Key::W)];
const PROMOTE_TILE_TO_TAB: [KeyBinding; 1] = [KeyBinding::new(egui::Modifiers::CTRL, egui::Key::T)];
const PROMOTE_TAB_FILES_TO_TABS: [KeyBinding; 1] = [KeyBinding::new(CTRL_SHIFT, egui::Key::T)];
const CLOSE_TILE: [KeyBinding; 1] = [KeyBinding::new(CTRL_SHIFT, egui::Key::W)];
const SPLIT_UP: [KeyBinding; 1] = [KeyBinding::new(CTRL_SHIFT, egui::Key::ArrowUp)];
const SPLIT_DOWN: [KeyBinding; 1] = [KeyBinding::new(CTRL_SHIFT, egui::Key::ArrowDown)];
const SPLIT_LEFT: [KeyBinding; 1] = [KeyBinding::new(CTRL_SHIFT, egui::Key::ArrowLeft)];
const SPLIT_RIGHT: [KeyBinding; 1] = [KeyBinding::new(CTRL_SHIFT, egui::Key::ArrowRight)];

#[must_use]
pub fn default_bindings(
    _profile: PlatformProfile,
    action: ShortcutAction,
) -> &'static [KeyBinding] {
    match action {
        ShortcutAction::OpenUserManual => &OPEN_USER_MANUAL,
        ShortcutAction::OpenSearch => &OPEN_SEARCH,
        ShortcutAction::OpenReplace => &OPEN_REPLACE,
        ShortcutAction::OpenSettings => &OPEN_SETTINGS,
        ShortcutAction::CloseSettings => &CLOSE_SETTINGS,
        ShortcutAction::CloseSearch => &CLOSE_SEARCH,
        ShortcutAction::RenameTab => &RENAME_TAB,
        ShortcutAction::OpenTextHistory => &OPEN_TEXT_HISTORY,
        ShortcutAction::OpenEncodingDialog => &OPEN_ENCODING_DIALOG,
        ShortcutAction::OpenStatusHistory => &OPEN_STATUS_HISTORY,
        ShortcutAction::CopyActivePath => &COPY_ACTIVE_PATH,
        ShortcutAction::RevealActivePath => &REVEAL_ACTIVE_PATH,
        ShortcutAction::ToggleTabListAutoHide => &TOGGLE_TAB_LIST_AUTO_HIDE,
        ShortcutAction::ToggleReadingOrder => &TOGGLE_READING_ORDER,
        ShortcutAction::ToggleControlChars => &TOGGLE_CONTROL_CHARS,
        ShortcutAction::TraverseRegionForward => &TRAVERSE_REGION_FORWARD,
        ShortcutAction::TraverseRegionBackward => &TRAVERSE_REGION_BACKWARD,
        ShortcutAction::OpenFileHere => &OPEN_FILE_HERE,
        ShortcutAction::NewTab => &NEW_TAB,
        ShortcutAction::OpenFile => &OPEN_FILE,
        ShortcutAction::SaveFileAs => &SAVE_FILE_AS,
        ShortcutAction::SaveFile => &SAVE_FILE,
        ShortcutAction::IncreaseFontSize => &INCREASE_FONT_SIZE,
        ShortcutAction::DecreaseFontSize => &DECREASE_FONT_SIZE,
        ShortcutAction::ToggleLineNumbers => &TOGGLE_LINE_NUMBERS,
        ShortcutAction::CloseTab => &CLOSE_TAB,
        ShortcutAction::PromoteTileToTab => &PROMOTE_TILE_TO_TAB,
        ShortcutAction::PromoteTabFilesToTabs => &PROMOTE_TAB_FILES_TO_TABS,
        ShortcutAction::CloseTile => &CLOSE_TILE,
        ShortcutAction::SplitUp => &SPLIT_UP,
        ShortcutAction::SplitDown => &SPLIT_DOWN,
        ShortcutAction::SplitLeft => &SPLIT_LEFT,
        ShortcutAction::SplitRight => &SPLIT_RIGHT,
    }
}

pub fn consume_shortcut(
    ctx: &egui::Context,
    profile: PlatformProfile,
    action: ShortcutAction,
) -> bool {
    default_bindings(profile, action)
        .iter()
        .any(|binding| ctx.input_mut(|input| input.consume_key(binding.modifiers, binding.key)))
}

#[cfg(test)]
mod tests {
    use super::{ShortcutAction, default_bindings};
    use crate::app::platform::PlatformProfile;
    use eframe::egui;

    #[test]
    fn windows_default_shortcuts_keep_existing_bindings() {
        assert_eq!(
            default_bindings(PlatformProfile::Windows, ShortcutAction::OpenFile),
            &[super::KeyBinding::new(egui::Modifiers::CTRL, egui::Key::O)]
        );
        assert_eq!(
            default_bindings(PlatformProfile::Windows, ShortcutAction::CloseTab),
            &[super::KeyBinding::new(egui::Modifiers::CTRL, egui::Key::W)]
        );
    }

    #[test]
    fn increase_font_size_keeps_both_existing_bindings() {
        assert_eq!(
            default_bindings(PlatformProfile::Windows, ShortcutAction::IncreaseFontSize),
            &[
                super::KeyBinding::new(egui::Modifiers::CTRL, egui::Key::Equals),
                super::KeyBinding::new(egui::Modifiers::CTRL, egui::Key::Plus),
            ]
        );
    }
}
