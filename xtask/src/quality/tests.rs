use super::{
    Outcome, REQUIRED_REPORTS, evidence, execute, isolated_config, required_evidence_complete, run,
    source_hash,
};
use serde_json::json;
use std::{ffi::OsStr, fs, process::Command};

#[test]
fn confidence_is_not_inferred_from_successful_json_parsing() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("report.json");
    assert_eq!(evidence(&path)["status"], "missing");
    fs::write(&path, b"not json").unwrap();
    assert_eq!(evidence(&path)["status"], "invalid");
    for (confidence, expected) in [
        (json!({"complete":true,"partial":false}), "complete"),
        (
            json!({"complete":false,"partial":true,"missing_input":["tool unavailable"]}),
            "partial",
        ),
        (json!({"complete":true,"partial":true}), "partial"),
        (json!({}), "unknown"),
    ] {
        fs::write(
            &path,
            json!({"measurement_confidence":confidence}).to_string(),
        )
        .unwrap();
        assert_eq!(evidence(&path)["status"], expected);
    }
}

#[test]
fn successful_commands_cannot_certify_missing_or_partial_required_reports() {
    let complete: std::collections::BTreeMap<_, _> = REQUIRED_REPORTS
        .iter()
        .map(|name| ((*name).to_owned(), json!({"status":"complete"})))
        .collect();
    assert!(required_evidence_complete(&complete));
    for &name in REQUIRED_REPORTS {
        for status in ["partial", "missing", "invalid", "unknown", "unreadable"] {
            let mut reports = complete.clone();
            reports.insert(name.to_owned(), json!({"status":status}));
            assert!(!required_evidence_complete(&reports), "{name}/{status}");
        }
        let mut reports = complete.clone();
        reports.remove(name);
        assert!(!required_evidence_complete(&reports));
    }
}

#[test]
fn published_summary_requires_stable_inputs_complete_evidence_and_a_successful_gate() {
    for case in [
        "pass",
        "partial",
        "gate-failure",
        "invalid-gate",
        "command-failure",
        "unavailable-tool",
        "source-change",
        "tool-change",
    ] {
        let root = tempfile::tempdir().unwrap();
        assert!(
            Command::new("git")
                .args(["init", "--quiet"])
                .arg(root.path())
                .status()
                .unwrap()
                .success()
        );
        fs::write(root.path().join("input.rs"), "before").unwrap();
        let output = root.path().join("run");
        let artifacts = output.join("artifacts");
        fs::create_dir_all(&artifacts).unwrap();
        fs::write(output.join(super::TOOL_FILE), "frozen tool").unwrap();
        for &name in REQUIRED_REPORTS {
            fs::write(
                artifacts.join(name),
                json!({"measurement_confidence":{"complete":true,"partial":false}}).to_string(),
            )
            .unwrap();
        }
        fs::write(
            output.join("check.stdout.log"),
            json!({"passed":true}).to_string(),
        )
        .unwrap();
        let identity = super::RunIdentity {
            before: source_hash(root.path(), &output).unwrap(),
            tool_hash: super::file_hash(&output.join(super::TOOL_FILE)).unwrap(),
            revision: "test".into(),
        };
        let mut commands = std::collections::BTreeMap::from([("check", Outcome::Passed)]);
        match case {
            "partial" => fs::write(
                artifacts.join("coverage.json"),
                json!({"measurement_confidence":{"complete":false,"partial":true}}).to_string(),
            )
            .unwrap(),
            "gate-failure" => fs::write(
                output.join("check.stdout.log"),
                json!({"passed":false,"policy_rule_violations":["fixture"]}).to_string(),
            )
            .unwrap(),
            "invalid-gate" => fs::write(output.join("check.stdout.log"), "not JSON").unwrap(),
            "command-failure" => {
                commands.insert("measure", Outcome::Failed { exit_code: Some(1) });
            }
            "unavailable-tool" => {
                commands.insert(
                    "audit",
                    Outcome::Unavailable {
                        reason: "not installed".into(),
                    },
                );
            }
            "source-change" => fs::write(root.path().join("input.rs"), "after").unwrap(),
            "tool-change" => fs::write(output.join(super::TOOL_FILE), "different tool").unwrap(),
            _ => {}
        }
        let result = super::publish_summary(root.path(), &output, identity, commands);
        let summary: serde_json::Value =
            serde_json::from_slice(&fs::read(output.join("summary.json")).unwrap()).unwrap();
        assert_eq!(result.is_ok(), case == "pass", "{case}");
        assert_eq!(summary["passed"], case == "pass", "{case}");
    }
}

#[test]
fn commands_distinguish_failures_from_unavailable_tools() {
    let root = tempfile::tempdir().unwrap();
    let executable = std::env::current_exe().unwrap();
    assert_eq!(
        execute(
            root.path(),
            root.path(),
            "pass",
            executable.as_os_str(),
            &[OsStr::new("--help")]
        )
        .unwrap(),
        Outcome::Passed
    );
    assert!(matches!(
        execute(
            root.path(),
            root.path(),
            "fail",
            executable.as_os_str(),
            &[OsStr::new("--invalid-test-option")]
        )
        .unwrap(),
        Outcome::Failed { .. }
    ));
    assert!(matches!(
        execute(
            root.path(),
            root.path(),
            "missing",
            root.path().join("no-tool").as_os_str(),
            &[]
        )
        .unwrap(),
        Outcome::Unavailable { .. }
    ));
}

#[test]
fn fresh_run_refuses_existing_output_and_preserves_policy_configuration() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("rqlens.toml"), "project_root='.'\noutput_dir='old'\nsource_roots=['src','tests']\n[policy.rules.sample]\nlevel='error'\nmax=0\n").unwrap();
    let config: toml::Value =
        toml::from_str(&isolated_config(root.path(), &root.path().join("new artifacts")).unwrap())
            .unwrap();
    assert_eq!(
        config["policy"]["rules"]["sample"]["max"].as_integer(),
        Some(0)
    );
    assert_eq!(config["source_roots"].as_array().unwrap().len(), 2);
    assert_eq!(config["project_root"].as_str(), root.path().to_str());
    assert!(run(root.path(), &std::env::current_exe().unwrap(), root.path()).is_err());
    assert!(!root.path().join("summary.json").exists());
}

#[test]
fn source_identity_covers_untracked_edits_and_deletions_but_not_run_outputs() {
    let root = tempfile::tempdir().unwrap();
    assert!(
        Command::new("git")
            .args(["init", "--quiet"])
            .arg(root.path())
            .status()
            .unwrap()
            .success()
    );
    let output = root.path().join("run");
    let file = root.path().join("input with spaces.rs");
    fs::write(&file, "before").unwrap();
    let before = source_hash(root.path(), &output).unwrap();
    fs::create_dir(&output).unwrap();
    fs::write(output.join("log"), "output").unwrap();
    assert_eq!(source_hash(root.path(), &output).unwrap(), before);
    fs::write(&file, "after!").unwrap();
    let changed = source_hash(root.path(), &output).unwrap();
    assert_ne!(before, changed);
    assert!(
        Command::new("git")
            .args(["add", "."])
            .current_dir(root.path())
            .status()
            .unwrap()
            .success()
    );
    fs::remove_file(file).unwrap();
    assert_ne!(source_hash(root.path(), &output).unwrap(), changed);
}
