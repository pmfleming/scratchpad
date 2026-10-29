use super::*;
use crate::app::{
    platform::PlatformProfile,
    services::settings_store::{SettingsStore, parse_toml_settings},
    shortcut_keymap::configured_bindings,
};

fn uncomment_defaults(raw: &str) -> String {
    raw.lines()
        .map(|line| {
            line.strip_prefix("# ")
                .filter(|line| line.contains(" = \""))
                .unwrap_or(line)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn every_generated_default_is_listed_and_can_be_uncommented_for_each_profile() {
    for profile in [
        PlatformProfile::Auto,
        PlatformProfile::Windows,
        PlatformProfile::LinuxGeneric,
        PlatformProfile::Hyprland,
    ] {
        let root = tempfile::tempdir().unwrap();
        let store = SettingsStore::new(root.path().to_owned());
        let mut settings = AppSettings::default();
        settings.platform.profile = profile;
        store.save(&settings).unwrap();
        let raw = std::fs::read_to_string(store.path()).unwrap();
        assert!(store.load().unwrap().unwrap().shortcuts.bindings.is_empty());
        let uncommented = parse_toml_settings(&uncomment_defaults(&raw)).unwrap();
        assert_eq!(
            uncommented.shortcuts.bindings.len(),
            ShortcutAction::ALL.len()
        );
        for action in ShortcutAction::ALL {
            assert_eq!(
                configured_bindings(&uncommented.shortcuts, action).unwrap(),
                default_bindings(profile, action),
                "{profile:?}: {action:?}"
            );
        }
        assert!(raw.contains("# reset_zoom = \"ctrl+0\""));
        assert!(raw.contains("# toggle_line_numbers = \"ctrl+9\""));
        assert!(raw.contains("# Fixed shortcuts"));
        assert!(raw.contains("# Search ("));
        assert!(raw.contains("# History ("));
    }
}

#[test]
fn saves_preserve_overrides_including_unknown_and_invalid_keys_without_duplicates() {
    let root = tempfile::tempdir().unwrap();
    let store = SettingsStore::new(root.path().to_owned());
    let mut settings = AppSettings::default();
    for (key, value) in [
        ("open_search", "ctrl+p, alt+f"),
        ("reset_zoom", "invalid+binding"),
        ("future_action", "ctrl+shift+f12"),
    ] {
        settings.shortcuts.bindings.insert(key.into(), value.into());
    }
    store.save(&settings).unwrap();
    let first = std::fs::read_to_string(store.path()).unwrap();
    assert!(!first.contains("# open_search ="));
    assert!(!first.contains("# reset_zoom ="));
    let loaded = store.load().unwrap().unwrap();
    assert_eq!(loaded.shortcuts, settings.shortcuts);
    let uncommented = parse_toml_settings(&uncomment_defaults(&first)).unwrap();
    assert_eq!(
        uncommented.shortcuts.bindings.len(),
        ShortcutAction::ALL.len() + 1
    );
    for (key, value) in &settings.shortcuts.bindings {
        assert_eq!(uncommented.shortcuts.binding(key), Some(value.as_str()));
    }
    store.save(&loaded).unwrap();
    assert_eq!(std::fs::read_to_string(store.path()).unwrap(), first);
}

#[test]
fn commented_defaults_do_not_freeze_platform_specific_shortcuts() {
    let root = tempfile::tempdir().unwrap();
    let store = SettingsStore::new(root.path().to_owned());
    let mut settings = AppSettings::default();
    settings.platform.profile = PlatformProfile::Windows;
    store.save(&settings).unwrap();
    let first = std::fs::read_to_string(store.path()).unwrap();
    let mut loaded = store.load().unwrap().unwrap();
    loaded.platform.profile = PlatformProfile::Hyprland;
    store.save(&loaded).unwrap();
    let second = std::fs::read_to_string(store.path()).unwrap();
    assert_ne!(first, second);
    assert!(store.load().unwrap().unwrap().shortcuts.bindings.is_empty());
    let documented = parse_toml_settings(&uncomment_defaults(&second)).unwrap();
    assert_eq!(
        configured_bindings(&documented.shortcuts, ShortcutAction::SplitLeft).unwrap(),
        default_bindings(PlatformProfile::Hyprland, ShortcutAction::SplitLeft)
    );
}
