import argparse
import json
import os
import shutil
import subprocess
import sys
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Dict, List, Optional


DEFAULT_OUTPUT = Path("hotspots.json")


@dataclass
class CodeMetrics:
    name: str
    kind: str
    start_line: int
    end_line: int
    cognitive: float
    cyclomatic: float
    mi: float
    effort: float
    sloc: float
    ploc: float
    cloc: float
    bugs: float
    abc_mag: float
    nom_fn: int
    nom_cl: int
    score: float = 0.0
    signals: str = ""
    abc_density: float = 0.0
    complexity_density: float = 0.0

    @property
    def short_name(self) -> str:
        if self.kind == "unit":
            return os.path.basename(self.name)
        if "::" in self.name:
            return self.name.split("::")[-1]
        return os.path.basename(self.name) if os.sep in self.name else self.name


class HotspotAnalyzer:
    def __init__(self, top: Optional[int], scope: str, include_anonymous: bool):
        self.top = top
        self.scope = scope
        self.include_anonymous = include_anonymous

    def _extract_metric(
        self, metrics: Dict, group: str, name: str, default: float = 0.0
    ) -> float:
        return float(metrics.get(group, {}).get(name, default))

    def process_node(self, node: Dict) -> CodeMetrics:
        m = node.get("metrics", {})
        metrics = CodeMetrics(
            name=node.get("name", "unknown"),
            kind=node.get("kind", "unknown"),
            start_line=node.get("start_line", 0),
            end_line=node.get("end_line", 0),
            cognitive=self._extract_metric(m, "cognitive", "sum"),
            cyclomatic=self._extract_metric(m, "cyclomatic", "sum"),
            mi=self._extract_metric(m, "mi", "mi_visual_studio", 100.0),
            effort=self._extract_metric(m, "halstead", "effort"),
            sloc=self._extract_metric(m, "loc", "sloc"),
            ploc=self._extract_metric(m, "loc", "ploc"),
            cloc=self._extract_metric(m, "loc", "cloc"),
            bugs=self._extract_metric(m, "halstead", "bugs"),
            abc_mag=self._extract_metric(m, "abc", "magnitude"),
            nom_fn=int(self._extract_metric(m, "nom", "functions")),
            nom_cl=int(self._extract_metric(m, "nom", "closures")),
        )
        metrics.score = self.calculate_score(metrics)
        metrics.signals = self.generate_signals(metrics)

        if metrics.sloc > 0:
            metrics.abc_density = metrics.abc_mag / metrics.sloc
            metrics.complexity_density = metrics.score / metrics.sloc

        return metrics

    def calculate_score(self, m: CodeMetrics) -> float:
        score = (m.cognitive * 4.0) + (m.cyclomatic * 2.5)
        score += max(0.0, 70.0 - m.mi) * 1.5
        score += min(30.0, m.effort / 1000.0)
        score += min(20.0, m.sloc / 10.0)
        return round(score, 2)

    def generate_signals(self, m: CodeMetrics) -> str:
        signals = []
        if m.cognitive >= 8:
            signals.append(f"high cognitive={m.cognitive}")
        if m.cyclomatic >= 12:
            signals.append(f"high cyclomatic={m.cyclomatic}")
        if m.mi < 40:
            signals.append(f"low MI={m.mi:.1f}")
        if m.effort >= 15000:
            signals.append(f"high effort={m.effort:,.0f}")
        if m.sloc >= 150:
            signals.append(f"large sloc={m.sloc}")
        return ", ".join(signals) if signals else "stable"

    def flatten(self, node: Dict, results: List[CodeMetrics]) -> None:
        results.append(self.process_node(node))
        for child in node.get("spaces", []):
            self.flatten(child, results)

    def run(self, paths: List[str]) -> List[CodeMetrics]:
        if not shutil.which("rust-code-analysis-cli"):
            print("Error: 'rust-code-analysis-cli' not found in PATH.", file=sys.stderr)
            sys.exit(1)

        cmd = [
            "rust-code-analysis-cli",
            "--metrics",
            "--paths",
            *paths,
            "--output-format",
            "json",
        ]
        try:
            result = subprocess.run(cmd, capture_output=True, text=True, check=True)
        except Exception as exc:
            print(f"Analysis failed: {exc}", file=sys.stderr)
            sys.exit(1)

        all_data = []
        for line in result.stdout.splitlines():
            if not line.strip():
                continue
            clean_line = line.replace('"N1":', '"halstead_N1":').replace(
                '"N2":', '"halstead_N2":'
            )
            try:
                self.flatten(json.loads(clean_line), all_data)
            except json.JSONDecodeError:
                print(
                    f"Warning: Skipping malformed CLI output line: {clean_line[:50]}...",
                    file=sys.stderr,
                )

        scope_map = {"files": ["unit"], "functions": ["function"]}
        target_kinds = scope_map.get(self.scope, ["unit", "function"])
        filtered = [m for m in all_data if m.kind in target_kinds]
        if not self.include_anonymous:
            filtered = [m for m in filtered if m.name != "<anonymous>"]

        ranked = sorted(filtered, key=lambda item: (-item.score, item.name))
        if self.top is not None:
            return ranked[: self.top]
        return ranked


def write_json(payload: object, output_path: Optional[Path]) -> None:
    json_text = json.dumps(payload, indent=2)
    if output_path is None:
        print(json_text)
        return
    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text(json_text + "\n", encoding="utf-8")


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Emit Rust complexity hotspot metrics as JSON"
    )
    parser.add_argument("--paths", nargs="+", default=["src"], help="Paths to analyze")
    parser.add_argument(
        "--top",
        type=int,
        default=None,
        help="Limit the number of ranked hotspot records. Defaults to all records.",
    )
    parser.add_argument(
        "--scope",
        choices=["all", "files", "functions"],
        default="all",
        help="Metric scope to include in the JSON output",
    )
    parser.add_argument(
        "--include-anonymous", action="store_true", help="Include anonymous functions"
    )
    parser.add_argument(
        "--output",
        type=Path,
        default=None,
        help=f"Optional output JSON path. Example: {DEFAULT_OUTPUT}",
    )

    args = parser.parse_args()
    analyzer = HotspotAnalyzer(
        top=args.top, scope=args.scope, include_anonymous=args.include_anonymous
    )
    payload = [asdict(metric) for metric in analyzer.run(args.paths)]
    write_json(payload, args.output)


if __name__ == "__main__":
    main()
