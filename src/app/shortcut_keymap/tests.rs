use super::{
    ALT_SHIFT, CTRL_SHIFT, Key, KeyBinding, Modifiers, ShortcutAction, configured_bindings,
    consume_shortcut, default_bindings, effective_bindings, invalid_shortcut_overrides,
    key_event_matches_binding, parse_binding_list, parse_key_binding, resolved_bindings,
};
use crate::app::platform::PlatformProfile;
use crate::app::services::settings_store::ShortcutSettings;
use eframe::egui;
use std::borrow::Cow;
use std::collections::{BTreeMap, HashSet};

// Compatibility oracle captured from the pre-refactor keymap. Columns are the
// persisted key, Windows/Linux default, and Hyprland default ("=" means shared).
const DEFAULTS: &str = "open_user_manual f1 =
open_search ctrl+f =
open_replace ctrl+h =
open_settings ctrl+comma =
close_settings escape =
close_search escape =
rename_tab f2 =
open_text_history ctrl+shift+h =
open_encoding_dialog ctrl+shift+e =
open_status_history ctrl+shift+m =
copy_active_path ctrl+shift+c =
reveal_active_path ctrl+shift+r =
toggle_tab_list ctrl+alt+b =
toggle_tab_list_auto_hide ctrl+shift+b =
toggle_reading_order ctrl+alt+r =
toggle_control_chars ctrl+alt+c =
traverse_region_forward f6 =
traverse_region_backward shift+f6 =
open_file_here ctrl+shift+o =
new_tab ctrl+n =
open_file ctrl+o =
save_file_as ctrl+shift+s =
save_file ctrl+s =
increase_font_size ctrl+equals,ctrl+plus =
decrease_font_size ctrl+minus =
toggle_line_numbers ctrl+0 =
close_tab ctrl+w =
promote_tile_to_tab ctrl+t =
promote_tab_files_to_tabs ctrl+shift+t =
close_tile ctrl+shift+w =
split_tile ctrl+alt+enter alt+enter
split_up ctrl+shift+up ctrl+alt+up
split_down ctrl+shift+down ctrl+alt+down
split_left ctrl+shift+left ctrl+alt+left
split_right ctrl+shift+right ctrl+alt+right
resize_tile_left ctrl+alt+left alt+shift+left
resize_tile_right ctrl+alt+right alt+shift+right
resize_tile_up ctrl+alt+up alt+shift+up
resize_tile_down ctrl+alt+down alt+shift+down
move_tile_left ctrl+alt+shift+left alt+left
move_tile_right ctrl+alt+shift+right alt+right
move_tile_up ctrl+alt+shift+up alt+up
move_tile_down ctrl+alt+shift+down alt+down";

#[test]
fn every_action_keeps_its_persisted_key_and_platform_defaults() {
    assert_eq!(DEFAULTS.lines().count(), ShortcutAction::ALL.len());
    let mut keys = HashSet::new();
    for (action, line) in ShortcutAction::ALL.into_iter().zip(DEFAULTS.lines()) {
        let columns = line.split_whitespace().collect::<Vec<_>>();
        assert_eq!(action.config_key(), columns[0], "{action:?}");
        assert!(keys.insert(action.config_key()));
        for (profile, raw) in [
            (PlatformProfile::Windows, columns[1]),
            (PlatformProfile::LinuxGeneric, columns[1]),
            (
                PlatformProfile::Hyprland,
                if columns[2] == "=" {
                    columns[1]
                } else {
                    columns[2]
                },
            ),
        ] {
            assert_eq!(
                default_bindings(profile, action),
                parse_binding_list(raw).unwrap(),
                "{action:?} {profile:?}"
            );
        }
    }
}

#[test]
fn parser_supports_aliases_and_rejects_incomplete_or_ambiguous_lists() {
    for (raw, modifiers, key) in [
        ("ctrl + shift + left", CTRL_SHIFT, Key::ArrowLeft),
        ("CONTROL+f12", Modifiers::CTRL, Key::F12),
        ("option+arrow_left", Modifiers::ALT, Key::ArrowLeft),
        ("super+return", Modifiers::COMMAND, Key::Enter),
        ("f12", Modifiers::NONE, Key::F12),
        ("arrow-left", Modifiers::NONE, Key::ArrowLeft),
        ("esc", Modifiers::NONE, Key::Escape),
        ("return", Modifiers::NONE, Key::Enter),
        ("num7", Modifiers::NONE, Key::Num7),
        ("-", Modifiers::NONE, Key::Minus),
        ("dash", Modifiers::NONE, Key::Minus),
    ] {
        assert_eq!(
            parse_key_binding(raw),
            Some(KeyBinding::new(modifiers, key)),
            "{raw}"
        );
    }
    for raw in [
        "",
        " ",
        "ctrl",
        "ctrl+bogus",
        "ctrl+a+b",
        "ctrl+",
        "ctrl++",
        "num12",
        "ctrl+a,",
        ",ctrl+a",
        "ctrl+a,,alt+b",
    ] {
        assert_eq!(parse_binding_list(raw), None, "{raw}");
    }
}

#[test]
fn overrides_fall_back_atomically_and_report_only_known_invalid_actions() {
    let profile = PlatformProfile::Hyprland;
    for (raw, expected) in [
        ("alt+l", Some("alt+l")),
        ("ctrl+p,alt+l", Some("ctrl+p,alt+l")),
        ("ctrl+bogus", None),
        ("ctrl+p,", None),
        ("", None),
    ] {
        let shortcuts = ShortcutSettings {
            bindings: BTreeMap::from([
                ("split_left".into(), raw.into()),
                ("unknown_action".into(), "bogus".into()),
            ]),
        };
        let parsed = expected.and_then(parse_binding_list);
        assert_eq!(
            configured_bindings(&shortcuts, ShortcutAction::SplitLeft),
            parsed
        );
        let resolved = resolved_bindings(profile, &shortcuts, ShortcutAction::SplitLeft);
        assert_eq!(matches!(resolved, Cow::Owned(_)), expected.is_some());
        assert_eq!(
            resolved.as_ref(),
            parsed
                .as_deref()
                .unwrap_or(default_bindings(profile, ShortcutAction::SplitLeft))
        );
        assert_eq!(
            effective_bindings(profile, &shortcuts, ShortcutAction::SplitLeft),
            resolved.as_ref()
        );
        let invalid = invalid_shortcut_overrides(&shortcuts);
        assert_eq!(invalid.len(), usize::from(expected.is_none()));
        for entry in invalid {
            assert_eq!(entry.action_key, "split_left");
            assert_eq!(entry.raw, raw);
        }
    }
}

#[test]
fn exact_modifiers_keep_overlapping_shortcuts_distinct() {
    for (pressed, required, key, matches) in [
        (CTRL_SHIFT, Modifiers::CTRL, Key::H, false),
        (ALT_SHIFT, Modifiers::ALT, Key::ArrowLeft, false),
        (CTRL_SHIFT, CTRL_SHIFT, Key::H, true),
        (CTRL_SHIFT, Modifiers::CTRL, Key::Plus, true),
        (CTRL_SHIFT, Modifiers::CTRL, Key::Equals, true),
    ] {
        assert_eq!(
            key_event_matches_binding(pressed, KeyBinding::new(required, key)),
            matches
        );
    }
}

#[test]
fn shortcut_consumption_leaves_nonmatches_and_consumes_matches_once() {
    let ctx = egui::Context::default();
    let settings = ShortcutSettings::default();
    let mut output = ctx.run_ui(
        egui::RawInput {
            events: vec![egui::Event::Key {
                key: Key::H,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: CTRL_SHIFT,
            }],
            ..Default::default()
        },
        |ui| {
            let ctx = ui.ctx();
            assert!(!consume_shortcut(
                ctx,
                PlatformProfile::Windows,
                &settings,
                ShortcutAction::OpenReplace
            ));
            assert!(consume_shortcut(
                ctx,
                PlatformProfile::Windows,
                &settings,
                ShortcutAction::OpenTextHistory
            ));
            assert!(!consume_shortcut(
                ctx,
                PlatformProfile::Windows,
                &settings,
                ShortcutAction::OpenTextHistory
            ));
        },
    );
    output.textures_delta.clear();
}
