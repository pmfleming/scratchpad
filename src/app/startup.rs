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
    pub restore_session_explicit: bool,
    pub open_target: StartupOpenTarget,
    pub open_target_explicit: bool,
    pub files: Vec<PathBuf>,
    pub log_cli: bool,
    pub startup_notice: Option<String>,
}

impl Default for StartupOptions {
    fn default() -> Self {
        Self {
            restore_session: true,
            restore_session_explicit: false,
            open_target: StartupOpenTarget::SeparateTabs,
            open_target_explicit: false,
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

pub const USAGE_TEXT: &str = concat!(
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
);
