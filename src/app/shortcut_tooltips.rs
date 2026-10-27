use crate::app::platform::{PlatformProfile, resolved_profile};
use crate::app::services::settings_store::ShortcutSettings;
use crate::app::shortcut_keymap::{
    KeyBinding, ShortcutAction, key_event_matches_binding, resolved_bindings,
};
use eframe::egui;
use std::borrow::Cow;
use std::sync::Arc;

struct RuntimeShortcuts {
    profile: PlatformProfile,
    settings: ShortcutSettings,
    bindings: Vec<(ShortcutAction, Cow<'static, [KeyBinding]>)>,
}

impl RuntimeShortcuts {
    fn new(profile: PlatformProfile, settings: &ShortcutSettings) -> Self {
        Self {
            profile,
            settings: settings.clone(),
            bindings: ShortcutAction::ALL
                .iter()
                .map(|action| (*action, resolved_bindings(profile, settings, *action)))
                .collect(),
        }
    }

    fn matches_source(&self, profile: PlatformProfile, settings: &ShortcutSettings) -> bool {
        self.profile == profile && self.settings == *settings
    }

    fn bindings(&self, action: ShortcutAction) -> &[KeyBinding] {
        self.bindings
            .iter()
            .find_map(|(candidate, bindings)| (*candidate == action).then_some(bindings.as_ref()))
            .unwrap_or_default()
    }
}

impl Default for RuntimeShortcuts {
    fn default() -> Self {
        Self::new(PlatformProfile::Auto, &ShortcutSettings::default())
    }
}

fn runtime_id() -> egui::Id {
    egui::Id::unique("scratchpad.runtime_shortcuts")
}

/// Publishes the current platform and user keymap for UI code and the editor.
/// Rebuilds the cached map only when its source settings change.
pub(crate) fn sync_context(
    ctx: &egui::Context,
    profile: PlatformProfile,
    settings: &ShortcutSettings,
) {
    ctx.data_mut(|data| {
        let current = data.get_temp::<Arc<RuntimeShortcuts>>(runtime_id());
        if current
            .as_deref()
            .is_some_and(|runtime| runtime.matches_source(profile, settings))
        {
            return;
        }
        data.insert_temp(
            runtime_id(),
            Arc::new(RuntimeShortcuts::new(profile, settings)),
        );
    });
}

fn runtime_shortcuts(ctx: &egui::Context) -> Arc<RuntimeShortcuts> {
    ctx.data(|data| data.get_temp(runtime_id()))
        .unwrap_or_else(|| Arc::new(RuntimeShortcuts::default()))
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
    let key = key_name(binding.key);
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
        parts.push(command_name(profile));
    }
    parts.push(key.as_ref());
    parts.join("+")
}

fn command_name(profile: PlatformProfile) -> &'static str {
    match resolved_profile(profile) {
        PlatformProfile::Windows => "WIN",
        PlatformProfile::Auto | PlatformProfile::LinuxGeneric | PlatformProfile::Hyprland => {
            "SUPER"
        }
    }
}

fn key_name(key: egui::Key) -> Cow<'static, str> {
    match key {
        egui::Key::Escape => Cow::Borrowed("ESC"),
        egui::Key::PageUp => Cow::Borrowed("PAGE UP"),
        egui::Key::PageDown => Cow::Borrowed("PAGE DOWN"),
        egui::Key::Minus | egui::Key::Plus | egui::Key::Equals | egui::Key::Comma => {
            Cow::Borrowed(key.symbol_or_name())
        }
        _ => Cow::Owned(key.name().to_ascii_uppercase()),
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
    use super::{action, is_app_shortcut, runtime_shortcuts, sync_context};
    use crate::app::platform::PlatformProfile;
    use crate::app::services::settings_store::ShortcutSettings;
    use crate::app::shortcut_keymap::ShortcutAction;
    use eframe::egui;
    use std::collections::BTreeMap;

    #[test]
    fn tooltips_follow_real_context_defaults_overrides_and_invalid_fallbacks() {
        let ctx = egui::Context::default();
        assert_eq!(
            action(&ctx, ShortcutAction::OpenFile, "Open"),
            "CTRL+O: Open"
        );
        for (profile, shortcut, raw, expected) in [
            (
                PlatformProfile::Hyprland,
                ShortcutAction::SplitLeft,
                None,
                "CTRL+ALT+LEFT: Action",
            ),
            (
                PlatformProfile::Windows,
                ShortcutAction::OpenFile,
                Some("ctrl+shift+p, super+o"),
                "CTRL+SHIFT+P / WIN+O: Action",
            ),
            (
                PlatformProfile::LinuxGeneric,
                ShortcutAction::SaveFile,
                Some("ctrl+bogus"),
                "CTRL+S: Action",
            ),
        ] {
            let settings = ShortcutSettings {
                bindings: raw
                    .into_iter()
                    .map(|raw| (shortcut.config_key().to_owned(), raw.to_owned()))
                    .collect(),
            };
            sync_context(&ctx, profile, &settings);
            assert_eq!(action(&ctx, shortcut, "Action"), expected);
        }
    }

    #[test]
    fn runtime_keymap_reserves_platform_and_user_bindings_exactly() {
        let ctx = egui::Context::default();
        let mut settings = ShortcutSettings {
            bindings: BTreeMap::from([("open_file".to_owned(), "ctrl+shift+p".to_owned())]),
        };
        sync_context(&ctx, PlatformProfile::Hyprland, &settings);
        let cached = runtime_shortcuts(&ctx);
        sync_context(&ctx, PlatformProfile::Hyprland, &settings);
        assert!(std::sync::Arc::ptr_eq(&cached, &runtime_shortcuts(&ctx)));

        assert!(is_app_shortcut(
            &ctx,
            egui::Modifiers::CTRL | egui::Modifiers::SHIFT,
            egui::Key::P,
        ));
        assert!(!is_app_shortcut(&ctx, egui::Modifiers::CTRL, egui::Key::P,));
        assert!(is_app_shortcut(
            &ctx,
            egui::Modifiers::ALT,
            egui::Key::ArrowLeft
        ));
        sync_context(&ctx, PlatformProfile::Windows, &settings);
        assert!(!is_app_shortcut(
            &ctx,
            egui::Modifiers::ALT,
            egui::Key::ArrowLeft
        ));
        settings
            .bindings
            .insert("open_file".into(), "ctrl+p".into());
        sync_context(&ctx, PlatformProfile::Windows, &settings);
        assert!(is_app_shortcut(&ctx, egui::Modifiers::CTRL, egui::Key::P));
        assert!(!is_app_shortcut(
            &ctx,
            egui::Modifiers::CTRL | egui::Modifiers::SHIFT,
            egui::Key::P
        ));
    }
}
