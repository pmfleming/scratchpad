use crate::app::platform::PlatformProfile;
use crate::app::services::settings_store::ShortcutSettings;
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

impl ShortcutAction {
    #[must_use]
    pub const fn config_key(self) -> &'static str {
        match self {
            Self::OpenUserManual => "open_user_manual",
            Self::OpenSearch => "open_search",
            Self::OpenReplace => "open_replace",
            Self::OpenSettings => "open_settings",
            Self::CloseSettings => "close_settings",
            Self::CloseSearch => "close_search",
            Self::RenameTab => "rename_tab",
            Self::OpenTextHistory => "open_text_history",
            Self::OpenEncodingDialog => "open_encoding_dialog",
            Self::OpenStatusHistory => "open_status_history",
            Self::CopyActivePath => "copy_active_path",
            Self::RevealActivePath => "reveal_active_path",
            Self::ToggleTabListAutoHide => "toggle_tab_list_auto_hide",
            Self::ToggleReadingOrder => "toggle_reading_order",
            Self::ToggleControlChars => "toggle_control_chars",
            Self::TraverseRegionForward => "traverse_region_forward",
            Self::TraverseRegionBackward => "traverse_region_backward",
            Self::OpenFileHere => "open_file_here",
            Self::NewTab => "new_tab",
            Self::OpenFile => "open_file",
            Self::SaveFileAs => "save_file_as",
            Self::SaveFile => "save_file",
            Self::IncreaseFontSize => "increase_font_size",
            Self::DecreaseFontSize => "decrease_font_size",
            Self::ToggleLineNumbers => "toggle_line_numbers",
            Self::CloseTab => "close_tab",
            Self::PromoteTileToTab => "promote_tile_to_tab",
            Self::PromoteTabFilesToTabs => "promote_tab_files_to_tabs",
            Self::CloseTile => "close_tile",
            Self::SplitUp => "split_up",
            Self::SplitDown => "split_down",
            Self::SplitLeft => "split_left",
            Self::SplitRight => "split_right",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyBinding {
    pub modifiers: egui::Modifiers,
    pub key: egui::Key,
}

impl KeyBinding {
    #[must_use]
    pub const fn new(modifiers: egui::Modifiers, key: egui::Key) -> Self {
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

#[must_use]
pub fn consume_shortcut(
    ctx: &egui::Context,
    profile: PlatformProfile,
    shortcuts: &ShortcutSettings,
    action: ShortcutAction,
) -> bool {
    if let Some(bindings) = configured_bindings(shortcuts, action) {
        return bindings.iter().any(|binding| {
            ctx.input_mut(|input| input.consume_key(binding.modifiers, binding.key))
        });
    }

    default_bindings(profile, action)
        .iter()
        .any(|binding| ctx.input_mut(|input| input.consume_key(binding.modifiers, binding.key)))
}

#[must_use]
pub fn configured_bindings(
    shortcuts: &ShortcutSettings,
    action: ShortcutAction,
) -> Option<Vec<KeyBinding>> {
    shortcuts
        .binding(action.config_key())
        .and_then(parse_binding_list)
}

fn parse_binding_list(raw: &str) -> Option<Vec<KeyBinding>> {
    let bindings = raw
        .split(',')
        .map(parse_key_binding)
        .collect::<Option<Vec<_>>>()?;

    (!bindings.is_empty()).then_some(bindings)
}

fn parse_key_binding(raw: &str) -> Option<KeyBinding> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    let mut modifiers = egui::Modifiers::NONE;
    let mut key = None;
    for part in trimmed.split('+') {
        let token = normalized_token(part);
        match token.as_str() {
            "ctrl" | "control" => modifiers.ctrl = true,
            "shift" => modifiers.shift = true,
            "alt" | "option" => modifiers.alt = true,
            "cmd" | "command" | "super" | "win" => modifiers.command = true,
            _ => {
                if key.is_some() {
                    return None;
                }
                key = parse_key_token(&token);
                key?;
            }
        }
    }

    key.map(|key| KeyBinding { modifiers, key })
}

fn normalized_token(token: &str) -> String {
    token
        .chars()
        .filter(|ch| !ch.is_whitespace() && *ch != '_' && *ch != '-')
        .collect::<String>()
        .to_ascii_lowercase()
}

fn parse_key_token(token: &str) -> Option<egui::Key> {
    match token {
        "a" => Some(egui::Key::A),
        "b" => Some(egui::Key::B),
        "c" => Some(egui::Key::C),
        "d" => Some(egui::Key::D),
        "e" => Some(egui::Key::E),
        "f" => Some(egui::Key::F),
        "g" => Some(egui::Key::G),
        "h" => Some(egui::Key::H),
        "i" => Some(egui::Key::I),
        "j" => Some(egui::Key::J),
        "k" => Some(egui::Key::K),
        "l" => Some(egui::Key::L),
        "m" => Some(egui::Key::M),
        "n" => Some(egui::Key::N),
        "o" => Some(egui::Key::O),
        "p" => Some(egui::Key::P),
        "q" => Some(egui::Key::Q),
        "r" => Some(egui::Key::R),
        "s" => Some(egui::Key::S),
        "t" => Some(egui::Key::T),
        "u" => Some(egui::Key::U),
        "v" => Some(egui::Key::V),
        "w" => Some(egui::Key::W),
        "x" => Some(egui::Key::X),
        "y" => Some(egui::Key::Y),
        "z" => Some(egui::Key::Z),
        "0" | "num0" => Some(egui::Key::Num0),
        "1" | "num1" => Some(egui::Key::Num1),
        "2" | "num2" => Some(egui::Key::Num2),
        "3" | "num3" => Some(egui::Key::Num3),
        "4" | "num4" => Some(egui::Key::Num4),
        "5" | "num5" => Some(egui::Key::Num5),
        "6" | "num6" => Some(egui::Key::Num6),
        "7" | "num7" => Some(egui::Key::Num7),
        "8" | "num8" => Some(egui::Key::Num8),
        "9" | "num9" => Some(egui::Key::Num9),
        "f1" => Some(egui::Key::F1),
        "f2" => Some(egui::Key::F2),
        "f3" => Some(egui::Key::F3),
        "f4" => Some(egui::Key::F4),
        "f5" => Some(egui::Key::F5),
        "f6" => Some(egui::Key::F6),
        "f7" => Some(egui::Key::F7),
        "f8" => Some(egui::Key::F8),
        "f9" => Some(egui::Key::F9),
        "f10" => Some(egui::Key::F10),
        "f11" => Some(egui::Key::F11),
        "f12" => Some(egui::Key::F12),
        "up" | "arrowup" => Some(egui::Key::ArrowUp),
        "down" | "arrowdown" => Some(egui::Key::ArrowDown),
        "left" | "arrowleft" => Some(egui::Key::ArrowLeft),
        "right" | "arrowright" => Some(egui::Key::ArrowRight),
        "escape" | "esc" => Some(egui::Key::Escape),
        "enter" | "return" => Some(egui::Key::Enter),
        "tab" => Some(egui::Key::Tab),
        "backspace" => Some(egui::Key::Backspace),
        "delete" | "del" => Some(egui::Key::Delete),
        "space" => Some(egui::Key::Space),
        "comma" | "," => Some(egui::Key::Comma),
        "equals" | "equal" | "=" => Some(egui::Key::Equals),
        "plus" => Some(egui::Key::Plus),
        "minus" | "dash" => Some(egui::Key::Minus),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        KeyBinding, ShortcutAction, configured_bindings, default_bindings, parse_key_binding,
    };
    use crate::app::platform::PlatformProfile;
    use crate::app::services::settings_store::ShortcutSettings;
    use eframe::egui;
    use std::collections::BTreeMap;

    #[test]
    fn windows_default_shortcuts_keep_existing_bindings() {
        assert_eq!(
            default_bindings(PlatformProfile::Windows, ShortcutAction::OpenFile),
            &[KeyBinding::new(egui::Modifiers::CTRL, egui::Key::O)]
        );
        assert_eq!(
            default_bindings(PlatformProfile::Windows, ShortcutAction::CloseTab),
            &[KeyBinding::new(egui::Modifiers::CTRL, egui::Key::W)]
        );
    }

    #[test]
    fn increase_font_size_keeps_both_existing_bindings() {
        assert_eq!(
            default_bindings(PlatformProfile::Windows, ShortcutAction::IncreaseFontSize),
            &[
                KeyBinding::new(egui::Modifiers::CTRL, egui::Key::Equals),
                KeyBinding::new(egui::Modifiers::CTRL, egui::Key::Plus),
            ]
        );
    }

    #[test]
    fn parses_configured_shortcut_binding() {
        assert_eq!(
            parse_key_binding("ctrl + shift + left"),
            Some(KeyBinding::new(super::CTRL_SHIFT, egui::Key::ArrowLeft))
        );
    }

    #[test]
    fn configured_binding_overrides_default_binding() {
        let shortcuts = ShortcutSettings {
            bindings: BTreeMap::from([("open_file".to_owned(), "ctrl+shift+p".to_owned())]),
        };

        assert_eq!(
            configured_bindings(&shortcuts, ShortcutAction::OpenFile),
            Some(vec![KeyBinding::new(super::CTRL_SHIFT, egui::Key::P)])
        );
    }

    #[test]
    fn invalid_configured_binding_falls_back_to_default() {
        let shortcuts = ShortcutSettings {
            bindings: BTreeMap::from([("open_file".to_owned(), "ctrl+bogus".to_owned())]),
        };

        assert_eq!(
            configured_bindings(&shortcuts, ShortcutAction::OpenFile),
            None
        );
        assert_eq!(
            default_bindings(PlatformProfile::Windows, ShortcutAction::OpenFile),
            &[KeyBinding::new(egui::Modifiers::CTRL, egui::Key::O)]
        );
    }
}
