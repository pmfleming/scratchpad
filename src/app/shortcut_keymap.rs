use crate::app::platform::{PlatformProfile, resolved_profile};
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
    ToggleTabList,
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
    SplitTile,
    SplitUp,
    SplitDown,
    SplitLeft,
    SplitRight,
    ResizeTileLeft,
    ResizeTileRight,
    ResizeTileUp,
    ResizeTileDown,
    MoveTileLeft,
    MoveTileRight,
    MoveTileUp,
    MoveTileDown,
}

impl ShortcutAction {
    pub(crate) const ALL: [Self; 43] = [
        Self::OpenUserManual,
        Self::OpenSearch,
        Self::OpenReplace,
        Self::OpenSettings,
        Self::CloseSettings,
        Self::CloseSearch,
        Self::RenameTab,
        Self::OpenTextHistory,
        Self::OpenEncodingDialog,
        Self::OpenStatusHistory,
        Self::CopyActivePath,
        Self::RevealActivePath,
        Self::ToggleTabList,
        Self::ToggleTabListAutoHide,
        Self::ToggleReadingOrder,
        Self::ToggleControlChars,
        Self::TraverseRegionForward,
        Self::TraverseRegionBackward,
        Self::OpenFileHere,
        Self::NewTab,
        Self::OpenFile,
        Self::SaveFileAs,
        Self::SaveFile,
        Self::IncreaseFontSize,
        Self::DecreaseFontSize,
        Self::ToggleLineNumbers,
        Self::CloseTab,
        Self::PromoteTileToTab,
        Self::PromoteTabFilesToTabs,
        Self::CloseTile,
        Self::SplitTile,
        Self::SplitUp,
        Self::SplitDown,
        Self::SplitLeft,
        Self::SplitRight,
        Self::ResizeTileLeft,
        Self::ResizeTileRight,
        Self::ResizeTileUp,
        Self::ResizeTileDown,
        Self::MoveTileLeft,
        Self::MoveTileRight,
        Self::MoveTileUp,
        Self::MoveTileDown,
    ];

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
            Self::ToggleTabList => "toggle_tab_list",
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
            Self::SplitTile => "split_tile",
            Self::SplitUp => "split_up",
            Self::SplitDown => "split_down",
            Self::SplitLeft => "split_left",
            Self::SplitRight => "split_right",
            Self::ResizeTileLeft => "resize_tile_left",
            Self::ResizeTileRight => "resize_tile_right",
            Self::ResizeTileUp => "resize_tile_up",
            Self::ResizeTileDown => "resize_tile_down",
            Self::MoveTileLeft => "move_tile_left",
            Self::MoveTileRight => "move_tile_right",
            Self::MoveTileUp => "move_tile_up",
            Self::MoveTileDown => "move_tile_down",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct InvalidShortcutOverride {
    pub(crate) action_key: &'static str,
    pub(crate) raw: String,
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
const ALT_SHIFT: egui::Modifiers = egui::Modifiers {
    alt: true,
    ctrl: false,
    shift: true,
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
const TOGGLE_TAB_LIST: [KeyBinding; 1] = [KeyBinding::new(CTRL_ALT, egui::Key::B)];
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
const GENERIC_SPLIT_TILE: [KeyBinding; 1] = [KeyBinding::new(CTRL_ALT, egui::Key::Enter)];
const GENERIC_SPLIT_UP: [KeyBinding; 1] = [KeyBinding::new(CTRL_SHIFT, egui::Key::ArrowUp)];
const GENERIC_SPLIT_DOWN: [KeyBinding; 1] = [KeyBinding::new(CTRL_SHIFT, egui::Key::ArrowDown)];
const GENERIC_SPLIT_LEFT: [KeyBinding; 1] = [KeyBinding::new(CTRL_SHIFT, egui::Key::ArrowLeft)];
const GENERIC_SPLIT_RIGHT: [KeyBinding; 1] = [KeyBinding::new(CTRL_SHIFT, egui::Key::ArrowRight)];
const GENERIC_RESIZE_LEFT: [KeyBinding; 1] = [KeyBinding::new(CTRL_ALT, egui::Key::ArrowLeft)];
const GENERIC_RESIZE_RIGHT: [KeyBinding; 1] = [KeyBinding::new(CTRL_ALT, egui::Key::ArrowRight)];
const GENERIC_RESIZE_UP: [KeyBinding; 1] = [KeyBinding::new(CTRL_ALT, egui::Key::ArrowUp)];
const GENERIC_RESIZE_DOWN: [KeyBinding; 1] = [KeyBinding::new(CTRL_ALT, egui::Key::ArrowDown)];
const CTRL_ALT_SHIFT: egui::Modifiers = egui::Modifiers {
    alt: true,
    ctrl: true,
    shift: true,
    mac_cmd: false,
    command: false,
};
// Keep generic-platform tile movement away from the editor's Ctrl/Alt+Arrow
// word-navigation bindings. Hyprland has its own compositor-style defaults.
const GENERIC_MOVE_LEFT: [KeyBinding; 1] = [KeyBinding::new(CTRL_ALT_SHIFT, egui::Key::ArrowLeft)];
const GENERIC_MOVE_RIGHT: [KeyBinding; 1] =
    [KeyBinding::new(CTRL_ALT_SHIFT, egui::Key::ArrowRight)];
const GENERIC_MOVE_UP: [KeyBinding; 1] = [KeyBinding::new(CTRL_ALT_SHIFT, egui::Key::ArrowUp)];
const GENERIC_MOVE_DOWN: [KeyBinding; 1] = [KeyBinding::new(CTRL_ALT_SHIFT, egui::Key::ArrowDown)];

const HYPRLAND_SPLIT_TILE: [KeyBinding; 1] =
    [KeyBinding::new(egui::Modifiers::ALT, egui::Key::Enter)];
const HYPRLAND_SPLIT_UP: [KeyBinding; 1] = [KeyBinding::new(CTRL_ALT, egui::Key::ArrowUp)];
const HYPRLAND_SPLIT_DOWN: [KeyBinding; 1] = [KeyBinding::new(CTRL_ALT, egui::Key::ArrowDown)];
const HYPRLAND_SPLIT_LEFT: [KeyBinding; 1] = [KeyBinding::new(CTRL_ALT, egui::Key::ArrowLeft)];
const HYPRLAND_SPLIT_RIGHT: [KeyBinding; 1] = [KeyBinding::new(CTRL_ALT, egui::Key::ArrowRight)];
const HYPRLAND_RESIZE_LEFT: [KeyBinding; 1] = [KeyBinding::new(ALT_SHIFT, egui::Key::ArrowLeft)];
const HYPRLAND_RESIZE_RIGHT: [KeyBinding; 1] = [KeyBinding::new(ALT_SHIFT, egui::Key::ArrowRight)];
const HYPRLAND_RESIZE_UP: [KeyBinding; 1] = [KeyBinding::new(ALT_SHIFT, egui::Key::ArrowUp)];
const HYPRLAND_RESIZE_DOWN: [KeyBinding; 1] = [KeyBinding::new(ALT_SHIFT, egui::Key::ArrowDown)];
const HYPRLAND_MOVE_LEFT: [KeyBinding; 1] =
    [KeyBinding::new(egui::Modifiers::ALT, egui::Key::ArrowLeft)];
const HYPRLAND_MOVE_RIGHT: [KeyBinding; 1] =
    [KeyBinding::new(egui::Modifiers::ALT, egui::Key::ArrowRight)];
const HYPRLAND_MOVE_UP: [KeyBinding; 1] =
    [KeyBinding::new(egui::Modifiers::ALT, egui::Key::ArrowUp)];
const HYPRLAND_MOVE_DOWN: [KeyBinding; 1] =
    [KeyBinding::new(egui::Modifiers::ALT, egui::Key::ArrowDown)];

fn tile_default_bindings(
    profile: PlatformProfile,
    action: ShortcutAction,
) -> Option<&'static [KeyBinding]> {
    let hyprland = resolved_profile(profile) == PlatformProfile::Hyprland;
    Some(match (hyprland, action) {
        (false, ShortcutAction::SplitTile) => &GENERIC_SPLIT_TILE,
        (false, ShortcutAction::SplitUp) => &GENERIC_SPLIT_UP,
        (false, ShortcutAction::SplitDown) => &GENERIC_SPLIT_DOWN,
        (false, ShortcutAction::SplitLeft) => &GENERIC_SPLIT_LEFT,
        (false, ShortcutAction::SplitRight) => &GENERIC_SPLIT_RIGHT,
        (false, ShortcutAction::ResizeTileLeft) => &GENERIC_RESIZE_LEFT,
        (false, ShortcutAction::ResizeTileRight) => &GENERIC_RESIZE_RIGHT,
        (false, ShortcutAction::ResizeTileUp) => &GENERIC_RESIZE_UP,
        (false, ShortcutAction::ResizeTileDown) => &GENERIC_RESIZE_DOWN,
        (false, ShortcutAction::MoveTileLeft) => &GENERIC_MOVE_LEFT,
        (false, ShortcutAction::MoveTileRight) => &GENERIC_MOVE_RIGHT,
        (false, ShortcutAction::MoveTileUp) => &GENERIC_MOVE_UP,
        (false, ShortcutAction::MoveTileDown) => &GENERIC_MOVE_DOWN,
        (true, ShortcutAction::SplitTile) => &HYPRLAND_SPLIT_TILE,
        (true, ShortcutAction::SplitUp) => &HYPRLAND_SPLIT_UP,
        (true, ShortcutAction::SplitDown) => &HYPRLAND_SPLIT_DOWN,
        (true, ShortcutAction::SplitLeft) => &HYPRLAND_SPLIT_LEFT,
        (true, ShortcutAction::SplitRight) => &HYPRLAND_SPLIT_RIGHT,
        (true, ShortcutAction::ResizeTileLeft) => &HYPRLAND_RESIZE_LEFT,
        (true, ShortcutAction::ResizeTileRight) => &HYPRLAND_RESIZE_RIGHT,
        (true, ShortcutAction::ResizeTileUp) => &HYPRLAND_RESIZE_UP,
        (true, ShortcutAction::ResizeTileDown) => &HYPRLAND_RESIZE_DOWN,
        (true, ShortcutAction::MoveTileLeft) => &HYPRLAND_MOVE_LEFT,
        (true, ShortcutAction::MoveTileRight) => &HYPRLAND_MOVE_RIGHT,
        (true, ShortcutAction::MoveTileUp) => &HYPRLAND_MOVE_UP,
        (true, ShortcutAction::MoveTileDown) => &HYPRLAND_MOVE_DOWN,
        _ => return None,
    })
}

#[must_use]
pub fn default_bindings(profile: PlatformProfile, action: ShortcutAction) -> &'static [KeyBinding] {
    if let Some(bindings) = tile_default_bindings(profile, action) {
        return bindings;
    }
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
        ShortcutAction::ToggleTabList => &TOGGLE_TAB_LIST,
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
        ShortcutAction::SplitTile
        | ShortcutAction::SplitUp
        | ShortcutAction::SplitDown
        | ShortcutAction::SplitLeft
        | ShortcutAction::SplitRight
        | ShortcutAction::ResizeTileLeft
        | ShortcutAction::ResizeTileRight
        | ShortcutAction::ResizeTileUp
        | ShortcutAction::ResizeTileDown
        | ShortcutAction::MoveTileLeft
        | ShortcutAction::MoveTileRight
        | ShortcutAction::MoveTileUp
        | ShortcutAction::MoveTileDown => unreachable!("tile bindings are handled above"),
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
        return bindings
            .iter()
            .any(|binding| consume_binding(ctx, *binding));
    }
    default_bindings(profile, action)
        .iter()
        .any(|binding| consume_binding(ctx, *binding))
}

fn consume_binding(ctx: &egui::Context, binding: KeyBinding) -> bool {
    let pressed = ctx.input(|input| {
        input.events.iter().any(|event| {
            matches!(
                event,
                egui::Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } if *key == binding.key && key_event_matches_binding(*modifiers, binding)
            )
        })
    });
    pressed && ctx.input_mut(|input| input.consume_key(binding.modifiers, binding.key))
}

/// Uses exact modifiers for shortcuts so, for example, Ctrl+Shift+H does not
/// accidentally trigger Ctrl+H and Alt+Shift+Arrow does not trigger Alt+Arrow.
/// Plus/equals retain egui's logical matching because producing `+` requires
/// Shift on common keyboard layouts.
#[must_use]
pub(crate) fn key_event_matches_binding(pressed: egui::Modifiers, binding: KeyBinding) -> bool {
    if matches!(binding.key, egui::Key::Plus | egui::Key::Equals) {
        pressed.matches_logically(binding.modifiers)
    } else {
        pressed.matches_exact(binding.modifiers)
    }
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

/// Returns the bindings actually used after applying valid user overrides and
/// resolving platform-specific defaults.
#[must_use]
pub fn effective_bindings(
    profile: PlatformProfile,
    shortcuts: &ShortcutSettings,
    action: ShortcutAction,
) -> Vec<KeyBinding> {
    configured_bindings(shortcuts, action)
        .unwrap_or_else(|| default_bindings(profile, action).to_vec())
}

#[must_use]
pub(crate) fn invalid_shortcut_overrides(
    shortcuts: &ShortcutSettings,
) -> Vec<InvalidShortcutOverride> {
    ShortcutAction::ALL
        .iter()
        .filter_map(|action| {
            let action_key = action.config_key();
            let raw = shortcuts.binding(action_key)?;
            parse_binding_list(raw)
                .is_none()
                .then(|| InvalidShortcutOverride {
                    action_key,
                    raw: raw.to_owned(),
                })
        })
        .collect()
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
        KeyBinding, ShortcutAction, configured_bindings, default_bindings, effective_bindings,
        invalid_shortcut_overrides, key_event_matches_binding, parse_key_binding,
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
        assert_eq!(
            default_bindings(PlatformProfile::Windows, ShortcutAction::MoveTileLeft),
            &[KeyBinding::new(super::CTRL_ALT_SHIFT, egui::Key::ArrowLeft)]
        );
        assert_eq!(
            default_bindings(PlatformProfile::Windows, ShortcutAction::SplitLeft),
            &[KeyBinding::new(super::CTRL_SHIFT, egui::Key::ArrowLeft)]
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
    fn toggle_tab_list_has_default_binding() {
        assert_eq!(
            default_bindings(PlatformProfile::Windows, ShortcutAction::ToggleTabList),
            &[KeyBinding::new(super::CTRL_ALT, egui::Key::B)]
        );
    }

    #[test]
    fn tile_management_defaults_share_the_alt_leader() {
        // `alt` is the in-app stand-in for the compositor's `super`, so every
        // tiling action hangs off it and never collides with editor word-nav.
        assert_eq!(
            default_bindings(PlatformProfile::Hyprland, ShortcutAction::SplitTile),
            &[KeyBinding::new(egui::Modifiers::ALT, egui::Key::Enter)]
        );
        assert_eq!(
            default_bindings(PlatformProfile::Hyprland, ShortcutAction::MoveTileLeft),
            &[KeyBinding::new(egui::Modifiers::ALT, egui::Key::ArrowLeft)]
        );
        assert_eq!(
            default_bindings(PlatformProfile::Hyprland, ShortcutAction::ResizeTileLeft),
            &[KeyBinding::new(super::ALT_SHIFT, egui::Key::ArrowLeft)]
        );
        assert_eq!(
            default_bindings(PlatformProfile::Hyprland, ShortcutAction::SplitLeft),
            &[KeyBinding::new(super::CTRL_ALT, egui::Key::ArrowLeft)]
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

    #[test]
    fn exact_modifiers_keep_overlapping_shortcuts_distinct() {
        assert!(!key_event_matches_binding(
            super::CTRL_SHIFT,
            KeyBinding::new(egui::Modifiers::CTRL, egui::Key::H),
        ));
        assert!(!key_event_matches_binding(
            super::ALT_SHIFT,
            KeyBinding::new(egui::Modifiers::ALT, egui::Key::ArrowLeft),
        ));
        assert!(key_event_matches_binding(
            super::CTRL_SHIFT,
            KeyBinding::new(super::CTRL_SHIFT, egui::Key::H),
        ));
    }

    #[test]
    fn effective_bindings_use_override_or_platform_default() {
        let shortcuts = ShortcutSettings {
            bindings: BTreeMap::from([("split_left".to_owned(), "alt+l".to_owned())]),
        };

        assert_eq!(
            effective_bindings(
                PlatformProfile::Hyprland,
                &shortcuts,
                ShortcutAction::SplitLeft,
            ),
            vec![KeyBinding::new(egui::Modifiers::ALT, egui::Key::L)]
        );
        assert_eq!(
            effective_bindings(
                PlatformProfile::Hyprland,
                &shortcuts,
                ShortcutAction::SplitRight,
            ),
            vec![KeyBinding::new(super::CTRL_ALT, egui::Key::ArrowRight)]
        );
    }

    #[test]
    fn invalid_shortcut_overrides_are_reported_for_known_actions() {
        let shortcuts = ShortcutSettings {
            bindings: BTreeMap::from([
                ("open_file".to_owned(), "ctrl+bogus".to_owned()),
                ("save_file".to_owned(), "ctrl+s".to_owned()),
            ]),
        };

        let invalid = invalid_shortcut_overrides(&shortcuts);

        assert_eq!(invalid.len(), 1);
        assert_eq!(invalid[0].action_key, "open_file");
        assert_eq!(invalid[0].raw, "ctrl+bogus");
    }
}
