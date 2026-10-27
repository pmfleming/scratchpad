#!/usr/bin/env python3
"""Exercise opt-in RQLens lint profiles through real Cargo checks and gates.

Provide a frozen executable, a NEW output directory, and an independently frozen
helper manifest. Fixtures are dependency-free and never execute their failing
lint examples. Expected negative cases count as success, not waived failures.
This probe establishes selected-lint behavior, not whole-guide compliance.
"""

import argparse
import hashlib
import json
import os
from pathlib import Path
import shlex
import shutil
import subprocess
import sys


CLEAN = '''//! Isolated lint-profile consumer fixture.

/// Return the fixture value.
#[must_use]
pub fn answer() -> u8 {
    42
}
'''
DEBUG = CLEAN.replace("    42\n", "    dbg!(42)\n")
NO_DEBUG = '''//! Isolated role-specific Debug fixture.

/// A public library-facing type without Debug.
pub struct Ticket;
'''
GOOGLE_FAILURE = CLEAN.replace("#[must_use]\n", "")
ENUM_GLOB = '''//! Google permits context-specific enum variant glob imports.

/// A fixture choice.
pub enum Choice {
    First,
    Second,
}

use Choice::*;

/// Return the fixture choice.
#[must_use]
pub fn answer() -> Choice {
    First
}
'''
JUSTIFIED = DEBUG.replace(
    "#[must_use]",
    '#[allow(clippy::dbg_macro, reason = "debug tracing is the purpose of this isolated fixture")]\n'
    "#[must_use]",
)

CASES = [
    ("default-compatibility", None, DEBUG, "passed", None),
    ("application-pass", "application-v1", CLEAN, "passed", None),
    ("application-debug-failure", "application-v1", DEBUG, "failed", "clippy::dbg_macro"),
    ("library-pass", "library-v1", CLEAN, "passed", None),
    ("library-debug-failure", "library-v1", NO_DEBUG, "failed", "missing_debug_implementations"),
    ("application-debug-exception", "application-v1", NO_DEBUG, "passed", None),
    ("library-target-scope", "library-v1", CLEAN, "passed", None),
    ("library-bin-only", "library-v1", "fn main() {}\n", "failed", None),
    ("google-pass", "google-v1", CLEAN, "passed", None),
    ("google-must-use-failure", "google-v1", GOOGLE_FAILURE, "failed", "clippy::must_use_candidate"),
    ("google-enum-glob-exception", "google-v1", ENUM_GLOB, "passed", None),
    ("justified-local-allow", "application-v1", JUSTIFIED, "passed", None),
    ("unknown-lint", "application-v1", '#![warn(rqlens_nonexistent_lint)]\n' + CLEAN, "failed", "unknown_lints"),
    ("unavailable-clippy", "application-v1", CLEAN, "unavailable", None),
    ("unavailable-selected-lint", "application-v1", CLEAN, "unavailable", None),
    ("unknown-profile", "rqlens-nonexistent-profile-v1", CLEAN, "invalid", None),
]


def execute(command, directory, name, environment=None, timeout=180):
    with (directory / f"{name}.stdout.log").open("w") as stdout:
        with (directory / f"{name}.stderr.log").open("w") as stderr:
            result = subprocess.run(
                list(map(str, command)),
                stdin=subprocess.DEVNULL,
                stdout=stdout,
                stderr=stderr,
                env=environment,
                timeout=timeout,
                check=False,
            )
    return result.returncode


def setup(directory, source, profile, helper, cargo):
    (directory / "src").mkdir(parents=True)
    source_name = "main.rs" if directory.name == "library-bin-only" else "lib.rs"
    (directory / "src" / source_name).write_text(source)
    if directory.name == "library-target-scope":
        (directory / "src/main.rs").write_text(
            "pub struct BinaryOnlyType;\n\nfn main() {\n    let _ = BinaryOnlyType;\n}\n"
        )
    (directory / "Cargo.toml").write_text(
        '[package]\nname = "rqlens-profile-consumer"\nversion = "0.0.0"\n'
        'edition = "2024"\nrust-version = "1.95"\nlicense = "MIT"\n'
        'repository = "https://example.invalid/rqlens-profile-consumer"\n'
    )
    shutil.copyfile(Path(__file__).resolve().parents[1] / "LICENSE", directory / "LICENSE")
    for filename in (
        "CONTRIBUTING.md", "CODE_OF_CONDUCT.md", "SECURITY.md", "CHANGELOG.md"
    ):
        (directory / filename).write_text("Isolated consumer fixture, not project policy.\n")
    (directory / "rust-toolchain.toml").write_text('[toolchain]\nchannel = "1.95"\n')
    config = directory / "rqlens.toml"
    config.write_text(
        'project_root = "."\nsource_roots = ["src"]\noutput_dir = "artifacts"\n'
        '[rust]\nidentity_resolution = "auto"\n'
        f'helper_manifest = {json.dumps(str(helper))}\n'
        '[verification]\ntimeout_seconds = 60\nworkspace = false\n'
        'all_targets = true\nall_features = false\nlocked = true\n'
        + (f'lint_profile = {json.dumps(profile)}\n' if profile else '')
        + '[policy.rules."rust.project.license"]\nlevel = "error"\nmax = 0\n'
    )
    for name, command in (
        ("format", [cargo, "fmt", "--manifest-path", directory / "Cargo.toml"]),
        ("lock", [cargo, "generate-lockfile", "--offline", "--manifest-path", directory / "Cargo.toml"]),
    ):
        if execute(command, directory, name) != 0:
            raise RuntimeError(f"fixture preparation failed: {directory}/{name}")
    return config


def selection(report):
    # The selected profile is evidence, not an inferred project role.
    return next(
        (check["lint_profile"] for check in report["data"]["checks"]
         if check["rule_id"] == "rust.official.clippy-profile"),
        None,
    )


def inspect(directory, profile, expected, diagnostic, exit_code):
    errors = []
    if expected == "invalid":
        stderr = (directory / "enforce.stderr.log").read_text()
        if exit_code == 0 or "profile" not in stderr.lower():
            errors.append(f"unknown profile not rejected: exit={exit_code}, stderr={stderr}")
        if (directory / "artifacts/rust_practices.json").exists():
            errors.append("unknown profile unexpectedly produced practices")
        return {"exit_code": exit_code, "errors": errors, "stderr": stderr}

    report = json.loads((directory / "artifacts/rust_practices.json").read_text())
    policy = json.loads((directory / "artifacts/policy_report.json").read_text())
    checks = report["data"]["checks"]
    baseline = next(check for check in checks if check["rule_id"] == "rust.official.clippy")
    clippy = next(
        check for check in checks
        if check["rule_id"] == ("rust.official.clippy-profile" if profile else "rust.official.clippy")
    )
    passing = expected == "passed"
    if (exit_code == 0) != passing:
        errors.append(f"unexpected enforce exit {exit_code}; expected {expected}")
    if policy["passed"] != passing:
        errors.append(f"policy passed={policy['passed']}; expected {passing}")
    if clippy["status"] != expected:
        errors.append(f"Clippy status={clippy['status']}; expected {expected}")
    diagnostics = [item for check in checks for item in check.get("diagnostics", [])]
    if diagnostic and not any(item.get("code") == diagnostic for item in diagnostics):
        errors.append(f"missing real diagnostic code {diagnostic}")
    selected = selection(report)
    command = selected["planned_command"] if selected else clippy["evidence"]["command"]
    baseline_command = baseline["evidence"]["command"]
    expected_baseline = [
        "cargo", "clippy", "--all-targets", "--locked", "--message-format=json", "--", "-D", "warnings"
    ]
    if baseline_command != expected_baseline:
        errors.append(f"baseline command changed: {baseline_command}")
    if profile:
        identity = selected["identity"]
        if identity["id"] != profile or identity["version"] != 1 or not identity["registry_digest"]:
            errors.append(f"missing/wrong versioned profile identity: {identity}")
        if report["data"]["lint_profile"] != identity:
            errors.append("summary profile identity disagrees with executed check")
        libraries = profile == "library-v1"
        if ("--lib" in command) != libraries or ("--all-targets" in command) == libraries:
            errors.append(f"incorrect profile target scope: {command}")
        if selected["scope"]["all_targets"] != (not libraries):
            errors.append(f"incorrect recorded scope: {selected['scope']}")
        if selected["scope"]["all_features"] is not False:
            errors.append("fixture feature scope was broadened")
        names = [lint["name"] for lint in selected["selected_lints"]]
        if names != selected["profile"]["lints"]:
            errors.append("effective lint selection disagrees with registry profile")
        groups = {"restriction", "pedantic", "all", "nursery"}
        if any(name.split("::")[-1] in groups for name in names):
            errors.append("blanket lint group unexpectedly selected")
        for name in names:
            if not any(command[i:i + 2] == ["-W", name] for i in range(len(command) - 1)):
                errors.append(f"selected lint missing from planned command: {name}")
        if selected["execution_attempted"] != (expected != "unavailable"):
            errors.append("profile execution-attempt status disagrees with availability")
        if not selected["backend"]["versions"] or "target" not in selected["scope"]:
            errors.append("tool versions or target scope not visible")
        if directory.name == "justified-local-allow":
            overrides = selected["local_overrides"]["records"]
            if not any(item["level"] == "allow" and "clippy::dbg_macro" in item["lints"]
                       and item["reason"] for item in overrides):
                errors.append("reasoned local allow was not inventoried")
        if directory.name == "unavailable-selected-lint" and "unsupported selected lint" not in str(clippy):
            errors.append("unsupported selected lint was not explicitly diagnosed")
    elif selected is not None or report["data"]["lint_profile"] is not None:
        errors.append("default compatibility unexpectedly selected a profile")
    return {
        "exit_code": exit_code,
        "profile": selected,
        "clippy_status": clippy["status"],
        "clippy_command": command,
        "baseline_clippy_status": baseline["status"],
        "baseline_clippy_command": baseline_command,
        "diagnostics": diagnostics,
        "policy_passed": policy["passed"],
        "failed_policies": policy["failed_policies"],
        "measurement_confidence": report["measurement_confidence"],
        "toolchain": report["toolchain"],
        "errors": errors,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("rqlens", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--helper-manifest", type=Path, required=True)
    args = parser.parse_args()
    tool_source = args.rqlens.resolve(strict=True)
    helper = args.helper_manifest.resolve(strict=True)
    directory = args.output.resolve()
    directory.mkdir(parents=True, exist_ok=False)
    tool = directory / tool_source.name
    shutil.copy2(tool_source, tool)
    cargo = shutil.which("cargo")
    if not cargo:
        raise RuntimeError("Cargo is required for real consumer probes")
    results = {
        "passed": False,
        "tool_sha256": hashlib.sha256(tool.read_bytes()).hexdigest(),
        "helper_manifest": str(helper),
        "cases": {},
    }
    try:
        if execute([tool, "lint-profiles"], directory, "registry") != 0:
            raise RuntimeError("lint profile registry CLI failed")
        results["registry"] = json.loads((directory / "registry.stdout.log").read_text())
        for name, command in (
            ("rustc-version", ["rustc", "--version", "--verbose"]),
            ("cargo-version", [cargo, "--version"]),
            ("clippy-version", [cargo, "clippy", "--version"]),
        ):
            if execute(command, directory, name) != 0:
                raise RuntimeError(f"required real tool unavailable: {name}")
        for name, profile, source, expected, diagnostic in CASES:
            case = directory / name
            config = setup(case, source, profile, helper, cargo)
            environment = os.environ.copy()
            if name == "unavailable-clippy":
                shim = case / "shim"
                shim.mkdir()
                wrapper = shim / "cargo"
                wrapper.write_text(
                    '#!/bin/sh\nif [ "$1" = clippy ]; then\n'
                    "  echo 'error: no such command: `clippy`' >&2\n  exit 101\nfi\n"
                    f"exec {shlex.quote(cargo)} \"$@\"\n"
                )
                wrapper.chmod(0o755)
                environment["PATH"] = str(shim) + os.pathsep + environment["PATH"]
            elif name == "unavailable-selected-lint":
                driver = shutil.which("clippy-driver")
                if not driver:
                    raise RuntimeError("real clippy-driver needed for inventory filtering probe")
                shim = case / "shim"
                shim.mkdir()
                wrapper = shim / "clippy-driver"
                wrapper.write_text(
                    '#!/bin/sh\nif [ "$1" = -W ] && [ "$2" = help ]; then\n'
                    f"  {shlex.quote(driver)} \"$@\" | grep -v 'clippy::dbg[-_]macro[[:space:]]'\n"
                    f"else\n  exec {shlex.quote(driver)} \"$@\"\nfi\n"
                )
                wrapper.chmod(0o755)
                environment["PATH"] = str(shim) + os.pathsep + environment["PATH"]
            code = execute(
                [tool, "enforce", "--practices-only", "--config", config],
                case, "enforce", environment=environment,
            )
            try:
                result = inspect(case, profile, expected, diagnostic, code)
            except (KeyError, ValueError, OSError, StopIteration) as error:
                result = {"exit_code": code, "errors": [f"evidence inspection: {error}"]}
            result["environment_overrides"] = {
                key: value for key, value in environment.items()
                if value != os.environ.get(key)
            }
            results["cases"][name] = result
            print(name, "PASS" if not result["errors"] else "FAIL", result["errors"], flush=True)
        results["passed"] = all(not case["errors"] for case in results["cases"].values())
    finally:
        (directory / "summary.json").write_text(json.dumps(results, indent=2) + "\n")
    return 0 if results["passed"] else 1


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (OSError, RuntimeError, ValueError, subprocess.TimeoutExpired) as error:
        print(f"RQLens profile probe failed: {error}", file=sys.stderr)
        sys.exit(1)
