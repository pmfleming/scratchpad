#!/usr/bin/env python3
"""Probe RQLens's native docs/safety extraction through its public CLI.

Usage: python3 scripts/check-rqlens-native.py RQLENS NEW_OUTPUT_DIRECTORY
       [--helper-manifest FROZEN_HELPER_CARGO_TOML]
The output directory must not exist. Evidence and the frozen executable are kept
there even when an assertion fails. A checkout-built lens can use mutable helper
sources; use --helper-manifest to select a separately frozen helper workspace.
This is a syntax-extraction regression probe,
not an enforcement pass, generated-code inventory, or native Windows test.
"""

import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import sys


DOCS = '''/// A documented type.
pub struct Widget {
    /// A documented field.
    pub value: usize,
    #[doc = ""]
    pub empty: usize,
    #[doc(hidden)]
    pub internal: usize,
}

impl Widget {
    /// A documented method.
    pub fn method(&self) {}
    #[doc = "   "]
    pub fn empty_method(&self) {}
    #[doc(hidden)]
    pub fn hidden_method(&self) {}
}

/// A documented enum.
pub enum Mode {
    /// A documented variant.
    First,
    Second,
    #[doc(hidden)]
    Internal,
}

#[doc = "Attached documentation."]
pub fn attributed() {}
#[doc = ""]
pub fn empty_function() {}
#[doc(hidden)]
pub fn hidden_function() {}
'''

SAFETY = '''fn four_line(pointer: *const u8) -> u8 {
    // SAFETY: the caller supplies a pointer to a live byte.
    // The allocation remains live for this synchronous read.
    // No writes occur concurrently with the read.
    // The reference does not escape this expression.
    unsafe { *pointer } // probe: four_line
}

fn block_comment(pointer: *const u8) -> u8 {
    /* SAFETY: the caller supplies a valid readable byte pointer. */
    unsafe { *pointer } // probe: block_comment
}

fn inline_argument(pointer: *const u8) -> u8 {
    // SAFETY: the caller supplies a live readable byte for this synchronous read.
    std::convert::identity(unsafe { *pointer }) // probe: inline_argument
}

fn substring(pointer: *const u8) -> u8 {
    // UNSAFETY: this substring is not a safety rationale marker.
    unsafe { *pointer } // probe: substring
}

fn string_literal(pointer: *const u8) -> u8 {
    let _text = "SAFETY: words in a string are not a source comment";
    unsafe { *pointer } // probe: string_literal
}

fn empty_rationale(pointer: *const u8) -> u8 {
    // SAFETY:
    unsafe { *pointer } // probe: empty_rationale
}

/// Read the pointed-to byte.
///
/// # Safety
/// The pointer must remain valid and readable for this call.
pub unsafe fn valid_section(pointer: *const u8) -> u8 {
    // SAFETY: the caller guarantees a live readable byte.
    unsafe { *pointer }
}

/// Read a byte; the word Safety alone is not a section.
pub unsafe fn prose_only(pointer: *const u8) -> u8 {
    // SAFETY: the caller guarantees a live readable byte.
    unsafe { *pointer }
}

/// Read a byte.
/// # Safety
pub unsafe fn empty_section(pointer: *const u8) -> u8 {
    // SAFETY: the caller guarantees a live readable byte.
    unsafe { *pointer }
}
'''


def require(condition, message):
    if not condition:
        raise AssertionError(message)


def run(tool, config, directory, metric):
    with (directory / f"{metric}.stdout.log").open("w") as stdout:
        with (directory / f"{metric}.stderr.log").open("w") as stderr:
            result = subprocess.run(
                [str(tool), "measure", metric, "--config", str(config)],
                stdin=subprocess.DEVNULL,
                stdout=stdout,
                stderr=stderr,
                timeout=180,
                check=False,
            )
    require(result.returncode == 0, f"{metric} exited {result.returncode}; see logs")


def inspect(directory):
    artifacts = directory / "fixture" / "artifacts"
    api = json.loads((artifacts / "api_health.json").read_text())
    rows = [row for row in api["records"] if row["path"] == "src/docs.rs"]
    require(len(rows) == 1, f"expected one docs.rs row, got {len(rows)}")
    row = rows[0]
    counts = {
        key: row[key]
        for key in (
            "public_item_count",
            "documented_public_item_count",
            "missing_documentation_count",
        )
    }
    expected = {
        "public_item_count": 10,
        "documented_public_item_count": 6,
        "missing_documentation_count": 4,
    }
    errors = []
    if counts != expected:
        errors.append(f"documentation counts: {counts}; expected {expected}")

    reliability = json.loads((artifacts / "reliability_findings.json").read_text())
    for name, report in (("api-health", api), ("reliability", reliability)):
        confidence = report["measurement_confidence"]
        if confidence.get("complete") is not True or confidence.get("partial") is not False:
            errors.append(f"{name} fixture evidence is not complete: {confidence}")
    records = [row for row in reliability["records"] if row["path"] == "src/safety.rs"]
    unsafe_lines = {
        row["line"]
        for row in records
        if row["rule_id"] == "rust.safety.undocumented-unsafe"
    }
    expected_unsafe = {
        line
        for line, text in enumerate(SAFETY.splitlines(), 1)
        if any(f"// probe: {name}" in text for name in (
            "substring", "string_literal", "empty_rationale"
        ))
    }
    if unsafe_lines != expected_unsafe:
        errors.append(
            f"undocumented unsafe lines: {sorted(unsafe_lines)}; "
            f"expected {sorted(expected_unsafe)}"
        )
    safety_doc_lines = {
        row["line"]
        for row in records
        if row["rule_id"] == "rust.safety.missing-safety-docs"
    }
    expected_docs = {
        line
        for line, text in enumerate(SAFETY.splitlines(), 1)
        if text.startswith(("pub unsafe fn prose_only(", "pub unsafe fn empty_section("))
    }
    if safety_doc_lines != expected_docs:
        errors.append(
            f"missing Safety section lines: {sorted(safety_doc_lines)}; "
            f"expected {sorted(expected_docs)}"
        )
    return {
        "errors": errors,
        "documentation": counts,
        "undocumented_unsafe_lines": sorted(unsafe_lines),
        "missing_safety_section_lines": sorted(safety_doc_lines),
        "api_confidence": api["measurement_confidence"],
        "reliability_confidence": reliability["measurement_confidence"],
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("rqlens", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--helper-manifest", type=Path)
    args = parser.parse_args()
    source = args.rqlens.resolve(strict=True)
    directory = args.output.resolve()
    directory.mkdir(parents=True, exist_ok=False)
    tool = directory / source.name
    shutil.copy2(source, tool)
    identity = {"tool_sha256": hashlib.sha256(tool.read_bytes()).hexdigest()}
    helper = args.helper_manifest.resolve(strict=True) if args.helper_manifest else None
    identity["helper_manifest"] = str(helper) if helper else "lens default (may be mutable)"
    (directory / "identity.json").write_text(json.dumps(identity, indent=2) + "\n")
    fixture = directory / "fixture"
    (fixture / "src").mkdir(parents=True)
    (fixture / "Cargo.toml").write_text(
        '[package]\nname = "rqlens-consumer-native-probe"\n'
        'version = "0.0.0"\nedition = "2024"\nlicense = "MIT"\n'
    )
    (fixture / "src" / "lib.rs").write_text(
        "//! Isolated authored-syntax extraction probe.\nmod docs;\nmod safety;\n"
    )
    (fixture / "src" / "docs.rs").write_text(DOCS)
    (fixture / "src" / "safety.rs").write_text(SAFETY)
    config = fixture / "rqlens.toml"
    config.write_text(
        'project_root = "."\nsource_roots = ["src"]\noutput_dir = "artifacts"\n'
        '[rust]\nidentity_resolution = "auto"\n'
        + (f"helper_manifest = {json.dumps(str(helper))}\n" if helper else "")
    )
    # No policy is relaxed: this independent fixture only measures extraction.
    # Scratchpad policy and measurement configuration are never modified.
    result = {**identity, "passed": False}
    try:
        for metric in ("reliability", "api-health"):
            run(tool, config, directory, metric)
        result.update(inspect(directory))
        require(not result["errors"], "; ".join(result["errors"]))
        result["passed"] = True
    finally:
        (directory / "summary.json").write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    try:
        main()
    except (OSError, AssertionError, KeyError, ValueError, subprocess.TimeoutExpired) as error:
        print(f"native RQLens probe failed: {error}", file=sys.stderr)
        sys.exit(1)
