use serde::{Deserialize, Serialize};
use std::ffi::OsString;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlatformProfile {
    #[default]
    Auto,
    Windows,
    LinuxGeneric,
    Hyprland,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlatformCapabilities {
    pub show_window_caption_buttons: bool,
    pub use_native_decorations: bool,
    pub allow_app_resize_grips: bool,
    pub allow_app_drag_regions: bool,
    pub global_shortcuts_managed_externally: bool,
}

#[must_use]
pub fn resolved_profile(configured: PlatformProfile) -> PlatformProfile {
    if configured != PlatformProfile::Auto {
        return configured;
    }

    detect_profile_from_current_env()
}

#[must_use]
pub fn capabilities(configured: PlatformProfile) -> PlatformCapabilities {
    capabilities_for_resolved_profile(resolved_profile(configured))
}

#[must_use]
pub fn capabilities_for_resolved_profile(profile: PlatformProfile) -> PlatformCapabilities {
    match profile {
        PlatformProfile::Windows => PlatformCapabilities {
            show_window_caption_buttons: true,
            use_native_decorations: false,
            allow_app_resize_grips: true,
            allow_app_drag_regions: true,
            global_shortcuts_managed_externally: false,
        },
        PlatformProfile::LinuxGeneric => PlatformCapabilities {
            show_window_caption_buttons: true,
            use_native_decorations: true,
            allow_app_resize_grips: true,
            allow_app_drag_regions: true,
            global_shortcuts_managed_externally: false,
        },
        PlatformProfile::Hyprland => PlatformCapabilities {
            show_window_caption_buttons: false,
            use_native_decorations: false,
            allow_app_resize_grips: false,
            allow_app_drag_regions: false,
            global_shortcuts_managed_externally: true,
        },
        PlatformProfile::Auto => {
            capabilities_for_resolved_profile(detect_profile_from_current_env())
        }
    }
}

fn detect_profile_from_current_env() -> PlatformProfile {
    detect_profile_from_env(|name| std::env::var_os(name))
}

fn detect_profile_from_env(mut env: impl FnMut(&str) -> Option<OsString>) -> PlatformProfile {
    native_default_profile(&mut env)
}

#[cfg(target_os = "windows")]
fn native_default_profile(_env: &mut impl FnMut(&str) -> Option<OsString>) -> PlatformProfile {
    PlatformProfile::Windows
}

#[cfg(target_os = "linux")]
fn native_default_profile(env: &mut impl FnMut(&str) -> Option<OsString>) -> PlatformProfile {
    if is_hyprland_env(env) {
        PlatformProfile::Hyprland
    } else {
        PlatformProfile::LinuxGeneric
    }
}

#[cfg(not(any(target_os = "windows", target_os = "linux")))]
fn native_default_profile(_env: &mut impl FnMut(&str) -> Option<OsString>) -> PlatformProfile {
    PlatformProfile::LinuxGeneric
}

#[cfg(target_os = "linux")]
fn is_hyprland_env(env: &mut impl FnMut(&str) -> Option<OsString>) -> bool {
    env("HYPRLAND_INSTANCE_SIGNATURE").is_some()
        || env("XDG_CURRENT_DESKTOP")
            .and_then(|value| value.into_string().ok())
            .is_some_and(|value| {
                value
                    .split(':')
                    .any(|part| part.eq_ignore_ascii_case("hyprland"))
            })
}

#[cfg(test)]
mod tests {
    use super::{PlatformProfile, capabilities_for_resolved_profile, detect_profile_from_env};
    use std::collections::HashMap;
    use std::ffi::OsString;

    #[test]
    fn windows_profile_keeps_caption_buttons() {
        let capabilities = capabilities_for_resolved_profile(PlatformProfile::Windows);

        assert!(capabilities.show_window_caption_buttons);
        assert!(!capabilities.use_native_decorations);
    }

    #[test]
    fn hyprland_profile_hides_window_manager_buttons() {
        let capabilities = capabilities_for_resolved_profile(PlatformProfile::Hyprland);

        assert!(!capabilities.show_window_caption_buttons);
        assert!(!capabilities.use_native_decorations);
        assert!(capabilities.global_shortcuts_managed_externally);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn auto_profile_detects_hyprland_from_env() {
        let env = HashMap::from([(
            "HYPRLAND_INSTANCE_SIGNATURE",
            OsString::from("instance-signature"),
        )]);

        let profile = detect_profile_from_env(|name| env.get(name).cloned());

        assert_eq!(profile, PlatformProfile::Hyprland);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn auto_profile_detects_generic_linux_without_hyprland_env() {
        let env = HashMap::<&str, OsString>::new();

        let profile = detect_profile_from_env(|name| env.get(name).cloned());

        assert_eq!(profile, PlatformProfile::LinuxGeneric);
    }
}
