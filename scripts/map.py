import argparse
import json
import re
import subprocess
import sys
from pathlib import Path
from typing import Dict, List, Optional, Set

HOTSPOT_CMD = [".venv/Scripts/python.exe", "scripts/hotspots.py"]
SLOWSPOT_CMD = [".venv/Scripts/python.exe", "scripts/slowspots.py", "--skip-bench"]
DEFAULT_OUTPUT = Path("map.json")

AREA_COLORS = {
    "chrome": "#569cd6",
    "domain": "#4ec9b0",
    "services": "#d7ba7d",
    "ui": "#c586c0",
    "default": "#808080",
}


def group_id(mod_name: Optional[str]) -> Optional[str]:
    if mod_name is None:
        return None
    return f"group:{mod_name}"


class ArchitectureMapper:
    def __init__(self) -> None:
        self.dependencies: Dict[str, Set[str]] = {}
        self.metrics: Dict[str, Dict] = {}
        self.performance: Dict[str, Dict] = {}
        self.file_to_mod: Dict[str, str] = {}
        self.module_paths: Set[str] = set()

    def extract_dependencies(self, root_dir: str) -> None:
        root = Path(root_dir)
        self._discover_modules(root)
        for file_path, mod_name in self.file_to_mod.items():
            content = Path(file_path).read_text(encoding="utf-8")
            self.dependencies.setdefault(mod_name, set())
            self.dependencies[mod_name].update(self._extract_use_dependencies(content))
            self.dependencies[mod_name].update(
                self._extract_child_modules(content, mod_name)
            )

    def _discover_modules(self, root_dir: Path) -> None:
        for file_path in root_dir.rglob("*.rs"):
            rel_path = file_path.relative_to(root_dir)
            mod_name = rel_path.as_posix().replace("/", "::").replace(".rs", "")
            if mod_name.endswith("::mod"):
                mod_name = mod_name[:-5]

            normalized_path = str(file_path.resolve())
            self.file_to_mod[normalized_path] = mod_name
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

    def gather_metrics(self) -> None:
        try:
            result = subprocess.run(
                HOTSPOT_CMD, capture_output=True, text=True, check=True
            )
            for item in json.loads(result.stdout):
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
                        mod_name, {"score": 0.0, "items": []}
                    )
                    perf_score = self._benchmark_score(item)
                    perf_entry["score"] = max(perf_entry["score"], perf_score)
                    perf_entry["items"].append(item)
        except Exception as exc:
            print(f"Warning: Could not gather performance metrics: {exc}", file=sys.stderr)

    def _benchmark_score(self, item: Dict) -> float:
        return float(item["mean_ns"]) / 100_000.0

    def total_score(self, mod_name: str) -> float:
        return float(self.metrics.get(mod_name, {}).get("score", 0.0)) + float(
            self.performance.get(mod_name, {}).get("score", 0.0)
        )

    def risk_color(self, score: float) -> str:
        if score > 600:
            return "#f44747"
        if score > 300:
            return "#d7ba7d"
        return "#b5cea8"

    def perf_risk_color(self, perf_score: float) -> str:
        if perf_score > 500:
            return "#f44747"
        if perf_score > 200:
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
            total_score = self.total_score(mod_name)
            perf_score = float(perf_data.get("score", 0.0))
            comp_score = float(self.metrics.get(mod_name, {}).get("score", 0.0))

            nodes.append(
                {
                    "data": {
                        "id": mod_name,
                        "label": mod_name.split("::")[-1],
                        "parent": group_id("::".join(mod_name.split("::")[:-1]) or None),
                        "comp_score": comp_score,
                        "perf_score": perf_score,
                        "total_score": total_score,
                        "sloc": int(self.metrics.get(mod_name, {}).get("sloc", 0)),
                        "signals": self.metrics.get(mod_name, {}).get("signals", "stable"),
                        "comp_risk_color": self.risk_color(comp_score),
                        "perf_risk_color": self.perf_risk_color(perf_score),
                        "is_slow": bool(perf_items),
                        "perf_benchmarks": [
                            {
                                "name": item["name"],
                                "mean_ms": float(item["mean_ns"]) / 1_000_000.0,
                                "p95_ms": (
                                    float(item["p95_ns"]) / 1_000_000.0
                                    if item.get("p95_ns") is not None
                                    else None
                                ),
                                "kind": item.get("benchmark_kind", "unmapped"),
                                "threshold_ms": item.get("threshold_ms", 50.0),
                                "signals": item.get("signals", "nominal"),
                            }
                            for item in perf_items
                        ],
                        "perf_kind": ", ".join(
                            sorted({item.get("benchmark_kind", "unmapped") for item in perf_items})
                        ),
                    }
                }
            )

        for source, targets in sorted(self.dependencies.items()):
            for target in sorted(targets):
                if source != target:
                    edges.append({"data": {"source": source, "target": target}})

        return {"nodes": nodes, "edges": edges}

    def viewer_payload(self) -> Dict:
        graph = self.build_graph_payload()
        return {
            "meta": {
                "title": "Scratchpad Architecture Map",
                "generated_from": "scripts/map.py",
                "source_root": "src",
                "node_count": len(graph["nodes"]),
                "edge_count": len(graph["edges"]),
            },
            "graph": graph,
        }


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Emit architecture dependency map data as JSON"
    )
    parser.add_argument(
        "--output",
        type=Path,
        default=None,
        help=f"Optional output JSON path. Example: {DEFAULT_OUTPUT}",
    )
    args = parser.parse_args()

    mapper = ArchitectureMapper()
    mapper.extract_dependencies("src")
    mapper.gather_metrics()
    mapper.gather_performance()

    json_text = json.dumps(mapper.viewer_payload(), indent=2)
    if args.output is None:
        print(json_text)
    else:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json_text + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
