use crate::app::platform::{PlatformProfile, resolved_profile};
use crate::app::services::settings_store::ShortcutSettings;
use eframe::egui::{self, Key, Modifiers};
use std::borrow::Cow;

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
        self.definition().config_key
    }

    // One exhaustive dispatch ties each persisted key to both platform defaults.
    const fn definition(self) -> ShortcutDefinition {
        match self {
            Self::OpenUserManual => OPEN_USER_MANUAL,
            Self::OpenSearch => OPEN_SEARCH,
            Self::OpenReplace => OPEN_REPLACE,
            Self::OpenSettings => OPEN_SETTINGS,
            Self::CloseSettings => CLOSE_SETTINGS,
            Self::CloseSearch => CLOSE_SEARCH,
            Self::RenameTab => RENAME_TAB,
            Self::OpenTextHistory => OPEN_TEXT_HISTORY,
            Self::OpenEncodingDialog => OPEN_ENCODING_DIALOG,
            Self::OpenStatusHistory => OPEN_STATUS_HISTORY,
            Self::CopyActivePath => COPY_ACTIVE_PATH,
            Self::RevealActivePath => REVEAL_ACTIVE_PATH,
            Self::ToggleTabList => TOGGLE_TAB_LIST,
            Self::ToggleTabListAutoHide => TOGGLE_TAB_LIST_AUTO_HIDE,
            Self::ToggleReadingOrder => TOGGLE_READING_ORDER,
            Self::ToggleControlChars => TOGGLE_CONTROL_CHARS,
            Self::TraverseRegionForward => TRAVERSE_REGION_FORWARD,
            Self::TraverseRegionBackward => TRAVERSE_REGION_BACKWARD,
            Self::OpenFileHere => OPEN_FILE_HERE,
            Self::NewTab => NEW_TAB,
            Self::OpenFile => OPEN_FILE,
            Self::SaveFileAs => SAVE_FILE_AS,
            Self::SaveFile => SAVE_FILE,
            Self::IncreaseFontSize => INCREASE_FONT_SIZE,
            Self::DecreaseFontSize => DECREASE_FONT_SIZE,
            Self::ToggleLineNumbers => TOGGLE_LINE_NUMBERS,
            Self::CloseTab => CLOSE_TAB,
            Self::PromoteTileToTab => PROMOTE_TILE_TO_TAB,
            Self::PromoteTabFilesToTabs => PROMOTE_TAB_FILES_TO_TABS,
            Self::CloseTile => CLOSE_TILE,
            Self::SplitTile => SPLIT_TILE,
            Self::SplitUp => SPLIT_UP,
            Self::SplitDown => SPLIT_DOWN,
            Self::SplitLeft => SPLIT_LEFT,
            Self::SplitRight => SPLIT_RIGHT,
            Self::ResizeTileLeft => RESIZE_LEFT,
            Self::ResizeTileRight => RESIZE_RIGHT,
            Self::ResizeTileUp => RESIZE_UP,
            Self::ResizeTileDown => RESIZE_DOWN,
            Self::MoveTileLeft => MOVE_LEFT,
            Self::MoveTileRight => MOVE_RIGHT,
            Self::MoveTileUp => MOVE_UP,
            Self::MoveTileDown => MOVE_DOWN,
        }
    }
}

struct ShortcutDefinition {
    config_key: &'static str,
    bindings: &'static [KeyBinding],
    hyprland: &'static [KeyBinding],
}

impl ShortcutDefinition {
    const fn new(config_key: &'static str, bindings: &'static [KeyBinding]) -> Self {
        Self {
            config_key,
            bindings,
            hyprland: bindings,
        }
    }

    const fn hyprland(mut self, bindings: &'static [KeyBinding]) -> Self {
        self.hyprland = bindings;
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct InvalidShortcutOverride {
    pub(crate) action_key: &'static str,
    pub(crate) raw: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyBinding {
    pub modifiers: Modifiers,
    pub key: Key,
}

impl KeyBinding {
    #[must_use]
    pub const fn new(modifiers: Modifiers, key: Key) -> Self {
        Self { modifiers, key }
    }
}

const CTRL_SHIFT: Modifiers = Modifiers {
    ctrl: true,
    shift: true,
    ..Modifiers::NONE
};
const CTRL_ALT: Modifiers = Modifiers {
    ctrl: true,
    alt: true,
    ..Modifiers::NONE
};
const ALT_SHIFT: Modifiers = Modifiers {
    alt: true,
    shift: true,
    ..Modifiers::NONE
};
const CTRL_ALT_SHIFT: Modifiers = Modifiers {
    ctrl: true,
    alt: true,
    shift: true,
    ..Modifiers::NONE
};

const OPEN_USER_MANUAL: ShortcutDefinition = ShortcutDefinition::new(
    "open_user_manual",
    &[KeyBinding::new(Modifiers::NONE, Key::F1)],
);
const OPEN_SEARCH: ShortcutDefinition =
    ShortcutDefinition::new("open_search", &[KeyBinding::new(Modifiers::CTRL, Key::F)]);
const OPEN_REPLACE: ShortcutDefinition =
    ShortcutDefinition::new("open_replace", &[KeyBinding::new(Modifiers::CTRL, Key::H)]);
const OPEN_SETTINGS: ShortcutDefinition = ShortcutDefinition::new(
    "open_settings",
    &[KeyBinding::new(Modifiers::CTRL, Key::Comma)],
);
const CLOSE_SETTINGS: ShortcutDefinition = ShortcutDefinition::new(
    "close_settings",
    &[KeyBinding::new(Modifiers::NONE, Key::Escape)],
);
const CLOSE_SEARCH: ShortcutDefinition = ShortcutDefinition::new(
    "close_search",
    &[KeyBinding::new(Modifiers::NONE, Key::Escape)],
);
const RENAME_TAB: ShortcutDefinition =
    ShortcutDefinition::new("rename_tab", &[KeyBinding::new(Modifiers::NONE, Key::F2)]);
const OPEN_TEXT_HISTORY: ShortcutDefinition =
    ShortcutDefinition::new("open_text_history", &[KeyBinding::new(CTRL_SHIFT, Key::H)]);
const OPEN_ENCODING_DIALOG: ShortcutDefinition = ShortcutDefinition::new(
    "open_encoding_dialog",
    &[KeyBinding::new(CTRL_SHIFT, Key::E)],
);
const OPEN_STATUS_HISTORY: ShortcutDefinition = ShortcutDefinition::new(
    "open_status_history",
    &[KeyBinding::new(CTRL_SHIFT, Key::M)],
);
const COPY_ACTIVE_PATH: ShortcutDefinition =
    ShortcutDefinition::new("copy_active_path", &[KeyBinding::new(CTRL_SHIFT, Key::C)]);
const REVEAL_ACTIVE_PATH: ShortcutDefinition =
    ShortcutDefinition::new("reveal_active_path", &[KeyBinding::new(CTRL_SHIFT, Key::R)]);
const TOGGLE_TAB_LIST: ShortcutDefinition =
    ShortcutDefinition::new("toggle_tab_list", &[KeyBinding::new(CTRL_ALT, Key::B)]);
const TOGGLE_TAB_LIST_AUTO_HIDE: ShortcutDefinition = ShortcutDefinition::new(
    "toggle_tab_list_auto_hide",
    &[KeyBinding::new(CTRL_SHIFT, Key::B)],
);
const TOGGLE_READING_ORDER: ShortcutDefinition =
    ShortcutDefinition::new("toggle_reading_order", &[KeyBinding::new(CTRL_ALT, Key::R)]);
const TOGGLE_CONTROL_CHARS: ShortcutDefinition =
    ShortcutDefinition::new("toggle_control_chars", &[KeyBinding::new(CTRL_ALT, Key::C)]);
const TRAVERSE_REGION_FORWARD: ShortcutDefinition = ShortcutDefinition::new(
    "traverse_region_forward",
    &[KeyBinding::new(Modifiers::NONE, Key::F6)],
);
const TRAVERSE_REGION_BACKWARD: ShortcutDefinition = ShortcutDefinition::new(
    "traverse_region_backward",
    &[KeyBinding::new(Modifiers::SHIFT, Key::F6)],
);
const OPEN_FILE_HERE: ShortcutDefinition =
    ShortcutDefinition::new("open_file_here", &[KeyBinding::new(CTRL_SHIFT, Key::O)]);
const NEW_TAB: ShortcutDefinition =
    ShortcutDefinition::new("new_tab", &[KeyBinding::new(Modifiers::CTRL, Key::N)]);
const OPEN_FILE: ShortcutDefinition =
    ShortcutDefinition::new("open_file", &[KeyBinding::new(Modifiers::CTRL, Key::O)]);
const SAVE_FILE_AS: ShortcutDefinition =
    ShortcutDefinition::new("save_file_as", &[KeyBinding::new(CTRL_SHIFT, Key::S)]);
const SAVE_FILE: ShortcutDefinition =
    ShortcutDefinition::new("save_file", &[KeyBinding::new(Modifiers::CTRL, Key::S)]);
const INCREASE_FONT_SIZE: ShortcutDefinition = ShortcutDefinition::new(
    "increase_font_size",
    &[
        KeyBinding::new(Modifiers::CTRL, Key::Equals),
        KeyBinding::new(Modifiers::CTRL, Key::Plus),
    ],
);
const DECREASE_FONT_SIZE: ShortcutDefinition = ShortcutDefinition::new(
    "decrease_font_size",
    &[KeyBinding::new(Modifiers::CTRL, Key::Minus)],
);
const TOGGLE_LINE_NUMBERS: ShortcutDefinition = ShortcutDefinition::new(
    "toggle_line_numbers",
    &[KeyBinding::new(Modifiers::CTRL, Key::Num0)],
);
const CLOSE_TAB: ShortcutDefinition =
    ShortcutDefinition::new("close_tab", &[KeyBinding::new(Modifiers::CTRL, Key::W)]);
const PROMOTE_TILE_TO_TAB: ShortcutDefinition = ShortcutDefinition::new(
    "promote_tile_to_tab",
    &[KeyBinding::new(Modifiers::CTRL, Key::T)],
);
const PROMOTE_TAB_FILES_TO_TABS: ShortcutDefinition = ShortcutDefinition::new(
    "promote_tab_files_to_tabs",
    &[KeyBinding::new(CTRL_SHIFT, Key::T)],
);
const CLOSE_TILE: ShortcutDefinition =
    ShortcutDefinition::new("close_tile", &[KeyBinding::new(CTRL_SHIFT, Key::W)]);
const SPLIT_TILE: ShortcutDefinition =
    ShortcutDefinition::new("split_tile", &[KeyBinding::new(CTRL_ALT, Key::Enter)])
        .hyprland(&[KeyBinding::new(Modifiers::ALT, Key::Enter)]);
const SPLIT_UP: ShortcutDefinition =
    ShortcutDefinition::new("split_up", &[KeyBinding::new(CTRL_SHIFT, Key::ArrowUp)])
        .hyprland(&[KeyBinding::new(CTRL_ALT, Key::ArrowUp)]);
const SPLIT_DOWN: ShortcutDefinition =
    ShortcutDefinition::new("split_down", &[KeyBinding::new(CTRL_SHIFT, Key::ArrowDown)])
        .hyprland(&[KeyBinding::new(CTRL_ALT, Key::ArrowDown)]);
const SPLIT_LEFT: ShortcutDefinition =
    ShortcutDefinition::new("split_left", &[KeyBinding::new(CTRL_SHIFT, Key::ArrowLeft)])
        .hyprland(&[KeyBinding::new(CTRL_ALT, Key::ArrowLeft)]);
const SPLIT_RIGHT: ShortcutDefinition = ShortcutDefinition::new(
    "split_right",
    &[KeyBinding::new(CTRL_SHIFT, Key::ArrowRight)],
)
.hyprland(&[KeyBinding::new(CTRL_ALT, Key::ArrowRight)]);
const RESIZE_LEFT: ShortcutDefinition = ShortcutDefinition::new(
    "resize_tile_left",
    &[KeyBinding::new(CTRL_ALT, Key::ArrowLeft)],
)
.hyprland(&[KeyBinding::new(ALT_SHIFT, Key::ArrowLeft)]);
const RESIZE_RIGHT: ShortcutDefinition = ShortcutDefinition::new(
    "resize_tile_right",
    &[KeyBinding::new(CTRL_ALT, Key::ArrowRight)],
)
.hyprland(&[KeyBinding::new(ALT_SHIFT, Key::ArrowRight)]);
const RESIZE_UP: ShortcutDefinition =
    ShortcutDefinition::new("resize_tile_up", &[KeyBinding::new(CTRL_ALT, Key::ArrowUp)])
        .hyprland(&[KeyBinding::new(ALT_SHIFT, Key::ArrowUp)]);
const RESIZE_DOWN: ShortcutDefinition = ShortcutDefinition::new(
    "resize_tile_down",
    &[KeyBinding::new(CTRL_ALT, Key::ArrowDown)],
)
.hyprland(&[KeyBinding::new(ALT_SHIFT, Key::ArrowDown)]);
// Generic tile movement must not consume Ctrl/Alt+Arrow word navigation.
const MOVE_LEFT: ShortcutDefinition = ShortcutDefinition::new(
    "move_tile_left",
    &[KeyBinding::new(CTRL_ALT_SHIFT, Key::ArrowLeft)],
)
.hyprland(&[KeyBinding::new(Modifiers::ALT, Key::ArrowLeft)]);
const MOVE_RIGHT: ShortcutDefinition = ShortcutDefinition::new(
    "move_tile_right",
    &[KeyBinding::new(CTRL_ALT_SHIFT, Key::ArrowRight)],
)
.hyprland(&[KeyBinding::new(Modifiers::ALT, Key::ArrowRight)]);
const MOVE_UP: ShortcutDefinition = ShortcutDefinition::new(
    "move_tile_up",
    &[KeyBinding::new(CTRL_ALT_SHIFT, Key::ArrowUp)],
)
.hyprland(&[KeyBinding::new(Modifiers::ALT, Key::ArrowUp)]);
const MOVE_DOWN: ShortcutDefinition = ShortcutDefinition::new(
    "move_tile_down",
    &[KeyBinding::new(CTRL_ALT_SHIFT, Key::ArrowDown)],
)
.hyprland(&[KeyBinding::new(Modifiers::ALT, Key::ArrowDown)]);

#[must_use]
pub fn default_bindings(profile: PlatformProfile, action: ShortcutAction) -> &'static [KeyBinding] {
    let definition = action.definition();
    if resolved_profile(profile) == PlatformProfile::Hyprland {
        definition.hyprland
    } else {
        definition.bindings
    }
}

#[must_use]
pub fn consume_shortcut(
    ctx: &egui::Context,
    profile: PlatformProfile,
    shortcuts: &ShortcutSettings,
    action: ShortcutAction,
) -> bool {
    resolved_bindings(profile, shortcuts, action)
        .iter()
        .any(|binding| consume_binding(ctx, *binding))
}

fn consume_binding(ctx: &egui::Context, binding: KeyBinding) -> bool {
    let pressed = ctx.input(|input| {
        input.events.iter().any(|event| {
            matches!(event, egui::Event::Key { key, pressed: true, modifiers, .. }
            if *key == binding.key && key_event_matches_binding(*modifiers, binding))
        })
    });
    pressed && ctx.input_mut(|input| input.consume_key(binding.modifiers, binding.key))
}

/// Match exact modifiers, except plus/equals where producing the character can
/// require Shift. This keeps Ctrl+Shift+H distinct from Ctrl+H.
#[must_use]
pub(crate) fn key_event_matches_binding(pressed: Modifiers, binding: KeyBinding) -> bool {
    if matches!(binding.key, Key::Plus | Key::Equals) {
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

/// Resolve overrides without copying the static default bindings.
pub(crate) fn resolved_bindings(
    profile: PlatformProfile,
    shortcuts: &ShortcutSettings,
    action: ShortcutAction,
) -> Cow<'static, [KeyBinding]> {
    configured_bindings(shortcuts, action).map_or_else(
        || Cow::Borrowed(default_bindings(profile, action)),
        Cow::Owned,
    )
}

/// Returns an owned copy of the effective bindings for callers that need one.
#[must_use]
pub fn effective_bindings(
    profile: PlatformProfile,
    shortcuts: &ShortcutSettings,
    action: ShortcutAction,
) -> Vec<KeyBinding> {
    resolved_bindings(profile, shortcuts, action).into_owned()
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
    // str::split always yields at least one item; an empty item fails parsing.
    raw.split(',').map(parse_key_binding).collect()
}

fn parse_key_binding(raw: &str) -> Option<KeyBinding> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let mut modifiers = Modifiers::NONE;
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
    let token = token.trim();
    if token == "-" {
        return token.to_owned();
    }
    token
        .chars()
        .filter(|ch| !ch.is_whitespace() && *ch != '_' && *ch != '-')
        .collect::<String>()
        .to_ascii_lowercase()
}

fn parse_key_token(token: &str) -> Option<Key> {
    if let Some(digit) = token.strip_prefix("num")
        && digit.len() == 1
        && digit.as_bytes()[0].is_ascii_digit()
    {
        return Key::from_name(digit);
    }
    let canonical = match token {
        "up" | "arrowup" => "ArrowUp",
        "down" | "arrowdown" => "ArrowDown",
        "left" | "arrowleft" => "ArrowLeft",
        "right" | "arrowright" => "ArrowRight",
        "esc" => "Escape",
        "return" => "Enter",
        "del" => "Delete",
        "equal" => "Equals",
        "dash" => "Minus",
        _ => token,
    };
    Key::from_name(canonical).or_else(|| {
        Key::ALL
            .iter()
            .copied()
            .find(|key| key.name().eq_ignore_ascii_case(canonical))
    })
}

#[cfg(test)]
mod tests;
