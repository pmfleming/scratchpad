import argparse
import json
from pathlib import Path
from typing import Any, Dict, List

from report_modes import add_mode_argument, emit_report

DEFAULT_OUTPUT = Path("measurement_catalog.json")
VISIBILITY_OUTPUT = Path("target/analysis/measurement_catalog.json")


def item(
    task_id: str,
    category: str,
    subcategory: str,
    title: str,
    description: str,
    commands: List[List[str]],
    output_artifacts: List[str],
    *,
    expensive: bool = False,
    related_profiles: List[str] | None = None,
    related_tests: List[str] | None = None,
    related_modules: List[str] | None = None,
) -> Dict[str, Any]:
    return {
        "id": task_id,
        "category": category,
        "subcategory": subcategory,
        "title": title,
        "description": description,
        "commands": commands,
        "output_artifacts": output_artifacts,
        "depends_on": [],
        "expensive": expensive,
        "supports_individual_run": True,
        "related_profiles": related_profiles or [],
        "related_tests": related_tests or [],
        "related_modules": related_modules or [],
    }


def build_catalog() -> Dict[str, Any]:
    py = ".venv/Scripts/python.exe"
    tasks = [
        item(
            "quality.hotspots",
            "quality",
            "hotspots",
            "Hotspots",
            "Ranks complexity risk without SLOC scoring.",
            [[py, "scripts/hotspots.py", "--mode", "visibility", "--paths", "src", "--scope", "all"]],
            ["target/analysis/hotspots.json"],
        ),
        item(
            "quality.clones",
            "quality",
            "clones",
            "Clones",
            "Finds repeated code structures.",
            [[py, "scripts/clone_alert.py", "--mode", "visibility", "--paths", "src"]],
            ["target/analysis/clones.json"],
        ),
        item(
            "performance.slowspots",
            "performance",
            "speed",
            "Broad Speed Tests",
            "Runs broad benchmark triage.",
            [[py, "scripts/slowspots.py", "--mode", "visibility"]],
            ["target/analysis/slowspots.json"],
        ),
        item(
            "performance.search",
            "performance",
            "searching",
            "Search Speed",
            "Measures search latency scaling.",
            [[py, "scripts/search_speed.py", "--mode", "visibility"]],
            ["target/analysis/search_speed.json"],
            related_profiles=[
                "search_current_app_state",
                "search_all_tabs",
                "search_dispatch",
            ],
            related_tests=["tests/search_tests.rs"],
        ),
        item(
            "performance.capacity",
            "performance",
            "capacity",
            "Capacity Reports",
            "Finds first unusable ceilings.",
            [[py, "scripts/capacity_report.py", "--mode", "visibility"]],
            ["target/analysis/capacity_report.json"],
            expensive=True,
        ),
        item(
            "performance.resources",
            "performance",
            "resources",
            "Resource Profiles",
            "Measures memory and allocation cost.",
            [[py, "scripts/resource_profiles.py", "--mode", "visibility"]],
            ["target/analysis/resource_profiles.json"],
            expensive=True,
        ),
        item(
            "performance.flamegraphs",
            "performance",
            "flamegraphs",
            "Flamegraphs",
            "Indexes and generates profile SVGs.",
            [[py, "scripts/generate_flamegraphs.py", "--mode", "visibility"]],
            ["target/analysis/flamegraphs.json"],
            expensive=True,
        ),
        item(
            "performance.report",
            "performance",
            "review",
            "Performance Review",
            "Combines speed, capacity, and profiles.",
            [[py, "scripts/speed_efficiency_report.py", "--mode", "visibility"]],
            ["target/analysis/speed_efficiency_report.json"],
        ),
        item(
            "correctness.catalog",
            "correctness",
            "tests",
            "Correctness Catalog",
            "Discovers tests by architecture layer.",
            [[py, "scripts/test_catalog.py", "--mode", "visibility"]],
            ["target/analysis/correctness_review.json", "target/analysis/test_catalog.json"],
        ),
        item(
            "correctness.all",
            "correctness",
            "tests",
            "All Tests",
            "Runs the full Rust test suite.",
            [[py, "scripts/test_catalog.py", "--mode", "visibility", "--run"]],
            ["target/analysis/correctness_review.json"],
            expensive=True,
        ),
        item(
            "map.architecture",
            "map",
            "architecture",
            "Architecture Map",
            "Refreshes module health and dependency map.",
            [[py, "scripts/map.py", "--mode", "visibility"]],
            ["target/analysis/map.json"],
        ),
    ]
    categories = [
        {"id": "quality", "title": "Quality Review"},
        {"id": "performance", "title": "Performance Review"},
        {"id": "correctness", "title": "Correctness Review"},
        {"id": "map", "title": "Map"},
    ]
    return {"version": 1, "categories": categories, "tasks": tasks}


def render_cli(payload: object) -> str:
    data = payload if isinstance(payload, dict) else {}
    lines = ["Measurement Catalog"]
    for task in data.get("tasks", []):
        lines.append(f"- {task['id']}: {task['title']}")
    return "\n".join(lines)


def main() -> None:
    parser = argparse.ArgumentParser(description="Emit dashboard measurement task catalog")
    parser.add_argument("--output", type=Path, default=None)
    add_mode_argument(parser)
    args = parser.parse_args()
    emit_report(
        build_catalog(),
        mode=args.mode,
        output_path=args.output,
        visibility_path=VISIBILITY_OUTPUT,
        cli_renderer=render_cli,
        label="measurement catalog",
    )


if __name__ == "__main__":
    main()
