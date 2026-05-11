use scratchpad::app::startup::{StartupAction, StartupOpenTarget, parse_startup_action};
use std::path::PathBuf;

fn args(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| value.to_string()).collect()
}

fn run(values: &[&str]) -> scratchpad::app::startup::StartupOptions {
    match parse_startup_action(args(values)) {
        StartupAction::Run(options) => options,
        other => panic!("expected run action, got {other:?}"),
    }
}

#[test]
fn parses_positional_files() {
    let options = run(&["a.txt", "b.txt"]);

    assert_eq!(
        options.files,
        vec![PathBuf::from("a.txt"), PathBuf::from("b.txt")]
    );
    assert_eq!(options.open_target, StartupOpenTarget::SeparateTabs);
    assert!(options.restore_session);
}

#[test]
fn clean_mode_skips_session_restore() {
    let options = run(&["/clean", "a.txt"]);

    assert!(!options.restore_session);
    assert!(options.restore_session_explicit);
    assert_eq!(options.files, vec![PathBuf::from("a.txt")]);
}

#[test]
fn addto_active_targets_current_workspace() {
    let options = run(&["/addto:active", "a.txt"]);

    assert_eq!(options.open_target, StartupOpenTarget::ActiveTab);
    assert!(options.open_target_explicit);
}

#[test]
fn addto_index_is_one_based() {
    let options = run(&["/addto:index:3", "a.txt"]);

    assert_eq!(options.open_target, StartupOpenTarget::TabIndex(2));
}

#[test]
fn comma_delimited_file_payload_supports_quotes() {
    let options = run(&["/files:\"C:\\a one.txt\",\"D:\\b.txt\""]);

    assert_eq!(
        options.files,
        vec![PathBuf::from("C:\\a one.txt"), PathBuf::from("D:\\b.txt")]
    );
}

#[test]
fn invalid_switch_combination_returns_startup_notice() {
    let options = run(&["/clean", "/addto:index:2", "a.txt"]);

    assert!(
        options
            .startup_notice
            .unwrap()
            .contains("/clean cannot be combined")
    );
}

#[test]
fn addto_without_files_returns_startup_notice() {
    let options = run(&["/addto"]);

    assert!(options.startup_notice.unwrap().contains("/addto requires"));
}

#[test]
fn help_and_version_are_standalone_actions() {
    assert_eq!(parse_startup_action(args(&["/help"])), StartupAction::Help);
    assert_eq!(parse_startup_action(args(&["/?"])), StartupAction::Help);
    assert_eq!(
        parse_startup_action(args(&["/version"])),
        StartupAction::Version
    );
}
