#![forbid(unsafe_code)]

use scratchpad::app::startup::{StartupAction, StartupOpenTarget, parse_startup_action};
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
fn rejects_addto_without_files() {
    let action = parse_startup_action(args(&["/addto"]));

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
