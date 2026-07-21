use crate::app::platform::{PlatformProfile, resolved_profile};
use crate::app::services::settings_store::ShortcutSettings;
use crate::app::shortcut_keymap::{
    KeyBinding, ShortcutAction, effective_bindings, key_event_matches_binding,
};
use eframe::egui;

#[derive(Clone)]
struct RuntimeShortcuts {
    profile: PlatformProfile,
    bindings: Vec<(ShortcutAction, Vec<KeyBinding>)>,
}

impl RuntimeShortcuts {
    fn new(profile: PlatformProfile, settings: &ShortcutSettings) -> Self {
        Self {
            profile,
            bindings: ShortcutAction::ALL
                .iter()
                .map(|action| (*action, effective_bindings(profile, settings, *action)))
                .collect(),
        }
    }

    fn bindings(&self, action: ShortcutAction) -> &[KeyBinding] {
        self.bindings
            .iter()
            .find_map(|(candidate, bindings)| (*candidate == action).then_some(bindings.as_slice()))
            .unwrap_or_default()
    }
}

impl Default for RuntimeShortcuts {
    fn default() -> Self {
        Self::new(PlatformProfile::Auto, &ShortcutSettings::default())
    }
}

fn runtime_id() -> egui::Id {
    egui::Id::new("scratchpad.runtime_shortcuts")
}

/// Publishes the current platform and user keymap for UI code and the editor.
/// This is refreshed before controls are rendered each frame.
pub(crate) fn sync_context(
    ctx: &egui::Context,
    profile: PlatformProfile,
    settings: &ShortcutSettings,
) {
    ctx.data_mut(|data| {
        data.insert_temp(runtime_id(), RuntimeShortcuts::new(profile, settings));
    });
}

fn runtime_shortcuts(ctx: &egui::Context) -> RuntimeShortcuts {
    ctx.data(|data| data.get_temp(runtime_id()))
        .unwrap_or_default()
}

/// Builds a tooltip from the binding that is actually active, including valid
/// user overrides and platform-specific defaults.
pub(crate) fn action(
    ctx: &egui::Context,
    shortcut_action: ShortcutAction,
    description: &str,
) -> String {
    let runtime = runtime_shortcuts(ctx);
    format!(
        "{}: {description}",
        format_bindings(runtime.profile, runtime.bindings(shortcut_action))
    )
}

#[cfg(test)]
fn action_for(
    profile: PlatformProfile,
    settings: &ShortcutSettings,
    shortcut_action: ShortcutAction,
    description: &str,
) -> String {
    let bindings = effective_bindings(profile, settings, shortcut_action);
    format!("{}: {description}", format_bindings(profile, &bindings))
}

/// Returns true when an editor key event belongs to the active app keymap.
/// Reserving exact effective bindings prevents editor navigation/editing from
/// also firing when a platform default or user override invokes an app action.
pub(crate) fn is_app_shortcut(
    ctx: &egui::Context,
    modifiers: egui::Modifiers,
    key: egui::Key,
) -> bool {
    let runtime = runtime_shortcuts(ctx);
    runtime.bindings.iter().any(|(_, bindings)| {
        bindings
            .iter()
            .any(|binding| binding.key == key && key_event_matches_binding(modifiers, *binding))
    })
}

#[must_use]
pub(crate) fn format_bindings(profile: PlatformProfile, bindings: &[KeyBinding]) -> String {
    bindings
        .iter()
        .map(|binding| format_binding(profile, *binding))
        .collect::<Vec<_>>()
        .join(" / ")
}

fn format_binding(profile: PlatformProfile, binding: KeyBinding) -> String {
    let mut parts = Vec::with_capacity(5);
    if binding.modifiers.ctrl {
        parts.push("CTRL");
    }
    if binding.modifiers.alt {
        parts.push("ALT");
    }
    if binding.modifiers.shift {
        parts.push("SHIFT");
    }
    if binding.modifiers.mac_cmd {
        parts.push("CMD");
    } else if binding.modifiers.command {
        parts.push(match resolved_profile(profile) {
            PlatformProfile::Windows => "WIN",
            PlatformProfile::Auto | PlatformProfile::LinuxGeneric | PlatformProfile::Hyprland => {
                "SUPER"
            }
        });
    }
    parts.push(key_name(binding.key));
    parts.join("+")
}

fn key_name(key: egui::Key) -> &'static str {
    match key {
        egui::Key::ArrowDown => "DOWN",
        egui::Key::ArrowLeft => "LEFT",
        egui::Key::ArrowRight => "RIGHT",
        egui::Key::ArrowUp => "UP",
        egui::Key::Escape => "ESC",
        egui::Key::Tab => "TAB",
        egui::Key::Backspace => "BACKSPACE",
        egui::Key::Enter => "ENTER",
        egui::Key::Space => "SPACE",
        egui::Key::Insert => "INSERT",
        egui::Key::Delete => "DELETE",
        egui::Key::Home => "HOME",
        egui::Key::End => "END",
        egui::Key::PageUp => "PAGE UP",
        egui::Key::PageDown => "PAGE DOWN",
        egui::Key::Minus => "-",
        egui::Key::Plus => "+",
        egui::Key::Equals => "=",
        egui::Key::Comma => ",",
        egui::Key::Num0 => "0",
        egui::Key::Num1 => "1",
        egui::Key::Num2 => "2",
        egui::Key::Num3 => "3",
        egui::Key::Num4 => "4",
        egui::Key::Num5 => "5",
        egui::Key::Num6 => "6",
        egui::Key::Num7 => "7",
        egui::Key::Num8 => "8",
        egui::Key::Num9 => "9",
        egui::Key::A => "A",
        egui::Key::B => "B",
        egui::Key::C => "C",
        egui::Key::D => "D",
        egui::Key::E => "E",
        egui::Key::F => "F",
        egui::Key::G => "G",
        egui::Key::H => "H",
        egui::Key::I => "I",
        egui::Key::J => "J",
        egui::Key::K => "K",
        egui::Key::L => "L",
        egui::Key::M => "M",
        egui::Key::N => "N",
        egui::Key::O => "O",
        egui::Key::P => "P",
        egui::Key::Q => "Q",
        egui::Key::R => "R",
        egui::Key::S => "S",
        egui::Key::T => "T",
        egui::Key::U => "U",
        egui::Key::V => "V",
        egui::Key::W => "W",
        egui::Key::X => "X",
        egui::Key::Y => "Y",
        egui::Key::Z => "Z",
        egui::Key::F1 => "F1",
        egui::Key::F2 => "F2",
        egui::Key::F3 => "F3",
        egui::Key::F4 => "F4",
        egui::Key::F5 => "F5",
        egui::Key::F6 => "F6",
        egui::Key::F7 => "F7",
        egui::Key::F8 => "F8",
        egui::Key::F9 => "F9",
        egui::Key::F10 => "F10",
        egui::Key::F11 => "F11",
        egui::Key::F12 => "F12",
        _ => "KEY",
    }
}

// Fixed editor and search-strip shortcuts. These are not user-configurable.
pub(crate) const COPY: &str = "CTRL+C: Copy";
pub(crate) const CUT: &str = "CTRL+X: Cut";
pub(crate) const PASTE: &str = "CTRL+V: Paste";
pub(crate) const REDO: &str = "CTRL+Y: Redo";
pub(crate) const REPLACE_ALL_MATCHES: &str = "ALT+ENTER: Replace all matches";
pub(crate) const REPLACE_CURRENT_MATCH: &str = "CTRL+ENTER: Replace current match";
pub(crate) const SEARCH_MATCH_CASE: &str = "ALT+C: Case Sensitive";
pub(crate) const SEARCH_MODE_REGEX: &str = "ALT+R: Regex";
pub(crate) const SEARCH_NEXT_MATCH: &str = "F3: Next Match";
pub(crate) const SEARCH_PREVIOUS_MATCH: &str = "SHIFT+F3: Previous Match";
pub(crate) const SEARCH_SCOPE_ALL_TABS: &str = "ALT+4: Search All Open Files";
pub(crate) const SEARCH_SCOPE_CURRENT_FILE: &str = "ALT+2: Search Current File";
pub(crate) const SEARCH_SCOPE_CURRENT_TAB: &str = "ALT+3: Search All Files on This Tab";
pub(crate) const SEARCH_SCOPE_SELECTION: &str = "ALT+1: Search Selected Text";
pub(crate) const SEARCH_SCOPE_SELECTION_DEFAULT: &str =
    "ALT+1: Search Selected Text (auto-selected)";
pub(crate) const SEARCH_WHOLE_WORD: &str = "ALT+W: Whole Word";
pub(crate) const SELECT_ALL: &str = "CTRL+A: Select All";
pub(crate) const UNDO: &str = "CTRL+Z: Undo";

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn tooltip_uses_platform_default() {
        assert_eq!(
            action_for(
                PlatformProfile::Hyprland,
                &ShortcutSettings::default(),
                ShortcutAction::SplitLeft,
                "Split Left",
            ),
            "CTRL+ALT+LEFT: Split Left"
        );
    }

    #[test]
    fn tooltip_uses_user_override_and_multiple_bindings() {
        let settings = ShortcutSettings {
            bindings: BTreeMap::from([(
                "open_file".to_owned(),
                "ctrl+shift+p, super+o".to_owned(),
            )]),
        };
        assert_eq!(
            action_for(
                PlatformProfile::Windows,
                &settings,
                ShortcutAction::OpenFile,
                "Open File",
            ),
            "CTRL+SHIFT+P / WIN+O: Open File"
        );
    }

    #[test]
    fn runtime_keymap_reserves_platform_and_user_bindings_exactly() {
        let ctx = egui::Context::default();
        let settings = ShortcutSettings {
            bindings: BTreeMap::from([("open_file".to_owned(), "ctrl+shift+p".to_owned())]),
        };
        sync_context(&ctx, PlatformProfile::Hyprland, &settings);

        assert!(is_app_shortcut(
            &ctx,
            egui::Modifiers::CTRL | egui::Modifiers::SHIFT,
            egui::Key::P,
        ));
        assert!(!is_app_shortcut(&ctx, egui::Modifiers::CTRL, egui::Key::P,));
        assert!(is_app_shortcut(
            &ctx,
            egui::Modifiers::ALT,
            egui::Key::ArrowLeft,
        ));
    }

    #[test]
    fn invalid_override_tooltip_uses_default() {
        let settings = ShortcutSettings {
            bindings: BTreeMap::from([("save_file".to_owned(), "ctrl+bogus".to_owned())]),
        };
        assert_eq!(
            action_for(
                PlatformProfile::LinuxGeneric,
                &settings,
                ShortcutAction::SaveFile,
                "Save",
            ),
            "CTRL+S: Save"
        );
    }
}
