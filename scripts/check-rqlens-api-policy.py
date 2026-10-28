#!/usr/bin/env python3
"""Probe stable API-policy rules through real measurements, policy and SARIF.

A new output directory and independently frozen executable/helper workspace are
required. Expected violations and abstentions are assertions, not test failures.
No report is fabricated and no production threshold or exclusion is changed.
"""

import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shlex
import shutil
import subprocess
import sys


spec = importlib.util.spec_from_file_location(
    "rqlens_profile_probe", Path(__file__).with_name("check-rqlens-profiles.py")
)
profiles = importlib.util.module_from_spec(spec)
spec.loader.exec_module(profiles)

# Stable public IDs are checked against the frozen blocking-rule registry.
RULES = {
    "docs": "rust.api.public-docs",
    "debug": "rust.api.public-debug",
    "must_use": "rust.api.must-use-candidate",
    "must_use_self": "rust.api.must-use-self",
    "imports": "rust.imports.wildcard",
}
DIAGNOSTICS = {
    "debug": "missing_debug_implementations",
    "must_use": "clippy::must_use_candidate",
    "must_use_self": "clippy::return_self_not_must_use",
    "imports": "clippy::wildcard_imports",
}

DOC_BAD = '''//! Native documentation policy fixture.

pub fn answer() {}
'''
DOC_GOOD = DOC_BAD.replace("pub fn answer()", "/// Perform the fixture operation.\npub fn answer()")
CRATE_BAD = "/// Perform the fixture operation.\npub fn answer() {}\n"
CRATE_GOOD = "//! Documented Cargo library root.\n\n" + CRATE_BAD
MEMBERS_BAD = '''//! Native public member documentation fixture.

/// A public record.
pub struct Record {
    pub value: u8,
}

impl Record {
    pub fn visit(&self) {}
}

/// A fixture state.
pub enum State {
    Ready,
}
'''
MEMBERS_GOOD = MEMBERS_BAD.replace(
    "    pub value:", "    /// Stored fixture value.\n    pub value:"
).replace(
    "    pub fn visit", "    /// Visit the fixture record.\n    pub fn visit"
).replace("    Ready,", "    /// The fixture is ready.\n    Ready,")
HIDDEN = '''//! Explicitly hidden and test-only documentation scope.

#[doc(hidden)]
pub fn hidden() {}

/// A visible record with hidden members.
pub struct Record {
    #[doc(hidden)]
    pub hidden: u8,
}

impl Record {
    #[doc(hidden)]
    pub fn hidden_method(&self) {}
}

#[cfg(test)]
mod tests {
    pub fn test_helper() {}

    #[test]
    fn smoke() {
        test_helper();
    }
}
'''
DEBUG_BAD = profiles.NO_DEBUG
DEBUG_GOOD = DEBUG_BAD.replace("pub struct Ticket;", "#[derive(Debug)]\npub struct Ticket;")
MUST_USE_BAD = profiles.GOOGLE_FAILURE
MUST_USE_GOOD = profiles.CLEAN
SELF_BAD = '''//! Self-returning method policy fixture.

/// A fixture token.
#[derive(Debug)]
pub struct Ticket;

impl Ticket {
    /// Return ownership of the fixture token.
    pub fn renewed(self) -> Self {
        self
    }
}
'''
SELF_GOOD = SELF_BAD.replace("    pub fn renewed", "    #[must_use]\n    pub fn renewed")
IMPORT_BAD = '''//! Compiler-backed import-policy fixture.

mod helpers {
    pub const ANSWER: u8 = 42;
}

use helpers::*;

/// Return the fixture value.
#[must_use]
pub fn answer() -> u8 {
    ANSWER
}
'''
IMPORT_GOOD = IMPORT_BAD.replace("use helpers::*;", "use helpers::ANSWER;")
PRELUDE_GLOB = IMPORT_BAD.replace("helpers", "prelude")
PUBLIC_REEXPORT = IMPORT_BAD.replace("use helpers::*;", "pub use helpers::*;")
TEST_GLOB = profiles.CLEAN + '''
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn answer_is_available() {
        assert_eq!(answer(), 42);
    }
}
'''
UNSUPPORTED_DOC = '''//! Nonliteral documentation must abstain in the native backend.

#[doc = include_str!("../details.md")]
pub fn answer() {}
'''


def require(condition, message):
    if not condition:
        raise AssertionError(message)


def fixture(root, name, source, rule, profile, helper, cargo):
    directory = root / name
    config = profiles.setup(directory, source, profile, helper, cargo)
    contents = config.read_text().split('[policy.rules.')[0]
    contents += f'[policy.rules.{json.dumps(RULES[rule])}]\nlevel = "error"\nmax = 0\n'
    if source == UNSUPPORTED_DOC:
        (directory / "details.md").write_text("Actual included documentation.\n")
        contents = contents.replace('[rust]\n', 'fingerprint_inputs = ["details.md"]\n[rust]\n')
    config.write_text(contents)
    return directory, config


def measure(tool, directory, config, rule, name, environment=None):
    producer = "api-health" if rule == "docs" else "practices"
    code = profiles.execute(
        [tool, "measure", producer, "--config", config], directory,
        f"{name}-measure", environment=environment,
    )
    require(code == 0, f"{name}: producer exit {code}; inspect retained logs")


def evaluate(tool, directory, config, rule, name, expected, environment=None, sarif=True,
             expected_path="src/lib.rs"):
    code = profiles.execute(
        [tool, "check", "--config", config], directory, f"{name}-check",
        environment=environment,
    )
    report = json.loads((directory / "artifacts/policy_report.json").read_text())
    evaluation = next(item for item in report["policy_rule_evaluations"] if item["rule_id"] == RULES[rule])
    require((code == 0) == (expected == 0), f"{name}: unexpected check exit {code}: {evaluation}")
    require(report["passed"] == (expected == 0), f"{name}: incorrect policy pass: {evaluation}")
    # A failing compiler invocation retains real positives but cannot establish
    # complete absence across its remaining targets/packages/lints. Native
    # literal documentation can have complete positive coverage independently.
    if expected is None or (rule != "docs" and expected > 0):
        require(evaluation["status"] == "incomplete", f"{name}: unavailable evidence not incomplete: {evaluation}")
        require(evaluation["observed"] is None and evaluation["evaluation_complete"] is False,
                f"{name}: unknown scope became a numeric zero: {evaluation}")
        if expected is not None:
            require(evaluation["observed_findings"] == expected,
                    f"{name}: observed compiler positives disagree with policy: {evaluation}")
    else:
        require(evaluation["evaluation_complete"] is True, f"{name}: expected complete evidence: {evaluation}")
        require(evaluation["observed"] == expected, f"{name}: count mismatch: {evaluation}")
    coverage = report["api_rule_coverage"][RULES[rule]]
    require(coverage["evaluation_complete"] == evaluation["evaluation_complete"],
            f"{name}: policy coverage view contradicts actual evaluation: {coverage}, {evaluation}")
    result = {"check_exit": code, "evaluation": evaluation, "passed": report["passed"],
              "coverage": coverage}
    policy_findings = [item for item in report["api_findings"] if item["rule_id"] == RULES[rule]]
    if rule != "docs" and expected is not None:
        practices = json.loads((directory / "artifacts/rust_practices.json").read_text())
        selected = next(item for item in practices["data"]["checks"]
                        if item["rule_id"] == "rust.official.clippy-profile")
        require(selected["status"] == ("passed" if expected == 0 else "failed"),
                f"{name}: policy does not reflect real selected execution")
        require(selected["lint_profile"]["execution_attempted"] is True,
                f"{name}: no selected compiler command executed")
        result["compiled_scope"] = selected["lint_profile"]["scope"]
    if rule == "docs" and expected is not None:
        native = json.loads((directory / "artifacts/api_health.json").read_text())
        native_coverage = native["api_policy_coverage"]
        require(native_coverage["counting_unit"] == "source-file-with-missing-authored-public-documentation",
                f"{name}: native policy counting unit changed")
        missing = sum(item["missing_public_item_count"] for item in native_coverage["files"]
                      if item["status"] == "complete")
        positive_items = {"member-docs-transition": 3, "crate-docs-transition": 0}.get(directory.name, 1)
        expected_missing = positive_items if expected else 0
        missing_crates = sum(item["missing_crate_documentation"] for item in native_coverage["files"]
                             if item["status"] == "complete")
        require(missing_crates == int(bool(expected) and directory.name == "crate-docs-transition"),
                f"{name}: actual Cargo crate-root documentation scope disagrees")
        require(missing == expected_missing, f"{name}: member documentation count {missing} != {expected_missing}")
        result["native_coverage"] = native_coverage
    (directory / f"{name}-policy.json").write_text(json.dumps(report, indent=2) + "\n")
    if sarif:
        output = directory / f"{name}.sarif"
        sarif_code = profiles.execute(
            [tool, "sarif", "--config", config, "--output", output], directory,
            f"{name}-sarif", environment=environment,
        )
        require(sarif_code == 0, f"{name}: SARIF exit {sarif_code}; inspect logs")
        document = json.loads(output.read_text())
        rows = [item for run in document["runs"] for item in run["results"]
                if item["ruleId"] == RULES[rule]]
        require(document["runs"][0]["properties"]["api_rule_coverage"][RULES[rule]] == coverage,
                f"{name}: policy/SARIF coverage disagreement")
        require(sorted(item["id"] for item in policy_findings) == sorted(
            row["properties"]["rqlens_finding"]["id"] for row in rows),
            f"{name}: policy/SARIF stable finding identities disagree")
        if expected is not None:
            require(len(rows) == expected, f"{name}: policy/SARIF count disagreement: {len(rows)} != {expected}")
            for row in rows:
                finding = row["properties"]["rqlens_finding"]
                uri = row["locations"][0]["physicalLocation"]["artifactLocation"]["uri"]
                require(finding["rule_id"] == RULES[rule] and finding["path"] == uri,
                        f"{name}: stable ID/location disagreement: {row}")
                require(uri == expected_path, f"{name}: finding escaped expected source scope: {uri}")
                location = row["locations"][0]["physicalLocation"]
                if rule == "docs":
                    require("region" not in location, f"{name}: file-level docs fabricated an item location")
                else:
                    require(finding["scope"] == "recorded-compiled-profile-scope",
                            f"{name}: compiled scope missing")
                    require(finding["package_name"] == "rqlens-profile-consumer",
                            f"{name}: compiler package scope lost")
                    require(finding["original_rule_id"] != RULES[rule] and finding["original_diagnostic"],
                            f"{name}: original compiler identity lost")
                    require(finding["original_diagnostic"]["code"] == DIAGNOSTICS[rule],
                            f"{name}: stable rule mapped the wrong compiler diagnostic")
        result["sarif_results"] = rows
    return result


def transition(tool, root, name, rule, profile, bad, good, count, helper, cargo):
    directory, config = fixture(root, name, bad, rule, profile, helper, cargo)
    measure(tool, directory, config, rule, "positive")
    result = {"positive": evaluate(tool, directory, config, rule, "positive", count)}
    (directory / "src/lib.rs").write_text(good)
    require(profiles.execute([cargo, "fmt", "--manifest-path", directory / "Cargo.toml"],
                             directory, "repair-format") == 0, "repair formatting failed")
    result["stale"] = evaluate(tool, directory, config, rule, "stale", None, sarif=False)
    measure(tool, directory, config, rule, "repaired")
    result["repaired"] = evaluate(tool, directory, config, rule, "repaired", 0)
    with (directory / "src/lib.rs").open("a") as source:
        source.write("\n// Freshness sentinel: meaningful evidence must be regenerated.\n")
    result["stale_negative"] = evaluate(tool, directory, config, rule, "stale-negative", None, sarif=False)
    measure(tool, directory, config, rule, "refreshed")
    result["refreshed"] = evaluate(tool, directory, config, rule, "refreshed", 0)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("rqlens", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--helper-manifest", type=Path, required=True)
    parser.add_argument("--blocking-rules", type=Path, required=True)
    args = parser.parse_args()
    source = args.rqlens.resolve(strict=True)
    helper = args.helper_manifest.resolve(strict=True)
    contract = json.loads(args.blocking_rules.read_text())
    require(all(rule in contract["rules"] for rule in RULES.values()), "stable API rule IDs absent from frozen registry")
    require(all(contract["profile_rules"][RULES[key]]["diagnostic_code"] == code
                for key, code in DIAGNOSTICS.items()), "stable API diagnostic mappings changed")
    root = args.output.resolve()
    root.mkdir(parents=True, exist_ok=False)
    tool = root / source.name
    shutil.copy2(source, tool)
    cargo = shutil.which("cargo")
    require(cargo is not None, "Cargo required")
    results = {"passed": False, "tool_sha256": hashlib.sha256(tool.read_bytes()).hexdigest(),
               "helper_manifest": str(helper), "rules": RULES, "cases": {}}

    def run_case(name, action):
        try:
            results["cases"][name] = {"passed": True, "evidence": action()}
        except (OSError, ValueError, KeyError, StopIteration, AssertionError,
                RuntimeError, subprocess.TimeoutExpired) as error:
            results["cases"][name] = {"passed": False, "error": str(error)}
        case = results["cases"][name]
        print(name, "PASS" if case["passed"] else "FAIL", case.get("error", ""), flush=True)

    try:
        for name, rule, profile, bad, good, count in (
            ("docs-transition", "docs", None, DOC_BAD, DOC_GOOD, 1),
            ("crate-docs-transition", "docs", None, CRATE_BAD, CRATE_GOOD, 1),
            ("member-docs-transition", "docs", None, MEMBERS_BAD, MEMBERS_GOOD, 1),
            ("debug-transition", "debug", "library-v1", DEBUG_BAD, DEBUG_GOOD, 1),
            ("must-use-transition", "must_use", "library-v1", MUST_USE_BAD, MUST_USE_GOOD, 1),
            ("must-use-self-transition", "must_use_self", "library-v1", SELF_BAD, SELF_GOOD, 1),
            ("imports-transition", "imports", "google-v1", IMPORT_BAD, IMPORT_GOOD, 1),
        ):
            run_case(name, lambda name=name, rule=rule, profile=profile, bad=bad, good=good, count=count:
                     transition(tool, root, name, rule, profile, bad, good, count, helper, cargo))

        def shared_mount(name, attribute):
            source = (
                '//! Shared physical source with distinct mount contexts.\n\n'
                + attribute + '\n#[path = "shared.rs"]\nmod restricted_mount;\n\n'
                '/// A visible production mount.\n#[path = "shared.rs"]\npub mod production_mount;\n'
            )
            # Set up Cargo before rustfmt resolves these additional module files.
            directory, config = fixture(root, name, "//! Shared mount fixture setup.\n",
                                        "docs", None, helper, cargo)
            (directory / "src/lib.rs").write_text(source)
            shared = directory / "src/shared.rs"
            shared.write_text("pub fn answer() {}\n")
            require(profiles.execute([cargo, "fmt", "--manifest-path", directory / "Cargo.toml"],
                                     directory, "mount-format") == 0, "mount formatting failed")
            measure(tool, directory, config, "docs", "positive")
            def mount_expectation(positive):
                document = json.loads((directory / "artifacts/api_health.json").read_text())
                scope = next(item for item in document["api_policy_coverage"]["files"]
                             if item["path"] == "src/shared.rs")
                if scope["status"] == "complete":
                    require(not scope["ambiguous_mount"] and not scope["unknown_ancestry_or_scope"],
                            "complete shared-file result contradicts ownership uncertainty")
                    return (1 if positive else 0), scope
                require(scope["status"] == "incomplete" and
                        (scope["ambiguous_mount"] or scope["unknown_ancestry_or_scope"]),
                        f"shared production mount incorrectly discarded: {scope}")
                return None, scope

            # A known production mount must report the real violation. An
            # unresolved ownership join must abstain, never become a clean zero.
            expected, scope = mount_expectation(True)
            result = {"positive_scope": scope,
                      "positive": evaluate(tool, directory, config, "docs", "positive", expected,
                                           expected_path="src/shared.rs")}
            shared.write_text("/// Perform the shared operation.\npub fn answer() {}\n")
            result["stale"] = evaluate(tool, directory, config, "docs", "stale", None, sarif=False)
            measure(tool, directory, config, "docs", "repaired")
            expected, scope = mount_expectation(False)
            result["repaired_scope"] = scope
            result["repaired"] = evaluate(tool, directory, config, "docs", "repaired", expected)
            return result

        for name, attribute in (("shared-hidden-production-mount", "#[doc(hidden)]"),
                                ("shared-test-production-mount", "#[cfg(test)]")):
            run_case(name, lambda name=name, attribute=attribute: shared_mount(name, attribute))

        def simple(name, rule, profile, source, expected, producer=True, extra=None):
            directory, config = fixture(root, name, source, rule, profile, helper, cargo)
            environment = None
            if extra:
                environment = extra(directory)
            if producer:
                measure(tool, directory, config, rule, "observed", environment)
            return evaluate(tool, directory, config, rule, "observed", expected, environment)

        for name, rule, profile, source, expected, producer in (
            ("hidden-and-test-docs", "docs", None, HIDDEN, 0, True),
            ("unsupported-native-docs", "docs", None, UNSUPPORTED_DOC, None, True),
            ("missing-native-docs", "docs", None, DOC_GOOD, None, False),
            ("missing-compiler-evidence", "debug", "library-v1", DEBUG_GOOD, None, False),
            ("no-profile-is-not-clean", "debug", None, DEBUG_GOOD, None, True),
            ("no-profile-must-use-is-not-clean", "must_use", None, MUST_USE_GOOD, None, True),
            ("no-profile-self-is-not-clean", "must_use_self", None, SELF_GOOD, None, True),
            ("no-profile-imports-is-not-clean", "imports", None, IMPORT_GOOD, None, True),
            ("wrong-profile-is-not-clean", "debug", "application-v1", DEBUG_GOOD, None, True),
            ("inline-test-glob-exception", "imports", "google-v1", TEST_GLOB, 0, True),
            ("enum-glob-exception", "imports", "google-v1", profiles.ENUM_GLOB, 0, True),
            ("prelude-glob-exception", "imports", "google-v1", PRELUDE_GLOB, 0, True),
            ("public-reexport-exception", "imports", "google-v1", PUBLIC_REEXPORT, 0, True),
        ):
            run_case(name, lambda name=name, rule=rule, profile=profile, source=source, expected=expected, producer=producer:
                     simple(name, rule, profile, source, expected, producer))

        def binary_scope(directory):
            (directory / "src/main.rs").write_text(
                "pub struct BinaryOnlyType;\n\nfn main() {\n    let _ = BinaryOnlyType;\n}\n"
            )
            require(profiles.execute([cargo, "fmt", "--manifest-path", directory / "Cargo.toml"],
                                     directory, "binary-format") == 0, "binary formatting failed")
            return None

        run_case("debug-library-target-scope", lambda: simple(
            "debug-library-target-scope", "debug", "library-v1", profiles.CLEAN, 0, extra=binary_scope))

        def partial_attribution():
            # A real unavailable Cargo graph, not a hand-edited clean report.
            directory, config = fixture(root, "partial-target-attribution", CRATE_BAD,
                                        "docs", None, helper, cargo)
            (directory / "api").mkdir()
            shutil.move(directory / "src/lib.rs", directory / "api/entry.rs")
            (directory / "src/main.rs").write_text("//! Documented binary root.\nfn main() {}\n")
            manifest = directory / "Cargo.toml"
            with manifest.open("a") as source:
                source.write('[lib]\npath = "api/entry.rs"\n[dependencies]\n'
                             'unavailable = { path = "../missing-dependency" }\n')
            config.write_text(config.read_text().replace('source_roots = ["src"]',
                                                        'source_roots = ["src", "api"]'))
            measure(tool, directory, config, "docs", "observed")
            result = evaluate(tool, directory, config, "docs", "observed", None, sarif=False)
            code = profiles.execute([tool, "sarif", "--config", config], directory, "unsupported-sarif")
            require(code != 0, "incomplete source provenance silently exported as current SARIF")
            result["sarif_exit"] = code
            return result

        run_case("partial-target-attribution", partial_attribution)

        def unavailable(directory):
            shim = directory / "shim"
            shim.mkdir()
            wrapper = shim / "cargo"
            wrapper.write_text('#!/bin/sh\nif [ "$1" = clippy ]; then\n'
                               "  echo 'error: no such command: `clippy`' >&2\n  exit 101\nfi\n"
                               f"exec {shlex.quote(cargo)} \"$@\"\n")
            wrapper.chmod(0o755)
            environment = os.environ.copy()
            environment["PATH"] = str(shim) + os.pathsep + environment["PATH"]
            return environment

        run_case("unavailable-compiler-backend", lambda: simple(
            "unavailable-compiler-backend", "debug", "library-v1", DEBUG_GOOD, None, extra=unavailable))
        results["passed"] = all(case["passed"] for case in results["cases"].values())
    finally:
        (root / "summary.json").write_text(json.dumps(results, indent=2) + "\n")
    return 0 if results["passed"] else 1


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (OSError, ValueError, KeyError, AssertionError, RuntimeError, subprocess.TimeoutExpired) as error:
        print(f"RQLens API-policy probe failed: {error}", file=sys.stderr)
        sys.exit(1)
