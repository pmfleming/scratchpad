use std::path::PathBuf;

pub const USER_MANUAL_FILE_NAME: &str = "user-manual.md";

pub fn resolve_user_manual_path() -> PathBuf {
    manual_path_candidates()
        .into_iter()
        .find(|path| path.is_file())
        .unwrap_or_else(default_install_path)
}

fn manual_path_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    if let Ok(exe_path) = std::env::current_exe()
        && let Some(exe_dir) = exe_path.parent()
    {
        push_candidate(&mut candidates, exe_dir.join(USER_MANUAL_FILE_NAME));
    }

    if let Ok(current_dir) = std::env::current_dir() {
        push_candidate(&mut candidates, current_dir.join(USER_MANUAL_FILE_NAME));
        push_candidate(
            &mut candidates,
            current_dir.join("docs").join(USER_MANUAL_FILE_NAME),
        );
    }

    candidates
}

fn default_install_path() -> PathBuf {
    if let Ok(exe_path) = std::env::current_exe()
        && let Some(exe_dir) = exe_path.parent()
    {
        return exe_dir.join(USER_MANUAL_FILE_NAME);
    }

    PathBuf::from(USER_MANUAL_FILE_NAME)
}

fn push_candidate(candidates: &mut Vec<PathBuf>, candidate: PathBuf) {
    if !candidates.iter().any(|existing| existing == &candidate) {
        candidates.push(candidate);
    }
}
