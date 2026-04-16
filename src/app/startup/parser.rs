use super::{StartupAction, StartupOpenTarget, StartupOptions};
use std::ffi::OsString;
use std::path::PathBuf;

pub(super) fn parse_startup_action<I, T>(args: I) -> StartupAction
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
        self.options.open_target_explicit = true;
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
        self.options.open_target_explicit = true;
        Ok(())
    }

    fn finish(mut self) -> StartupAction {
        if self.requested_clean {
            self.options.restore_session = false;
            self.options.restore_session_explicit = true;
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
