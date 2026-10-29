use super::AppSettings;
use crate::app::{
    platform::resolved_profile,
    shortcut_keymap::{KeyBinding, ShortcutAction, default_bindings},
};

/// AppSettings serializes [shortcuts] last. Keep defaults as comments in that
/// table: writing them as overrides would freeze Auto/Hyprland resolution and
/// prevent future default shortcut changes from taking effect.
pub(super) fn append(toml: &mut String, settings: &AppSettings) {
    let profile = resolved_profile(settings.platform.profile);
    toml.push_str(&format!(
        "\n# Keyboard shortcuts — defaults for {profile:?}.\n\
         # Uncomment a line to customize it; separate alternatives with commas.\n\
         # Existing overrides are listed above and are not repeated below.\n\
         # Commented defaults follow the selected platform profile.\n\
         # increase_font_size / decrease_font_size control per-pane zoom.\n"
    ));
    for action in ShortcutAction::ALL {
        if settings.shortcuts.binding(action.config_key()).is_some() {
            continue;
        }
        let bindings = default_bindings(profile, action)
            .iter()
            .map(binding_text)
            .collect::<Vec<_>>()
            .join(", ");
        toml.push_str(&format!("# {} = \"{bindings}\"\n", action.config_key()));
    }
    toml.push_str(include_str!("shortcut_reference.txt"));
}

fn binding_text(binding: &KeyBinding) -> String {
    let mut parts = Vec::new();
    for (enabled, label) in [
        (binding.modifiers.ctrl, "ctrl"),
        (binding.modifiers.alt, "alt"),
        (binding.modifiers.shift, "shift"),
        (binding.modifiers.command, "command"),
    ] {
        if enabled {
            parts.push(label.to_owned());
        }
    }
    parts.push(binding.key.name().to_ascii_lowercase());
    parts.join("+")
}

#[cfg(test)]
mod tests;
