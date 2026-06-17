use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OpenFileDialogKind {
    OpenFile,
    OpenFileHere,
}

impl OpenFileDialogKind {
    #[must_use]
    fn title(self) -> &'static str {
        match self {
            Self::OpenFile => "Open Files - Scratchpad",
            Self::OpenFileHere => "Open Files Here - Scratchpad",
        }
    }

    pub(crate) fn action_name(self) -> &'static str {
        match self {
            Self::OpenFile => "Open file dialog",
            Self::OpenFileHere => "Open Here dialog",
        }
    }
}

#[must_use]
pub(crate) fn pick_open_files(kind: OpenFileDialogKind) -> Option<Vec<PathBuf>> {
    imp::pick_open_files(kind)
}

pub(crate) fn spawn_pick_open_files(
    kind: OpenFileDialogKind,
) -> std::io::Result<Receiver<Option<Vec<PathBuf>>>> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .name("scratchpad-open-file-dialog".to_owned())
        .spawn(move || {
            let _ = tx.send(pick_open_files(kind));
        })?;
    Ok(rx)
}

#[must_use]
pub(crate) fn pick_save_as_path(default_file_name: &str) -> Option<PathBuf> {
    imp::pick_save_as_path(default_file_name)
}

pub(crate) fn reveal_file(path: &Path) -> std::io::Result<()> {
    imp::reveal_file(path)
}

#[must_use]
pub(crate) fn reveal_file_label() -> &'static str {
    imp::REVEAL_FILE_LABEL
}

#[must_use]
pub(crate) fn reveal_file_error_message() -> &'static str {
    imp::REVEAL_FILE_ERROR_MESSAGE
}

fn text_file_dialog(title: &str) -> rfd::FileDialog {
    rfd::FileDialog::new()
        .set_title(title)
        .add_filter(
            "Text and source files",
            &[
                "txt", "md", "markdown", "log", "rs", "toml", "json", "yaml", "yml",
            ],
        )
        .add_filter("All files", &["*"])
}

#[cfg(target_os = "windows")]
mod imp {
    use super::{OpenFileDialogKind, text_file_dialog};
    use std::ffi::OsString;
    use std::path::{Path, PathBuf};
    use std::process::Command;

    pub(super) const REVEAL_FILE_LABEL: &str = "Reveal In Explorer";
    pub(super) const REVEAL_FILE_ERROR_MESSAGE: &str = "Could not reveal this file in Explorer.";

    pub(super) fn pick_open_files(kind: OpenFileDialogKind) -> Option<Vec<PathBuf>> {
        text_file_dialog(kind.title()).pick_files()
    }

    pub(super) fn pick_save_as_path(default_file_name: &str) -> Option<PathBuf> {
        text_file_dialog("Save File - Scratchpad")
            .set_file_name(default_file_name)
            .save_file()
    }

    pub(super) fn reveal_file(path: &Path) -> std::io::Result<()> {
        let mut select_arg = OsString::from("/select,");
        select_arg.push(path);
        Command::new("explorer.exe")
            .arg(select_arg)
            .spawn()
            .map(|_| ())
    }
}

#[cfg(target_os = "linux")]
mod imp {
    use super::{OpenFileDialogKind, text_file_dialog};
    use std::path::{Path, PathBuf};
    use std::process::Command;

    pub(super) const REVEAL_FILE_LABEL: &str = "Open Containing Folder";
    pub(super) const REVEAL_FILE_ERROR_MESSAGE: &str = "Could not open the containing folder.";

    pub(super) fn pick_open_files(kind: OpenFileDialogKind) -> Option<Vec<PathBuf>> {
        // Keep the Linux path explicit. rfd uses the XDG desktop portal backend
        // on Linux in this build, which is the right boundary for Hyprland and
        // other Wayland compositors.
        text_file_dialog(kind.title()).pick_files()
    }

    pub(super) fn pick_save_as_path(default_file_name: &str) -> Option<PathBuf> {
        text_file_dialog("Save File - Scratchpad")
            .set_file_name(default_file_name)
            .save_file()
    }

    pub(super) fn reveal_file(path: &Path) -> std::io::Result<()> {
        let target = path.parent().unwrap_or(path);
        Command::new("xdg-open").arg(target).spawn().map(|_| ())
    }
}

#[cfg(not(any(target_os = "windows", target_os = "linux")))]
mod imp {
    use super::{OpenFileDialogKind, text_file_dialog};
    use std::path::{Path, PathBuf};

    pub(super) const REVEAL_FILE_LABEL: &str = "Open Containing Folder";
    pub(super) const REVEAL_FILE_ERROR_MESSAGE: &str = "Could not open the containing folder.";

    pub(super) fn pick_open_files(kind: OpenFileDialogKind) -> Option<Vec<PathBuf>> {
        text_file_dialog(kind.title()).pick_files()
    }

    pub(super) fn pick_save_as_path(default_file_name: &str) -> Option<PathBuf> {
        text_file_dialog("Save File - Scratchpad")
            .set_file_name(default_file_name)
            .save_file()
    }

    pub(super) fn reveal_file(_path: &Path) -> std::io::Result<()> {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "opening containing folders is unsupported on this platform",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::{OpenFileDialogKind, reveal_file_label};

    #[test]
    fn open_dialog_kinds_have_distinct_titles() {
        assert_eq!(
            OpenFileDialogKind::OpenFile.title(),
            "Open Files - Scratchpad"
        );
        assert_eq!(
            OpenFileDialogKind::OpenFileHere.title(),
            "Open Files Here - Scratchpad"
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_reveal_label_is_file_manager_neutral() {
        assert_eq!(reveal_file_label(), "Open Containing Folder");
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_reveal_label_names_explorer() {
        assert_eq!(reveal_file_label(), "Reveal In Explorer");
    }
}
