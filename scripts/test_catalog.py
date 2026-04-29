import argparse
import json
import re
import subprocess
import sys
import time
from pathlib import Path
from typing import Any, Dict, Iterable, List, Optional

from report_modes import add_mode_argument, emit_report

DEFAULT_OUTPUT = Path("correctness_review.json")
VISIBILITY_OUTPUT = Path("target/analysis/correctness_review.json")
TEST_CATALOG_OUTPUT = Path("target/analysis/test_catalog.json")
DESCRIPTIONS_PATH = Path("scripts/test_descriptions.json")

LAYER_RULES = [
    ("App Shell and State", ("app_state", "app_tests.rs")),
    ("Commands and Transactions", ("commands", "transactions", "transaction_tests.rs")),
    ("Domain Model", ("domain/tab", "domain\\tab", "domain/panes", "tab_tests.rs", "tab_manager_tests.rs")),
    ("Buffer and Text Storage", ("domain/buffer", "buffer_tests.rs", "piece_tree_tests.rs")),
    ("Services and Persistence", ("services", "file_service_tests.rs", "file_controller_tests.rs", "session_store_tests.rs")),
    ("Search", ("search", "search_tests.rs")),
    ("UI and Editor Interaction", ("ui", "editor", "tab_strip", "tab_drag")),
    ("Startup and Settings", ("startup", "settings", "startup_tests.rs", "settings_store_tests.rs")),
]

FILE_DESCRIPTIONS = {
    "app_tests.rs": "Checks app state behavior.",
    "buffer_tests.rs": "Validates buffer metadata.",
    "file_controller_tests.rs": "Covers open/save controller flows.",
    "file_service_tests.rs": "Checks file service IO.",
    "piece_tree_tests.rs": "Validates piece-tree editing behavior.",
    "search_tests.rs": "Verifies search scopes and matches.",
    "session_store_tests.rs": "Checks saved workspace restoration.",
    "settings_store_tests.rs": "Checks settings persistence.",
    "startup_tests.rs": "Validates startup argument parsing.",
    "tab_manager_tests.rs": "Checks shared tab ordering.",
    "tab_tests.rs": "Validates workspace tab behavior.",
    "transaction_tests.rs": "Checks transaction grouping behavior.",
}


def load_descriptions() -> Dict[str, str]:
    if not DESCRIPTIONS_PATH.exists():
        return {}
    try:
        payload = json.loads(DESCRIPTIONS_PATH.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return {}
    return {str(key): trim_description(str(value)) for key, value in payload.items()}


def trim_description(value: str) -> str:
    words = value.strip().split()
    if len(words) <= 9:
        return " ".join(words)
    return " ".join(words[:9]).rstrip(".,;:") + "."


def layer_for_path(path: Path) -> str:
    normalized = path.as_posix()
    for layer, needles in LAYER_RULES:
        if any(needle.replace("\\", "/") in normalized for needle in needles):
            return layer
    return "Domain Model" if normalized.startswith("tests/") else "App Shell and State"


def module_for_path(path: Path) -> str:
    if path.as_posix().startswith("src/"):
        without_prefix = path.with_suffix("").as_posix()[4:]
        if without_prefix.endswith("/mod"):
            without_prefix = without_prefix[:-4]
        return without_prefix.replace("/", "::")
    return path.stem


def title_from_name(name: str) -> str:
    tokens = [token for token in re.split(r"[_\W]+", name) if token]
    useful = [token for token in tokens if token not in {"test", "should", "when"}]
    if not useful:
        useful = tokens or ["behavior"]
    return trim_description(" ".join(["Checks"] + useful[:7]) + ".")


def description_for(path: Path, name: str, overrides: Dict[str, str]) -> str:
    keys = [
        f"{path.as_posix()}::{name}",
        name,
        path.name,
    ]
    for key in keys:
        if key in overrides:
            return overrides[key]
    if path.name in FILE_DESCRIPTIONS:
        return FILE_DESCRIPTIONS[path.name]
    return title_from_name(name)


def iter_rust_files(paths: Iterable[Path]) -> Iterable[Path]:
    for root in paths:
        if not root.exists():
            continue
        if root.is_file() and root.suffix == ".rs":
            yield root
        else:
            yield from root.rglob("*.rs")


def discover_tests() -> List[Dict[str, Any]]:
    overrides = load_descriptions()
    tests: List[Dict[str, Any]] = []
    pattern = re.compile(r"^\s*#\[(?:test|tokio::test|async_std::test)\]\s*(?:\r?\n\s*#\[[^\]]+\]\s*)*\r?\n\s*(?:async\s+)?fn\s+([A-Za-z0-9_]+)", re.MULTILINE)
    for path in sorted(iter_rust_files([Path("tests"), Path("src")])):
        try:
            text = path.read_text(encoding="utf-8")
        except OSError:
            continue
        kind = "integration" if path.as_posix().startswith("tests/") else "inline"
        line_offsets = [0]
        for match in re.finditer(r"\n", text):
            line_offsets.append(match.end())
        for match in pattern.finditer(text):
            name = match.group(1)
            line = 1
            for index, offset in enumerate(line_offsets, start=1):
                if offset > match.start():
                    break
                line = index
            test_id = f"{path.as_posix()}::{name}"
            tests.append(
                {
                    "id": test_id,
                    "name": name,
                    "path": path.as_posix(),
                    "line": line,
                    "layer": layer_for_path(path),
                    "module": module_for_path(path),
                    "description": description_for(path, name, overrides),
                    "kind": kind,
                    "last_status": "unknown",
                    "last_duration": None,
                    "command": f"cargo test {name}",
                }
            )
    return tests


def run_tests() -> Dict[str, Dict[str, Any]]:
    started = time.perf_counter()
    try:
        result = subprocess.run(["cargo", "test"], capture_output=True, text=True)
    except FileNotFoundError:
        return {}
    duration = time.perf_counter() - started
    statuses: Dict[str, Dict[str, Any]] = {}
    for line in result.stdout.splitlines():
        match = re.match(r"test\s+(.+?)\s+\.\.\.\s+(ok|FAILED|ignored)", line.strip())
        if not match:
            continue
        full_name = match.group(1)
        name = full_name.split("::")[-1]
        status = {"ok": "passed", "FAILED": "failed", "ignored": "skipped"}[match.group(2)]
        statuses[name] = {"status": status, "duration": None}
    statuses["__run__"] = {
        "status": "passed" if result.returncode == 0 else "failed",
        "duration": duration,
        "stdout_tail": "\n".join(result.stdout.splitlines()[-40:]),
        "stderr_tail": "\n".join(result.stderr.splitlines()[-40:]),
    }
    return statuses


def build_payload(run: bool = False) -> Dict[str, Any]:
    tests = discover_tests()
    statuses = run_tests() if run else {}
    for item in tests:
        status = statuses.get(item["name"])
        if status:
            item["last_status"] = status["status"]
            item["last_duration"] = status["duration"]

    by_layer: Dict[str, Dict[str, int]] = {}
    for item in tests:
        layer = by_layer.setdefault(
            item["layer"],
            {"total": 0, "passed": 0, "failed": 0, "skipped": 0, "unknown": 0},
        )
        layer["total"] += 1
        layer[item["last_status"]] = layer.get(item["last_status"], 0) + 1

    summary = {
        "test_count": len(tests),
        "integration_count": sum(1 for item in tests if item["kind"] == "integration"),
        "inline_count": sum(1 for item in tests if item["kind"] == "inline"),
        "layers": len(by_layer),
        "failed": sum(1 for item in tests if item["last_status"] == "failed"),
        "unknown": sum(1 for item in tests if item["last_status"] == "unknown"),
        "last_run": statuses.get("__run__"),
    }
    return {
        "version": 1,
        "generated_from": "scripts/test_catalog.py",
        "summary": summary,
        "layers": [
            {
                "name": layer,
                **counts,
                "failed_ratio": (counts["failed"] / counts["total"]) if counts["total"] else 0.0,
            }
            for layer, counts in sorted(by_layer.items(), key=lambda entry: entry[0])
        ],
        "tests": tests,
    }


def render_cli(payload: object) -> str:
    data = payload if isinstance(payload, dict) else {}
    summary = data.get("summary", {})
    lines = [
        "Correctness Review",
        f"Tests: {summary.get('test_count', 0)}",
        f"Layers: {summary.get('layers', 0)}",
        f"Failed: {summary.get('failed', 0)}",
    ]
    return "\n".join(lines)


def main() -> None:
    parser = argparse.ArgumentParser(description="Emit categorized correctness test catalog")
    parser.add_argument("--output", type=Path, default=None)
    parser.add_argument("--run", action="store_true", help="Run cargo test and attach status")
    add_mode_argument(parser)
    args = parser.parse_args()
    payload = build_payload(run=args.run)
    if args.mode == "visibility":
        TEST_CATALOG_OUTPUT.parent.mkdir(parents=True, exist_ok=True)
        TEST_CATALOG_OUTPUT.write_text(json.dumps(payload["tests"], indent=2) + "\n", encoding="utf-8")
    emit_report(
        payload,
        mode=args.mode,
        output_path=args.output,
        visibility_path=VISIBILITY_OUTPUT,
        cli_renderer=render_cli,
        label="correctness review",
    )


if __name__ == "__main__":
    main()
