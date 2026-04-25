import argparse
import json
import re
import subprocess
import sys
from collections import defaultdict
from pathlib import Path
from typing import DefaultDict, Dict, Iterable, List, Optional, Sequence, Set, Tuple

from report_modes import add_mode_argument, emit_report

HOTSPOT_CMD = [".venv/Scripts/python.exe", "scripts/hotspots.py"]
SLOWSPOT_CMD = [".venv/Scripts/python.exe", "scripts/slowspots.py", "--skip-bench"]
DEFAULT_OUTPUT = Path("map.json")
VISIBILITY_OUTPUT = Path("target/analysis/map.json")
CORRECTNESS_PATH = Path("target/analysis/correctness_review.json")

AREA_COLORS = {
    "chrome": "#569cd6",
    "domain": "#4ec9b0",
    "services": "#d7ba7d",
    "ui": "#c586c0",
    "default": "#808080",
}

LAYER_ORDER = {
    "chrome": 0,
    "ui": 1,
    "app_state": 2,
    "services": 2,
    "domain": 3,
    "default": 2,
}

DEFECT_KEYWORDS = ("fix", "bug", "regress", "panic", "crash", "issue", "fault")


def group_id(mod_name: Optional[str]) -> Optional[str]:
    if mod_name is None:
        return None
    return f"group:{mod_name}"


class ArchitectureMapper:
    def __init__(self) -> None:
        self.dependencies: Dict[str, Set[str]] = {}
        self.reverse_dependencies: DefaultDict[str, Set[str]] = defaultdict(set)
        self.metrics: Dict[str, Dict] = {}
        self.performance: Dict[str, Dict] = {}
        self.file_to_mod: Dict[str, str] = {}
        self.mod_to_file: Dict[str, str] = {}
        self.module_paths: Set[str] = set()
        self.module_sources: Dict[str, str] = {}
        self.public_api_counts: Dict[str, int] = {}
        self.test_support: Dict[str, Dict[str, object]] = {}
        self.correctness: Dict[str, Dict[str, object]] = {}
        self.git_history: Dict[str, Dict[str, object]] = {}
        self.cycle_members: Set[str] = set()
        self.risk_breakdown: Dict[str, Dict[str, object]] = {}

    def extract_dependencies(self, root_dir: str) -> None:
        root = Path(root_dir)
        self._discover_modules(root)
        for file_path, mod_name in self.file_to_mod.items():
            content = Path(file_path).read_text(encoding="utf-8")
            self.module_sources[mod_name] = content
            self.public_api_counts[mod_name] = self._count_public_api(content)
            self.dependencies.setdefault(mod_name, set())
            self.dependencies[mod_name].update(self._extract_use_dependencies(content))
            self.dependencies[mod_name].update(
                self._extract_child_modules(content, mod_name)
            )

        for source, targets in self.dependencies.items():
            for target in targets:
                self.reverse_dependencies[target].add(source)

        self.cycle_members = self._find_cycle_members()

    def _discover_modules(self, root_dir: Path) -> None:
        for file_path in root_dir.rglob("*.rs"):
            rel_path = file_path.relative_to(root_dir)
            mod_name = rel_path.as_posix().replace("/", "::").replace(".rs", "")
            if mod_name.endswith("::mod"):
                mod_name = mod_name[:-5]

            normalized_path = str(file_path.resolve())
            self.file_to_mod[normalized_path] = mod_name
            self.mod_to_file[mod_name] = normalized_path
            self.module_paths.add(mod_name)
            self.dependencies.setdefault(mod_name, set())

    def _extract_use_dependencies(self, content: str) -> Set[str]:
        dependencies: Set[str] = set()
        use_statements = re.findall(r"^\s*use\s+crate::([^;]+);", content, re.MULTILINE)
        for raw_use in use_statements:
            dependency = self._normalize_use_dependency(raw_use.strip())
            if dependency and dependency in self.module_paths:
                dependencies.add(dependency)
        return dependencies

    def _normalize_use_dependency(self, raw_use: str) -> Optional[str]:
        candidate = raw_use.split(" as ")[0].strip()
        if "::{" in candidate:
            candidate = candidate.split("::{", 1)[0]
        elif "{" in candidate:
            candidate = candidate.split("{", 1)[0].rstrip(":")

        candidate = candidate.split(",")[0].strip().rstrip(":")
        if not candidate:
            return None
        return self._resolve_module_prefix(candidate)

    def _resolve_module_prefix(self, candidate: str) -> Optional[str]:
        parts = candidate.split("::")
        for length in range(len(parts), 0, -1):
            prefix = "::".join(parts[:length])
            if prefix in self.module_paths:
                return prefix
        return None

    def _extract_child_modules(self, content: str, mod_name: str) -> Set[str]:
        children = set()
        declared_mods = re.findall(
            r"^\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+([a-zA-Z0-9_]+)\s*;",
            content,
            re.MULTILINE,
        )
        for child in declared_mods:
            child_mod = f"{mod_name}::{child}"
            if child_mod in self.module_paths:
                children.add(child_mod)
        return children

    def _count_public_api(self, content: str) -> int:
        patterns = [
            r"^\s*pub(?:\([^)]*\))?\s+fn\s+",
            r"^\s*pub(?:\([^)]*\))?\s+struct\s+",
            r"^\s*pub(?:\([^)]*\))?\s+enum\s+",
            r"^\s*pub(?:\([^)]*\))?\s+trait\s+",
            r"^\s*pub(?:\([^)]*\))?\s+mod\s+",
            r"^\s*pub(?:\([^)]*\))?\s+(?:const|static|type)\s+",
        ]
        return sum(
            len(re.findall(pattern, content, re.MULTILINE)) for pattern in patterns
        )

    def gather_metrics(self) -> None:
        from hotspots import HotspotAnalyzer
        from dataclasses import asdict
        try:
            analyzer = HotspotAnalyzer(top=None, scope="all", include_anonymous=False)
            results = analyzer.run(["src"])
            for metric in results:
                item = asdict(metric)
                mod_name = self._metric_module_name(item["name"])
                if mod_name:
                    self.metrics[mod_name] = item
        except Exception as exc:
            print(f"Warning: Could not gather complexity metrics: {exc}", file=sys.stderr)

    def _metric_module_name(self, metric_name: str) -> Optional[str]:
        normalized_name = str(Path(metric_name).resolve())
        if normalized_name in self.file_to_mod:
            return self.file_to_mod[normalized_name]

        metric_path = Path(metric_name)
        try:
            rel_path = metric_path.relative_to("src")
        except ValueError:
            return None

        mod_name = rel_path.as_posix().replace("/", "::").replace(".rs", "")
        if mod_name.endswith("::mod"):
            mod_name = mod_name[:-5]
        if mod_name == "lib":
            return "scratchpad"
        return mod_name

    def gather_performance(self) -> None:
        try:
            result = subprocess.run(
                SLOWSPOT_CMD, capture_output=True, text=True, check=True
            )
            for item in json.loads(result.stdout):
                for mod_name in item.get("targets", []):
                    perf_entry = self.performance.setdefault(
                        mod_name,
                        {
                            "score": 0.0,
                            "mean_ms": 0.0,
                            "variance": 0.0,
                            "items": [],
                        },
                    )
                    perf_score = self._benchmark_score(item)
                    perf_entry["score"] = max(perf_entry["score"], perf_score)
                    perf_entry["mean_ms"] = max(
                        perf_entry["mean_ms"], float(item["mean_ns"]) / 1_000_000.0
                    )
                    perf_entry["variance"] = max(
                        perf_entry["variance"], self._benchmark_variance(item)
                    )
                    perf_entry["items"].append(item)
        except Exception as exc:
            print(f"Warning: Could not gather performance metrics: {exc}", file=sys.stderr)

    def _benchmark_score(self, item: Dict) -> float:
        return float(item["mean_ns"]) / 100_000.0

    def _benchmark_variance(self, item: Dict) -> float:
        mean_ns = float(item.get("mean_ns", 0.0))
        if mean_ns <= 0:
            return 0.0
        return float(item.get("std_dev_ns", 0.0)) / mean_ns

    def gather_test_support(self) -> None:
        test_files = list(Path("tests").rglob("*.rs")) if Path("tests").exists() else []
        test_contents = []
        for path in test_files:
            try:
                test_contents.append((str(path), path.read_text(encoding="utf-8")))
            except OSError:
                continue

        for mod_name, file_path in self.mod_to_file.items():
            source = self.module_sources.get(mod_name, "")
            stem = Path(file_path).stem
            path_hint = mod_name.replace("::", "_")
            has_inline_tests = "#[cfg(test)]" in source or "mod tests" in source
            references: List[str] = []

            for test_path, content in test_contents:
                if (
                    mod_name in content
                    or stem in content
                    or path_hint in Path(test_path).stem
                    or stem in Path(test_path).stem
                ):
                    references.append(test_path)

            self.test_support[mod_name] = {
                "has_inline_tests": has_inline_tests,
                "external_refs": sorted(set(references)),
                "coverage_hint": has_inline_tests or bool(references),
            }

    def gather_correctness(self) -> None:
        if not CORRECTNESS_PATH.exists():
            return
        try:
            payload = json.loads(CORRECTNESS_PATH.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError):
            return

        tests = payload.get("tests", []) if isinstance(payload, dict) else []
        for item in tests:
            module = str(item.get("module", ""))
            if not module:
                continue
            candidates = [
                module,
                module.replace("/", "::"),
                module.replace("\\", "::"),
            ]
            matched = None
            for candidate in candidates:
                if candidate in self.module_paths:
                    matched = candidate
                    break
            if matched is None:
                matched = self._match_test_to_module(str(item.get("path", "")), module)
            if matched is None:
                continue
            entry = self.correctness.setdefault(
                matched,
                {
                    "test_count": 0,
                    "failed_tests": 0,
                    "unknown_tests": 0,
                    "skipped_tests": 0,
                    "tests": [],
                },
            )
            entry["test_count"] = int(entry["test_count"]) + 1
            status = str(item.get("last_status", "unknown"))
            if status == "failed":
                entry["failed_tests"] = int(entry["failed_tests"]) + 1
            elif status == "unknown":
                entry["unknown_tests"] = int(entry["unknown_tests"]) + 1
            elif status == "skipped":
                entry["skipped_tests"] = int(entry["skipped_tests"]) + 1
            tests_list = entry["tests"]
            if isinstance(tests_list, list):
                tests_list.append(
                    {
                        "id": item.get("id"),
                        "name": item.get("name"),
                        "path": item.get("path"),
                        "line": item.get("line"),
                        "status": status,
                        "description": item.get("description"),
                    }
                )

    def _match_test_to_module(self, path_text: str, module: str) -> Optional[str]:
        stem = Path(path_text).stem
        hints = [stem, module.split("::")[-1], module.split("/")[-1]]
        for mod_name in self.module_paths:
            tail = mod_name.split("::")[-1]
            if tail in hints or any(hint and hint in mod_name for hint in hints):
                return mod_name
        return None

    def gather_git_history(self) -> None:
        cmd = [
            "git",
            "log",
            "--format=commit%x09%H%x09%an%x09%s",
            "--numstat",
            "--",
            "src",
            "tests",
        ]
        try:
            result = subprocess.run(
                cmd, capture_output=True, text=True, check=True
            )
        except Exception as exc:
            print(f"Warning: Could not gather git history: {exc}", file=sys.stderr)
            return

        records: DefaultDict[str, Dict[str, object]] = defaultdict(
            lambda: {
                "commits": 0,
                "churn": 0,
                "contributors": set(),
                "defect_commits": 0,
            }
        )

        current_author = ""
        current_subject = ""
        for raw_line in result.stdout.splitlines():
            if raw_line.startswith("commit\t"):
                parts = raw_line.split("\t", 3)
                current_author = parts[2] if len(parts) > 2 else ""
                current_subject = parts[3].lower() if len(parts) > 3 else ""
                continue

            if not raw_line.strip():
                continue

            parts = raw_line.split("\t")
            if len(parts) != 3:
                continue

            added_text, deleted_text, path_text = parts
            if not path_text.endswith(".rs"):
                continue

            resolved_path = str(Path(path_text).resolve())
            mod_name = self.file_to_mod.get(resolved_path)
            if mod_name is None:
                continue

            added = int(added_text) if added_text.isdigit() else 0
            deleted = int(deleted_text) if deleted_text.isdigit() else 0
            record = records[mod_name]
            record["commits"] = int(record["commits"]) + 1
            record["churn"] = int(record["churn"]) + added + deleted
            cast_set = record["contributors"]
            assert isinstance(cast_set, set)
            cast_set.add(current_author)
            if any(keyword in current_subject for keyword in DEFECT_KEYWORDS):
                record["defect_commits"] = int(record["defect_commits"]) + 1

        for mod_name in self.module_paths:
            record = records.get(
                mod_name,
                {
                    "commits": 0,
                    "churn": 0,
                    "contributors": set(),
                    "defect_commits": 0,
                },
            )
            contributors = sorted(record["contributors"]) if isinstance(record["contributors"], set) else []
            self.git_history[mod_name] = {
                "commits": int(record["commits"]),
                "churn": int(record["churn"]),
                "contributors": contributors,
                "contributor_count": len(contributors),
                "defect_commits": int(record["defect_commits"]),
            }

    def _find_cycle_members(self) -> Set[str]:
        visited: Set[str] = set()
        stack: List[str] = []
        on_stack: Set[str] = set()
        cycle_members: Set[str] = set()

        def dfs(node: str) -> None:
            visited.add(node)
            stack.append(node)
            on_stack.add(node)

            for neighbor in sorted(self.dependencies.get(node, set())):
                if neighbor not in visited:
                    dfs(neighbor)
                elif neighbor in on_stack:
                    try:
                        start = stack.index(neighbor)
                    except ValueError:
                        start = 0
                    cycle_members.update(stack[start:])

            stack.pop()
            on_stack.remove(node)

        for mod_name in sorted(self.module_paths):
            if mod_name not in visited:
                dfs(mod_name)
        return cycle_members

    def layer_name(self, mod_name: str) -> str:
        parts = mod_name.split("::")
        if len(parts) > 1 and parts[0] == "app":
            return parts[1]
        return "default"

    def _count_layer_violations(self, mod_name: str) -> int:
        source_layer = self.layer_name(mod_name)
        source_rank = LAYER_ORDER.get(source_layer, LAYER_ORDER["default"])
        violations = 0
        for dependency in self.dependencies.get(mod_name, set()):
            target_layer = self.layer_name(dependency)
            target_rank = LAYER_ORDER.get(target_layer, LAYER_ORDER["default"])
            if source_rank < target_rank:
                violations += 1
        return violations

    def _dependency_density(self, mod_name: str) -> Tuple[int, int]:
        outbound = len(self.dependencies.get(mod_name, set()))
        inbound = len(self.reverse_dependencies.get(mod_name, set()))
        return outbound, inbound

    def compute_risks(self) -> None:
        for mod_name in sorted(self.module_paths):
            metric = self.metrics.get(mod_name, {})
            perf = self.performance.get(mod_name, {})
            git = self.git_history.get(mod_name, {})
            tests = self.test_support.get(mod_name, {})
            correctness = self.correctness.get(mod_name, {})
            outbound, inbound = self._dependency_density(mod_name)
            public_api = self.public_api_counts.get(mod_name, 0)
            sloc = float(metric.get("sloc", 0.0))
            complexity = float(metric.get("score", 0.0))
            churn = float(git.get("churn", 0))
            contributors = int(git.get("contributor_count", 0))
            defect_commits = int(git.get("defect_commits", 0))
            commit_count = int(git.get("commits", 0))
            has_tests = bool(tests.get("coverage_hint", False))
            test_count = int(correctness.get("test_count", 0))
            failed_tests = int(correctness.get("failed_tests", 0))
            unknown_tests = int(correctness.get("unknown_tests", 0))
            skipped_tests = int(correctness.get("skipped_tests", 0))
            has_correctness_tests = has_tests or test_count > 0
            perf_score = float(perf.get("score", 0.0))
            perf_mean_ms = float(perf.get("mean_ms", 0.0))
            perf_variance = float(perf.get("variance", 0.0))
            layer_violations = self._count_layer_violations(mod_name)
            cycle_member = mod_name in self.cycle_members

            maintainability = round(
                complexity
                + min(120.0, sloc * 0.22)
                + min(80.0, public_api * 7.0)
                + min(90.0, outbound * 12.0 + inbound * 10.0),
                2,
            )
            change_risk = round(
                min(160.0, churn / 12.0)
                + min(100.0, commit_count * 2.5)
                + min(80.0, contributors * 14.0)
                + min(90.0, defect_commits * 18.0)
                + (90.0 if not has_correctness_tests else 0.0),
                2,
            )
            correctness_risk = round(
                (140.0 if failed_tests else 0.0)
                + min(120.0, failed_tests * 45.0)
                + min(80.0, unknown_tests * 4.0)
                + min(40.0, skipped_tests * 10.0)
                + (90.0 if not has_correctness_tests else 0.0),
                2,
            )
            performance = round(
                perf_score
                + min(120.0, perf_mean_ms * 2.5)
                + min(90.0, perf_variance * 180.0),
                2,
            )
            architectural = round(
                min(120.0, outbound * 10.0)
                + min(120.0, inbound * 8.0)
                + min(120.0, layer_violations * 32.0)
                + (110.0 if cycle_member else 0.0)
                + (60.0 if sloc >= 250 else 0.0),
                2,
            )
            total = round(
                maintainability + change_risk + performance + architectural + correctness_risk, 2
            )

            signals: Dict[str, List[str]] = {
                "maintainability": [],
                "change": [],
                "performance": [],
                "correctness": [],
                "architectural": [],
            }
            if complexity >= 300:
                signals["maintainability"].append(
                    f"high internal complexity {complexity:.0f}"
                )
            if sloc >= 150:
                signals["maintainability"].append(f"large module {int(sloc)} sloc")
            if public_api >= 5:
                signals["maintainability"].append(f"broad interface {public_api} public items")
            if outbound + inbound >= 8:
                signals["maintainability"].append(
                    f"high coupling in={inbound} out={outbound}"
                )

            if not has_correctness_tests:
                signals["change"].append("low test evidence")
            if churn >= 200:
                signals["change"].append(f"high churn {int(churn)} lines")
            if contributors >= 3:
                signals["change"].append(f"many contributors {contributors}")
            if defect_commits >= 1:
                signals["change"].append(f"defect history {defect_commits} fix commits")

            if perf_mean_ms > 0:
                signals["performance"].append(f"runtime cost {perf_mean_ms:.2f} ms")
            if perf_variance >= 0.15:
                signals["performance"].append(
                    f"instability variance {perf_variance:.2f}"
                )
            if not perf.get("items"):
                signals["performance"].append("no benchmark mapping")

            if failed_tests:
                signals["correctness"].append(f"failing tests {failed_tests}")
            if unknown_tests:
                signals["correctness"].append(f"unknown tests {unknown_tests}")
            if skipped_tests:
                signals["correctness"].append(f"skipped tests {skipped_tests}")
            if not has_correctness_tests:
                signals["correctness"].append("no direct tests")

            if layer_violations >= 1:
                signals["architectural"].append(
                    f"layer violations {layer_violations}"
                )
            if cycle_member:
                signals["architectural"].append("circular dependency")
            if inbound >= 6:
                signals["architectural"].append(f"oversized hub inbound {inbound}")
            if sloc >= 250:
                signals["architectural"].append("oversized module")

            self.risk_breakdown[mod_name] = {
                "maintainability_risk": maintainability,
                "change_risk": change_risk,
                "performance_risk": performance,
                "correctness_risk": correctness_risk,
                "quality_risk": maintainability,
                "architectural_risk": architectural,
                "total_score": total,
                "signals": {key: value or ["stable"] for key, value in signals.items()},
                "evidence": {
                    "complexity_score": complexity,
                    "sloc": int(sloc),
                    "public_api_count": public_api,
                    "outbound_dependencies": outbound,
                    "inbound_dependencies": inbound,
                    "commit_count": commit_count,
                    "churn": int(churn),
                    "contributors": git.get("contributors", []),
                    "contributor_count": contributors,
                    "defect_commits": defect_commits,
                    "has_tests": has_correctness_tests,
                    "test_refs": tests.get("external_refs", []),
                    "test_count": test_count,
                    "failed_tests": failed_tests,
                    "unknown_tests": unknown_tests,
                    "skipped_tests": skipped_tests,
                    "correctness_tests": correctness.get("tests", []),
                    "layer_violations": layer_violations,
                    "cycle_member": cycle_member,
                    "perf_mean_ms": perf_mean_ms,
                    "perf_variance": perf_variance,
                },
            }

    def risk_color(self, score: float) -> str:
        if score >= 700:
            return "#f44747"
        if score >= 350:
            return "#d7ba7d"
        return "#b5cea8"

    def get_group_style(self, mod_name: str) -> Dict[str, str]:
        parts = mod_name.split("::")
        area = parts[1] if len(parts) > 1 and parts[0] == "app" else "default"
        base_color = AREA_COLORS.get(area, AREA_COLORS["default"])
        opacity = max(0.1, 0.4 - (len(parts) * 0.08))
        return {"color": base_color, "opacity": opacity}

    def build_graph_payload(self) -> Dict[str, List[Dict]]:
        nodes: List[Dict] = []
        edges: List[Dict] = []
        groups: Set[str] = set()

        for mod_name in self.dependencies:
            parts = mod_name.split("::")
            for depth in range(1, len(parts)):
                groups.add("::".join(parts[:depth]))

        for group in sorted(groups):
            style = self.get_group_style(group)
            parent = group_id("::".join(group.split("::")[:-1]) or None)
            nodes.append(
                {
                    "data": {
                        "id": group_id(group),
                        "module": group,
                        "label": group.split("::")[-1],
                        "parent": parent,
                        "is_group": True,
                        "bg_color": style["color"],
                        "bg_opacity": style["opacity"],
                    }
                }
            )

        for mod_name in sorted(self.dependencies):
            perf_data = self.performance.get(mod_name, {})
            perf_items = perf_data.get("items", [])
            metric = self.metrics.get(mod_name, {})
            risk = self.risk_breakdown.get(mod_name, {})
            evidence = risk.get("evidence", {})
            category_signals = risk.get("signals", {})

            nodes.append(
                {
                    "data": {
                        "id": mod_name,
                        "layer": self.layer_name(mod_name),
                        "churn": int(evidence.get("churn", 0)),
                        "label": mod_name.split("::")[-1],
                        "parent": group_id("::".join(mod_name.split("::")[:-1]) or None),
                        "comp_score": float(metric.get("score", 0.0)),
                        "perf_score": float(perf_data.get("score", 0.0)),
                        "quality_risk": float(risk.get("quality_risk", risk.get("maintainability_risk", 0.0))),
                        "maintainability_risk": float(
                            risk.get("maintainability_risk", 0.0)
                        ),
                        "correctness_risk": float(risk.get("correctness_risk", 0.0)),
                        "change_risk": float(risk.get("change_risk", 0.0)),
                        "performance_risk": float(risk.get("performance_risk", 0.0)),
                        "architectural_risk": float(
                            risk.get("architectural_risk", 0.0)
                        ),
                        "total_score": float(risk.get("total_score", 0.0)),
                        "sloc": int(metric.get("sloc", 0)),
                        "signals": list(
                            dict.fromkeys(
                                signal
                                for values in category_signals.values()
                                for signal in values
                            )
                        ),
                        "category_signals": category_signals,
                        "risk_colors": {
                            "maintainability": self.risk_color(
                                float(risk.get("maintainability_risk", 0.0))
                            ),
                            "change": self.risk_color(
                                float(risk.get("change_risk", 0.0))
                            ),
                            "performance": self.risk_color(
                                float(risk.get("performance_risk", 0.0))
                            ),
                            "correctness": self.risk_color(
                                float(risk.get("correctness_risk", 0.0))
                            ),
                            "architectural": self.risk_color(
                                float(risk.get("architectural_risk", 0.0))
                            ),
                        },
                        "evidence": evidence,
                        "is_slow": bool(perf_items),
                        "perf_benchmarks": [
                            {
                                "name": item["name"],
                                "mean_ms": float(item["mean_ns"]) / 1_000_000.0,
                                "dispersion_ms": (
                                    float(item["dispersion_ns"]) / 1_000_000.0
                                    if item.get("dispersion_ns") is not None
                                    else None
                                ),
                                "dispersion_label": item.get(
                                    "dispersion_label", "median_abs_dev"
                                ),
                                "kind": item.get("benchmark_kind", "unmapped"),
                                "threshold_ms": item.get("threshold_ms", 50.0),
                                "signals": item.get("signals", "nominal"),
                            }
                            for item in perf_items
                        ],
                        "perf_kind": ", ".join(
                            sorted(
                                {item.get("benchmark_kind", "unmapped") for item in perf_items}
                            )
                        ),
                    }
                }
            )

        for source, targets in sorted(self.dependencies.items()):
            for target in sorted(targets):
                if source != target:
                    edges.append({"data": {"source": source, "target": target}})

        return {"nodes": nodes, "edges": edges}

    def meta_summary(self) -> Dict[str, object]:
        measured_modules = len(self.risk_breakdown)
        category_totals = {
            "maintainability": round(
                sum(
                    item["maintainability_risk"]
                    for item in self.risk_breakdown.values()
                ),
                2,
            ),
            "change": round(
                sum(item["change_risk"] for item in self.risk_breakdown.values()), 2
            ),
            "performance": round(
                sum(
                    item["performance_risk"] for item in self.risk_breakdown.values()
                ),
                2,
            ),
            "correctness": round(
                sum(
                    item["correctness_risk"] for item in self.risk_breakdown.values()
                ),
                2,
            ),
            "architectural": round(
                sum(
                    item["architectural_risk"] for item in self.risk_breakdown.values()
                ),
                2,
            ),
        }
        return {
            "measured_modules": measured_modules,
            "cycle_members": len(self.cycle_members),
            "modules_without_test_evidence": sum(
                1
                for item in self.risk_breakdown.values()
                if not bool(item["evidence"]["has_tests"])
            ),
            "category_totals": category_totals,
        }

    def viewer_payload(self) -> Dict:
        graph = self.build_graph_payload()
        return {
            "meta": {
                "title": "Scratchpad Architecture Risk Map",
                "generated_from": "scripts/map.py",
                "source_root": "src",
                "node_count": len(graph["nodes"]),
                "edge_count": len(graph["edges"]),
                "risk_model": [
                    "maintainability",
                    "change",
                "performance",
                "correctness",
                "architectural",
                ],
                "summary": self.meta_summary(),
            },
            "graph": graph,
        }


def refresh_analysis_inputs() -> None:
    commands = [
        HOTSPOT_CMD + ["--mode", "analysis", "--paths", "src"],
        SLOWSPOT_CMD + ["--mode", "analysis"],
    ]
    for command in commands:
        subprocess.run(command, check=True, capture_output=True, text=True)


def render_cli(payload: object) -> str:
    data = payload if isinstance(payload, dict) else {}
    nodes = data.get("graph", {}).get("nodes", [])
    modules = [
        node.get("data", {})
        for node in nodes
        if not node.get("data", {}).get("is_group")
    ]
    top = sorted(modules, key=lambda item: -float(item.get("total_score", 0.0)))[:10]
    lines = ["Architecture Risk Map"]
    for index, item in enumerate(top, start=1):
        lines.append(
            f"{index:>2}. {item.get('id', '<unknown>')} | total={float(item.get('total_score', 0.0)):.2f} | maintainability={float(item.get('maintainability_risk', 0.0)):.2f} | change={float(item.get('change_risk', 0.0)):.2f} | architectural={float(item.get('architectural_risk', 0.0)):.2f}"
        )
    if not top:
        lines.append("No modules found.")
    return "\n".join(lines)


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Emit architecture dependency and risk map data as JSON"
    )
    parser.add_argument(
        "--output",
        type=Path,
        default=None,
        help=f"Optional output JSON path. Example: {DEFAULT_OUTPUT}",
    )
    parser.add_argument(
        "--refresh",
        action="store_true",
        help="Regenerate hotspot and slowspot inputs before building the map.",
    )
    add_mode_argument(parser)
    args = parser.parse_args()

    if args.refresh:
        refresh_analysis_inputs()

    mapper = ArchitectureMapper()
    mapper.extract_dependencies("src")
    mapper.gather_metrics()
    mapper.gather_performance()
    mapper.gather_test_support()
    mapper.gather_correctness()
    mapper.gather_git_history()
    mapper.compute_risks()

    payload = mapper.viewer_payload()
    emit_report(
        payload,
        mode=args.mode,
        output_path=args.output,
        visibility_path=VISIBILITY_OUTPUT,
        cli_renderer=render_cli,
        label="map",
    )


if __name__ == "__main__":
    main()
