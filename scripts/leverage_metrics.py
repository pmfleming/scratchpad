import argparse
import json
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Dict, Iterable, List, Optional, Sequence

from report_modes import add_mode_argument, emit_report

DEFAULT_OUTPUT = Path("leverage_metrics.json")
VISIBILITY_OUTPUT = Path("target/analysis/leverage_metrics.json")


class LeverageAnalyzer:
    def __init__(self, top: Optional[int]):
        self.top = top

    def _iter_rust_files(self, paths: Sequence[str]) -> Iterable[Path]:
        for raw_path in paths:
            path = Path(raw_path)
            if path.is_file() and path.suffix == ".rs":
                yield path
                continue
            if path.is_dir():
                yield from path.rglob("*.rs")

    def run(self, paths: Sequence[str]) -> List[Dict]:
        all_files = [str(path) for path in self._iter_rust_files(paths)]
        if not all_files:
            return []
        if not shutil.which("cargo"):
            print("Warning: cargo not found; skipping leverage AST analysis.", file=sys.stderr)
            return []

        with tempfile.NamedTemporaryFile(mode='w', delete=False, encoding='utf-8') as f:
            for file_path in all_files:
                f.write(f"{file_path}\n")
            temp_path = f.name

        cmd = ["cargo", "run", "--quiet", "--bin", "leverage_ast", "--", temp_path]
        try:
            result = subprocess.run(
                cmd,
                capture_output=True,
                text=True,
                check=True,
            )
        except Exception as exc:
            print(f"Warning: Leverage AST analysis failed: {exc}", file=sys.stderr)
            return []
        finally:
            try:
                os.remove(temp_path)
            except OSError:
                pass

        try:
            records = json.loads(result.stdout)
        except json.JSONDecodeError as exc:
            print(f"Warning: Leverage AST output was malformed: {exc}", file=sys.stderr)
            return []

        ranked = sorted(records, key=lambda item: (-item.get("total_leverage_score", 0.0), item.get("module_name", "")))
        if self.top is not None:
            return ranked[: self.top]
        return ranked


def render_cli(payload: object) -> str:
    rows = payload if isinstance(payload, list) else []
    lines = ["Leverage Metrics"]
    if not rows:
        lines.append("No leverage metrics found.")
        return "\n".join(lines)

    for index, item in enumerate(rows[:10], start=1):
        lines.append(
            f"{index:>2}. {item['module_name']} | score={item['total_leverage_score']:.2f} | iterator={item['iterator_leverage_score']:.1f}% | indirection={item['indirection_ratio']:.1f}% | unsafe={item['unsafe_blocks']}"
        )

    if len(rows) > 10:
        lines.append(f"... and {len(rows) - 10} more modules.")

    return "\n".join(lines)


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Emit Leverage AST metrics as JSON"
    )
    parser.add_argument("--paths", nargs="+", default=["src"], help="Paths to analyze")
    parser.add_argument(
        "--top",
        type=int,
        default=None,
        help="Limit the number of ranked records. Defaults to all records.",
    )
    parser.add_argument(
        "--output",
        type=Path,
        default=None,
        help=f"Optional output JSON path. Example: {DEFAULT_OUTPUT}",
    )
    add_mode_argument(parser)

    args = parser.parse_args()
    analyzer = LeverageAnalyzer(top=args.top)
    payload = analyzer.run(args.paths)
    
    emit_report(
        payload,
        mode=args.mode,
        output_path=args.output,
        visibility_path=VISIBILITY_OUTPUT,
        cli_renderer=render_cli,
        label="leverage",
    )


if __name__ == "__main__":
    main()
