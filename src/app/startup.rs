use std::path::PathBuf;

mod parser;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StartupAction {
    Run(StartupOptions),
    Help,
    Version,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StartupOptions {
    pub restore_session: bool,
    pub open_target: StartupOpenTarget,
    pub files: Vec<PathBuf>,
    pub log_cli: bool,
    pub startup_notice: Option<String>,
}

impl Default for StartupOptions {
    fn default() -> Self {
        Self {
            restore_session: true,
            open_target: StartupOpenTarget::SeparateTabs,
            files: Vec::new(),
            log_cli: false,
            startup_notice: None,
        }
    }
}

impl StartupOptions {
    pub fn clean() -> Self {
        Self {
            restore_session: false,
            ..Default::default()
        }
    }

    pub fn describe(&self) -> String {
        let target = match self.open_target {
            StartupOpenTarget::SeparateTabs => "separate-tabs".to_owned(),
            StartupOpenTarget::ActiveTab => "active-tab".to_owned(),
            StartupOpenTarget::TabIndex(index) => format!("tab-index-{}", index + 1),
        };
        let files = self
            .files
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            "restore_session={}, open_target={}, files=[{}], notice={}",
            self.restore_session,
            target,
            files,
            self.startup_notice.as_deref().unwrap_or("none")
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StartupOpenTarget {
    SeparateTabs,
    ActiveTab,
    TabIndex(usize),
}

pub fn parse_startup_action_from_env() -> StartupAction {
    parse_startup_action(std::env::args_os().skip(1))
}

pub fn parse_startup_action<I, T>(args: I) -> StartupAction
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString>,
{
    parser::parse_startup_action(args)
}

pub fn usage_text() -> &'static str {
    concat!(
        "Scratchpad command line usage\n",
        "\n",
        "  scratchpad.exe [switches] [files...]\n",
        "\n",
        "Switches:\n",
        "  /clean                Start with one fresh untitled tab and skip session restore\n",
        "  /here                 Add incoming files into the active workspace tab\n",
        "  /addto                Alias for /addto:active\n",
        "  /addto:active         Add incoming files into the active workspace tab\n",
        "  /addto:index:N        Add incoming files into the Nth tab (1-based)\n",
        "  /files:\"a\",\"b\"      Comma-delimited quoted file list in one argument\n",
        "  /log-cli              Log parsed startup options to the runtime log\n",
        "  /help or /?           Show this help text\n",
        "  /version              Print the application version and exit\n",
        "\n",
        "Examples:\n",
        "  scratchpad.exe \"C:\\notes\\a.txt\" \"C:\\notes\\b.txt\"\n",
        "  scratchpad.exe /clean \"C:\\notes\\a.txt\"\n",
        "  scratchpad.exe /addto:active /files:\"C:\\a.txt\",\"C:\\b.txt\"\n"
    )
}

#[cfg(test)]
mod tests {
    use super::{StartupAction, StartupOpenTarget, parse_startup_action};
    use std::path::PathBuf;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    #[test]
    fn parses_positional_files() {
        let action = parse_startup_action(args(&["C:\\one.txt", "C:\\two.txt"]));

        match action {
            StartupAction::Run(options) => {
                assert!(options.restore_session);
                assert_eq!(options.open_target, StartupOpenTarget::SeparateTabs);
                assert_eq!(
                    options.files,
                    vec![PathBuf::from("C:\\one.txt"), PathBuf::from("C:\\two.txt")]
                );
            }
            _ => panic!("expected normal run action"),
        }
    }

    #[test]
    fn parses_clean_mode() {
        let action = parse_startup_action(args(&["/clean"]));

        match action {
            StartupAction::Run(options) => {
                assert!(!options.restore_session);
                assert!(options.files.is_empty());
            }
            _ => panic!("expected normal run action"),
        }
    }

    #[test]
    fn parses_addto_active() {
        let action = parse_startup_action(args(&["/addto", "C:\\file.txt"]));

        match action {
            StartupAction::Run(options) => {
                assert_eq!(options.open_target, StartupOpenTarget::ActiveTab);
                assert_eq!(options.files, vec![PathBuf::from("C:\\file.txt")]);
            }
            _ => panic!("expected normal run action"),
        }
    }

    #[test]
    fn parses_addto_index_as_one_based() {
        let action = parse_startup_action(args(&["/addto:index:2", "C:\\file.txt"]));

        match action {
            StartupAction::Run(options) => {
                assert_eq!(options.open_target, StartupOpenTarget::TabIndex(1));
            }
            _ => panic!("expected normal run action"),
        }
    }

    #[test]
    fn parses_comma_delimited_file_list() {
        let action = parse_startup_action(args(&["/files:\"C:\\one.txt\",\"C:\\two files.txt\""]));

        match action {
            StartupAction::Run(options) => {
                assert_eq!(
                    options.files,
                    vec![
                        PathBuf::from("C:\\one.txt"),
                        PathBuf::from("C:\\two files.txt"),
                    ]
                );
            }
            _ => panic!("expected normal run action"),
        }
    }

    #[test]
    fn rejects_invalid_switch_combinations() {
        let action = parse_startup_action(args(&["/clean", "/addto:index:2", "C:\\file.txt"]));

        match action {
            StartupAction::Run(options) => {
                assert!(options.files.is_empty());
                assert!(options.startup_notice.is_some());
            }
            _ => panic!("expected normal run action"),
        }
    }

    #[test]
    fn returns_help_action() {
        let action = parse_startup_action(args(&["/help"]));
        assert!(matches!(action, StartupAction::Help));
    }

    #[test]
    fn returns_version_action() {
        let action = parse_startup_action(args(&["/version"]));
        assert!(matches!(action, StartupAction::Version));
    }
}
