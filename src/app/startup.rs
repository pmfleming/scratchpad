use std::ffi::OsString;
use std::path::PathBuf;

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
    T: Into<OsString>,
{
    let mut parser = StartupActionParser::default();

    for raw_arg in args {
        match parser.parse_argument(raw_arg.into()) {
            Ok(ParseDirective::Continue) => {}
            Ok(ParseDirective::Help) => return StartupAction::Help,
            Ok(ParseDirective::Version) => return StartupAction::Version,
            Err(error) => return invalid_startup_action(error),
        }
    }

    parser.finish()
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

fn invalid_startup_action(message: impl Into<String>) -> StartupAction {
    StartupAction::Run(StartupOptions {
        startup_notice: Some(message.into()),
        ..Default::default()
    })
}

fn is_help_switch(arg: &str) -> bool {
    equals_switch(arg, "/help") || equals_switch(arg, "/?")
}

fn is_version_switch(arg: &str) -> bool {
    equals_switch(arg, "/version")
}

fn equals_switch(left: &str, right: &str) -> bool {
    left.eq_ignore_ascii_case(right)
}

fn strip_switch_prefix<'a>(arg: &'a str, prefix: &str) -> Option<&'a str> {
    arg.get(..prefix.len())
        .is_some_and(|candidate| candidate.eq_ignore_ascii_case(prefix))
        .then_some(&arg[prefix.len()..])
}

#[derive(Default)]
struct StartupActionParser {
    options: StartupOptions,
    requested_clean: bool,
    saw_here: bool,
    saw_addto: bool,
}

enum ParseDirective {
    Continue,
    Help,
    Version,
}

impl StartupActionParser {
    fn parse_argument(&mut self, raw_arg: OsString) -> Result<ParseDirective, String> {
        let arg = raw_arg.to_string_lossy().into_owned();

        if is_help_switch(&arg) {
            return Ok(ParseDirective::Help);
        }
        if is_version_switch(&arg) {
            return Ok(ParseDirective::Version);
        }
        if self.try_parse_flag_switch(&arg)? || self.try_parse_prefixed_switch(&arg)? {
            return Ok(ParseDirective::Continue);
        }
        if arg.starts_with('/') {
            return Err(format!(
                "Unknown startup switch: {arg}. Use /help to show supported switches."
            ));
        }

        self.options.files.push(PathBuf::from(raw_arg));
        Ok(ParseDirective::Continue)
    }

    fn try_parse_flag_switch(&mut self, arg: &str) -> Result<bool, String> {
        if equals_switch(arg, "/log-cli") {
            self.options.log_cli = true;
            return Ok(true);
        }
        if equals_switch(arg, "/clean") {
            self.requested_clean = true;
            return Ok(true);
        }
        if equals_switch(arg, "/here") {
            self.activate_here_target()?;
            return Ok(true);
        }
        if equals_switch(arg, "/addto") {
            self.activate_addto_target(StartupOpenTarget::ActiveTab)?;
            return Ok(true);
        }

        Ok(false)
    }

    fn try_parse_prefixed_switch(&mut self, arg: &str) -> Result<bool, String> {
        if let Some(payload) = strip_switch_prefix(arg, "/addto:") {
            self.activate_addto_target(parse_addto_target(payload)?)?;
            return Ok(true);
        }
        if let Some(payload) = strip_switch_prefix(arg, "/files:") {
            let mut files = parse_file_list(payload)?;
            self.options.files.append(&mut files);
            return Ok(true);
        }

        Ok(false)
    }

    fn activate_here_target(&mut self) -> Result<(), String> {
        if self.saw_addto {
            return Err(
                "/here cannot be combined with /addto. Use one workspace-targeting switch."
                    .to_owned(),
            );
        }

        self.saw_here = true;
        self.options.open_target = StartupOpenTarget::ActiveTab;
        Ok(())
    }

    fn activate_addto_target(&mut self, target: StartupOpenTarget) -> Result<(), String> {
        if self.saw_here {
            return Err(
                "/addto cannot be combined with /here. Use one workspace-targeting switch."
                    .to_owned(),
            );
        }

        self.saw_addto = true;
        self.options.open_target = target;
        Ok(())
    }

    fn finish(mut self) -> StartupAction {
        if self.requested_clean {
            self.options.restore_session = false;
        }

        if let Err(error) = self.validate_final_state() {
            return invalid_startup_action(error);
        }

        StartupAction::Run(self.options)
    }

    fn validate_final_state(&self) -> Result<(), String> {
        if self.saw_addto && self.options.files.is_empty() {
            return Err(
                "/addto requires at least one incoming file path or a /files: payload.".to_owned(),
            );
        }

        if self.requested_clean
            && matches!(self.options.open_target, StartupOpenTarget::TabIndex(_))
        {
            return Err(
                "/clean cannot be combined with /addto:index:N because there is no restored tab index to target."
                    .to_owned(),
            );
        }

        Ok(())
    }
}

fn parse_addto_target(payload: &str) -> Result<StartupOpenTarget, String> {
    if payload.eq_ignore_ascii_case("active") {
        return Ok(StartupOpenTarget::ActiveTab);
    }

    let Some(index_payload) = strip_switch_prefix(payload, "index:") else {
        return Err(
            "Invalid /addto target. Use /addto, /addto:active, or /addto:index:N.".to_owned(),
        );
    };

    let one_based_index = index_payload
        .parse::<usize>()
        .map_err(|_| "Invalid /addto:index:N value. N must be a positive integer.".to_owned())?;

    if one_based_index == 0 {
        return Err("Invalid /addto:index:N value. N must start at 1.".to_owned());
    }

    Ok(StartupOpenTarget::TabIndex(one_based_index - 1))
}

fn parse_file_list(payload: &str) -> Result<Vec<PathBuf>, String> {
    if payload.trim().is_empty() {
        return Err("/files: requires at least one file path.".to_owned());
    }

    let mut entries = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;

    for ch in payload.chars() {
        match ch {
            '"' => in_quotes = !in_quotes,
            ',' if !in_quotes => {
                push_file_list_entry(&mut entries, &current)?;
                current.clear();
            }
            _ => current.push(ch),
        }
    }

    if in_quotes {
        return Err("Malformed /files: list. Quotes must be balanced.".to_owned());
    }

    push_file_list_entry(&mut entries, &current)?;
    Ok(entries)
}

fn push_file_list_entry(entries: &mut Vec<PathBuf>, current: &str) -> Result<(), String> {
    let candidate = current.trim();
    if candidate.is_empty() {
        return Err("Malformed /files: list. Empty file entries are not allowed.".to_owned());
    }

    entries.push(PathBuf::from(candidate));
    Ok(())
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
