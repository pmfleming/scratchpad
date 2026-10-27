//! Thin, fail-closed orchestration. RustQualityLens owns metric and policy semantics.
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    ffi::OsStr,
    fs, io,
    path::Path,
    process::{Command, Stdio},
};

#[cfg(test)]
mod tests;

const REQUIRED_REPORTS: &[&str] = &[
    "hotspots.json",
    "clones.json",
    "rust_escape_hatches.json",
    "reliability_findings.json",
    "locality_metrics.json",
    "leverage_metrics.json",
    "test_quality.json",
    "coverage.json",
    "rust_practices.json",
];

fn required_evidence_complete(reports: &BTreeMap<String, Value>) -> bool {
    REQUIRED_REPORTS.iter().all(|name| {
        reports
            .get(*name)
            .is_some_and(|report| report["status"] == "complete")
    })
}

#[derive(Debug, Serialize, PartialEq)]
#[serde(tag = "status", rename_all = "snake_case")]
enum Outcome {
    Passed,
    Failed { exit_code: Option<i32> },
    Unavailable { reason: String },
    ExecutionError { reason: String },
}

fn execute(
    root: &Path,
    directory: &Path,
    name: &str,
    executable: &OsStr,
    args: &[&OsStr],
) -> io::Result<Outcome> {
    let stdout = fs::File::create(directory.join(format!("{name}.stdout.log")))?;
    let stderr = fs::File::create(directory.join(format!("{name}.stderr.log")))?;
    let result = Command::new(executable)
        .args(args)
        .current_dir(root)
        .stdin(Stdio::null())
        .stdout(stdout)
        .stderr(stderr)
        .status();
    Ok(match result {
        Ok(status) if status.success() => Outcome::Passed,
        Ok(status) => Outcome::Failed {
            exit_code: status.code(),
        },
        Err(error) if error.kind() == io::ErrorKind::NotFound => Outcome::Unavailable {
            reason: error.to_string(),
        },
        Err(error) => Outcome::ExecutionError {
            reason: error.to_string(),
        },
    })
}

fn git(root: &Path, args: &[&str]) -> io::Result<Vec<u8>> {
    let result = Command::new("git").args(args).current_dir(root).output()?;
    if !result.status.success() {
        return Err(io::Error::other(
            String::from_utf8_lossy(&result.stderr).into_owned(),
        ));
    }
    Ok(result.stdout)
}

fn file_hash(path: &Path) -> io::Result<String> {
    let mut hash = blake3::Hasher::new();
    hash.update_reader(fs::File::open(path)?)?;
    Ok(hash.finalize().to_hex().to_string())
}

// Include tracked and non-ignored untracked inputs, including deletions. Exclude
// this run itself even if the caller chose an output outside the ignored target/.
fn source_hash(root: &Path, output: &Path) -> io::Result<String> {
    let files = git(
        root,
        &[
            "ls-files",
            "--cached",
            "--others",
            "--exclude-standard",
            "-z",
        ],
    )?;
    let mut paths: Vec<_> = files
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
        .collect();
    paths.sort_unstable();
    paths.dedup();
    let mut hash = blake3::Hasher::new();
    for path in paths {
        let name = std::str::from_utf8(path).map_err(io::Error::other)?;
        let path = root.join(name);
        if path.starts_with(output) {
            continue;
        }
        hash.update(&(name.len() as u64).to_le_bytes());
        hash.update(name.as_bytes());
        match file_hash(&path) {
            Ok(digest) => {
                hash.update(digest.as_bytes());
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                hash.update(b"deleted");
            }
            Err(error) => return Err(error),
        }
    }
    Ok(hash.finalize().to_hex().to_string())
}

fn isolated_config(root: &Path, artifacts: &Path) -> io::Result<String> {
    let mut config: toml::Value =
        toml::from_str(&fs::read_to_string(root.join("rqlens.toml"))?).map_err(io::Error::other)?;
    let path_value = |path: &Path| {
        path.to_str()
            .map(|text| toml::Value::String(text.to_owned()))
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "RustQualityLens configuration paths must be UTF-8",
                )
            })
    };
    config["project_root"] = path_value(root)?;
    config["output_dir"] = path_value(artifacts)?;
    toml::to_string_pretty(&config).map_err(io::Error::other)
}

fn evidence(path: &Path) -> Value {
    match fs::read(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => json!({"status":"missing"}),
        Err(error) => json!({"status":"unreadable", "reason":error.to_string()}),
        Ok(bytes) => match serde_json::from_slice::<Value>(&bytes) {
            Err(error) => json!({"status":"invalid", "reason":error.to_string()}),
            Ok(value) => {
                let confidence = &value["measurement_confidence"];
                let status = match (
                    confidence["complete"].as_bool(),
                    confidence["partial"].as_bool(),
                ) {
                    (Some(true), Some(false)) => "complete",
                    (_, Some(true)) | (Some(false), _) => "partial",
                    _ => "unknown",
                };
                json!({"status":status, "schema_version":value["schema_version"], "confidence":confidence})
            }
        },
    }
}

fn collect_evidence(artifacts: &Path) -> io::Result<BTreeMap<String, Value>> {
    let mut reports = BTreeMap::new();
    if artifacts.is_dir() {
        for entry in fs::read_dir(artifacts)? {
            let path = entry?.path();
            if path.extension() == Some(OsStr::new("json")) {
                reports.insert(
                    path.file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned(),
                    evidence(&path),
                );
            }
        }
    }
    for &name in REQUIRED_REPORTS {
        reports
            .entry(name.to_owned())
            .or_insert_with(|| evidence(&artifacts.join(name)));
    }
    Ok(reports)
}

/// Never merges with previous runs or turns a partial/missing result into success.
pub(super) fn run(root: &Path, tool: &Path, output: &Path) -> io::Result<()> {
    let root = root.canonicalize()?;
    let tool = tool.canonicalize()?;
    let output = prepare_run(&root, &tool, output)?;
    let identity = capture_identity(&root, &output)?;
    let commands = collect_commands(&root, &output)?;
    publish_summary(&root, &output, identity, commands)
}

const TOOL_FILE: &str = if cfg!(windows) {
    "rqlens.exe"
} else {
    "rqlens"
};

struct RunIdentity {
    before: String,
    tool_hash: String,
    revision: String,
}

fn prepare_run(root: &Path, tool: &Path, output: &Path) -> io::Result<std::path::PathBuf> {
    // Exclusive creation also rejects symlinks and existing empty directories.
    if let Some(parent) = output.parent().filter(|path| !path.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    fs::create_dir(output)?;
    let output = output.canonicalize()?;
    let artifacts = output.join("artifacts");
    let config = output.join("rqlens.toml");
    fs::write(&config, isolated_config(root, &artifacts)?)?;
    fs::copy(tool, output.join(TOOL_FILE))?;
    Ok(output)
}

fn capture_identity(root: &Path, output: &Path) -> io::Result<RunIdentity> {
    let tool_hash = file_hash(&output.join(TOOL_FILE))?;
    let before = source_hash(root, output)?;
    fs::write(
        output.join("source.patch"),
        git(root, &["diff", "--binary", "HEAD"])?,
    )?;
    fs::write(
        output.join("source-status.txt"),
        git(root, &["status", "--porcelain=v1"])?,
    )?;
    let revision =
        String::from_utf8(git(root, &["rev-parse", "HEAD"])?).map_err(io::Error::other)?;
    Ok(RunIdentity {
        before,
        tool_hash,
        revision,
    })
}

fn collect_commands(root: &Path, output: &Path) -> io::Result<BTreeMap<&'static str, Outcome>> {
    let frozen = output.join(TOOL_FILE);
    let config = output.join("rqlens.toml");
    let mut commands = BTreeMap::new();
    for (name, executable, args) in [
        ("rustc", "rustc", vec!["-vV"]),
        ("cargo", "cargo", vec!["--version"]),
        ("audit-tool", "cargo-audit", vec!["audit", "--version"]),
        (
            "coverage-tool",
            "cargo-llvm-cov",
            vec!["llvm-cov", "--version"],
        ),
        ("feature-tool", "cargo-hack", vec!["hack", "--version"]),
    ] {
        let args: Vec<_> = args.iter().map(OsStr::new).collect();
        commands.insert(
            name,
            execute(root, output, name, OsStr::new(executable), &args)?,
        );
    }
    commands.insert(
        "rqlens-version",
        execute(
            root,
            output,
            "rqlens-version",
            frozen.as_os_str(),
            &[OsStr::new("--version")],
        )?,
    );
    for (name, args) in [
        ("measure", vec![OsStr::new("measure"), OsStr::new("all")]),
        ("verify", vec![OsStr::new("verify")]),
        (
            "check",
            vec![
                OsStr::new("check"),
                OsStr::new("--fail-on"),
                OsStr::new("partial"),
                OsStr::new("--fail-on"),
                OsStr::new("test-failure"),
                OsStr::new("--fail-on"),
                OsStr::new("practice-failure"),
                OsStr::new("--fail-on"),
                OsStr::new("reliability-finding"),
            ],
        ),
    ] {
        let mut args = args;
        args.extend([OsStr::new("--config"), config.as_os_str()]);
        commands.insert(
            name,
            execute(root, output, name, frozen.as_os_str(), &args)?,
        );
    }
    Ok(commands)
}

fn gate_diagnostics(path: &Path) -> Value {
    let parsed = fs::read(path)
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).map_err(io::Error::other));
    match parsed {
        Ok(report) => [
            "passed",
            "failed_policies",
            "input_failures",
            "policy_rule_evaluations",
            "policy_rule_violations",
            "practice_failures",
            "test_failures",
            "threshold_violations",
        ]
        .into_iter()
        .map(|key| (key.to_owned(), report[key].clone()))
        .collect(),
        Err(error) => json!({"passed":false, "report_error":error.to_string()}),
    }
}

fn publish_summary(
    root: &Path,
    output: &Path,
    identity: RunIdentity,
    commands: BTreeMap<&str, Outcome>,
) -> io::Result<()> {
    let RunIdentity {
        before,
        tool_hash,
        revision,
    } = identity;
    let reports = collect_evidence(&output.join("artifacts"))?;
    let after = source_hash(root, output)?;
    let unchanged_tool = file_hash(&output.join(TOOL_FILE))? == tool_hash;
    let gate = gate_diagnostics(&output.join("check.stdout.log"));
    let complete = required_evidence_complete(&reports);
    let passed = before == after
        && unchanged_tool
        && complete
        && gate["passed"] == true
        && commands.values().all(|result| *result == Outcome::Passed);
    let environment: BTreeMap<_, _> = [
        "RUSTFLAGS",
        "CARGO_ENCODED_RUSTFLAGS",
        "CARGO_BUILD_TARGET",
        "CARGO_LLVM_COV_TARGET_DIR",
        "LLVM_COV",
        "LLVM_PROFDATA",
    ]
    .into_iter()
    .filter_map(|name| std::env::var(name).ok().map(|value| (name, value)))
    .collect();
    let summary = json!({
        "schema_version":1, "passed":passed, "source_revision":revision.trim(),
        "source_blake3_before":before, "source_blake3_after":after,
        "tool_blake3":tool_hash, "tool_unchanged":unchanged_tool, "environment":environment,
        "commands":commands, "artifacts":reports, "required_evidence_complete":complete,
        "gate_report":"check.stdout.log", "gate":gate,
        "note":"Command success does not imply complete evidence. The strict RustQualityLens check owns policy and freshness validation; confidence unknown is not complete. Dirty source hashes detect changes, not a reconstructible source archive."
    });
    fs::write(
        output.join("summary.json"),
        serde_json::to_vec_pretty(&summary)?,
    )?;
    println!("Quality evidence: {} (passed={passed})", output.display());
    if passed {
        Ok(())
    } else {
        Err(io::Error::other(
            "quality gate failed; inspect summary.json and check.stdout.log",
        ))
    }
}
