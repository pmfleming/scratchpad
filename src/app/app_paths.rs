use std::ffi::OsString;
use std::path::PathBuf;

#[cfg(target_os = "windows")]
const APP_DIR_WINDOWS: &str = "Scratchpad";
const APP_DIR_UNIX: &str = "scratchpad";

#[must_use]
pub fn runtime_settings_root() -> PathBuf {
    settings_root_from_env(|name| std::env::var_os(name))
}

#[must_use]
pub fn runtime_session_root() -> PathBuf {
    session_root_from_env(|name| std::env::var_os(name))
}

#[must_use]
pub fn legacy_temp_root() -> PathBuf {
    std::env::temp_dir().join(APP_DIR_UNIX)
}

fn settings_root_from_env(mut env: impl FnMut(&str) -> Option<OsString>) -> PathBuf {
    platform_settings_root(&mut env).unwrap_or_else(legacy_temp_root)
}

fn session_root_from_env(mut env: impl FnMut(&str) -> Option<OsString>) -> PathBuf {
    platform_session_root(&mut env).unwrap_or_else(legacy_temp_root)
}

#[cfg(target_os = "windows")]
fn platform_settings_root(env: &mut impl FnMut(&str) -> Option<OsString>) -> Option<PathBuf> {
    env_path(env, "APPDATA").map(|root| root.join(APP_DIR_WINDOWS))
}

#[cfg(target_os = "windows")]
fn platform_session_root(env: &mut impl FnMut(&str) -> Option<OsString>) -> Option<PathBuf> {
    env_path(env, "LOCALAPPDATA").map(|root| root.join(APP_DIR_WINDOWS))
}

#[cfg(target_os = "linux")]
fn platform_settings_root(env: &mut impl FnMut(&str) -> Option<OsString>) -> Option<PathBuf> {
    if let Some(root) = env_path(env, "XDG_CONFIG_HOME") {
        return Some(root.join(APP_DIR_UNIX));
    }

    env_path(env, "HOME").map(|home| home.join(".config").join(APP_DIR_UNIX))
}

#[cfg(target_os = "linux")]
fn platform_session_root(env: &mut impl FnMut(&str) -> Option<OsString>) -> Option<PathBuf> {
    if let Some(root) = env_path(env, "XDG_STATE_HOME") {
        return Some(root.join(APP_DIR_UNIX));
    }

    env_path(env, "HOME").map(|home| home.join(".local").join("state").join(APP_DIR_UNIX))
}

#[cfg(not(any(target_os = "windows", target_os = "linux")))]
fn platform_settings_root(_env: &mut impl FnMut(&str) -> Option<OsString>) -> Option<PathBuf> {
    None
}

#[cfg(not(any(target_os = "windows", target_os = "linux")))]
fn platform_session_root(_env: &mut impl FnMut(&str) -> Option<OsString>) -> Option<PathBuf> {
    None
}

fn env_path(env: &mut impl FnMut(&str) -> Option<OsString>, name: &str) -> Option<PathBuf> {
    env(name)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::{session_root_from_env, settings_root_from_env};
    use std::collections::HashMap;
    use std::ffi::OsString;
    use std::path::PathBuf;

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_paths_follow_xdg_locations() {
        let env = HashMap::from([
            ("XDG_CONFIG_HOME", OsString::from("/config")),
            ("XDG_STATE_HOME", OsString::from("/state")),
        ]);

        assert_eq!(
            settings_root_from_env(|name| env.get(name).cloned()),
            PathBuf::from("/config/scratchpad")
        );
        assert_eq!(
            session_root_from_env(|name| env.get(name).cloned()),
            PathBuf::from("/state/scratchpad")
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_paths_fallback_to_home_when_xdg_unset() {
        let env = HashMap::from([("HOME", OsString::from("/home/user"))]);

        assert_eq!(
            settings_root_from_env(|name| env.get(name).cloned()),
            PathBuf::from("/home/user/.config/scratchpad")
        );
        assert_eq!(
            session_root_from_env(|name| env.get(name).cloned()),
            PathBuf::from("/home/user/.local/state/scratchpad")
        );
    }
}
