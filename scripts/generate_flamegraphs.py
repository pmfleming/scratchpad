import argparse
import json
import shutil
import subprocess
import sys
from pathlib import Path
from typing import List, Optional

from report_modes import add_mode_argument, emit_report

DEFAULT_OUTPUT = Path("flamegraphs.json")
VISIBILITY_OUTPUT = Path("target/analysis/flamegraphs.json")
FLAMEGRAPH_DIR = Path("target/analysis/flamegraphs")
STACKS_PATH = Path("cargo-flamegraph.stacks")
MIN_FREE_BYTES = 20 * 1024 * 1024 * 1024

BENCHMARKS = [
    {
        "id": "tab_operations_profile",
        "name": "Tab Operations Profile",
        "cargo_args": ["--bin", "profile_tab_operations"],
    },
    {
        "id": "tab_tile_layout_profile",
        "name": "Tab Tile Layout Profile",
        "cargo_args": ["--bin", "profile_tab_tile_layout"],
    },
    {
        "id": "search_current_app_state_profile",
        "name": "Search Current App-State Profile",
        "cargo_args": ["--bin", "profile_search_current_app_state"],
    },
    {
        "id": "search_all_tabs_profile",
        "name": "Search All Tabs Profile",
        "cargo_args": ["--bin", "profile_search_all_tabs"],
    },
]


class FlamegraphGenerator:
    def __init__(self, output_dir: Path):
        self.output_dir = output_dir

    def load_existing_results(self, index_path: Optional[Path]) -> List[dict]:
        if index_path is None or not index_path.exists():
            return []

        try:
            payload = json.loads(index_path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError):
            return []

        return payload if isinstance(payload, list) else []

    def check_tool(self) -> bool:
        try:
            subprocess.run(
                ["cargo", "flamegraph", "--version"],
                capture_output=True,
                check=True,
                text=True,
            )
            return True
        except (subprocess.CalledProcessError, FileNotFoundError):
            return False

    def free_bytes(self) -> int:
        return shutil.disk_usage(Path.cwd()).free

    def has_enough_disk_space(self) -> bool:
        return self.free_bytes() >= MIN_FREE_BYTES

    def cleanup_stack_dump(self) -> None:
        try:
            if STACKS_PATH.exists():
                STACKS_PATH.unlink()
        except OSError as exc:
            print(
                f"Warning: Could not remove leftover stack dump '{STACKS_PATH}': {exc}",
                file=sys.stderr,
            )

    def warn_low_disk_space(self) -> None:
        free_gb = self.free_bytes() / (1024**3)
        required_gb = MIN_FREE_BYTES / (1024**3)
        print(
            (
                "Warning: Flamegraph generation skipped because disk space is low "
                f"({free_gb:.1f} GB free, {required_gb:.0f} GB required minimum)."
            ),
            file=sys.stderr,
        )

    def merge_results(self, existing_results: List[dict], new_results: List[dict]) -> List[dict]:
        merged = {
            item.get("id"): item
            for item in existing_results
            if isinstance(item, dict) and item.get("id")
        }
        for item in new_results:
            item_id = item.get("id") if isinstance(item, dict) else None
            if item_id:
                merged[item_id] = item
        return [merged[item_id] for item_id in sorted(merged)]

    def is_disk_full_error(self, output: str) -> bool:
        lowered = output.lower()
        return (
            "storagefull" in lowered
            or "disk is full" in lowered
            or "no space on device" in lowered
            or "there is not enough space on the disk" in lowered
        )

    def generate(
        self,
        skip_if_missing: bool = True,
        existing_index_path: Optional[Path] = None,
    ) -> List[dict]:
        existing_results = self.load_existing_results(existing_index_path)

        if not self.check_tool():
            print("Warning: `cargo-flamegraph` not found.", file=sys.stderr)
            if skip_if_missing:
                return existing_results
            return self.get_error_data("cargo-flamegraph not installed")

        self.cleanup_stack_dump()

        if not self.has_enough_disk_space():
            self.warn_low_disk_space()
            return existing_results

        self.output_dir.mkdir(parents=True, exist_ok=True)
        results = []
        
        for config in BENCHMARKS:
            svg_path = self.output_dir / f"{config['id']}.svg"
            print(f"Generating flamegraph for {config['name']}...", file=sys.stderr)

            if not self.has_enough_disk_space():
                self.warn_low_disk_space()
                return self.merge_results(existing_results, results)

            self.cleanup_stack_dump()

            cmd = [
                "cargo",
                "flamegraph",
                "--dev",
                "-o",
                str(svg_path),
            ]
            cmd.extend(config.get("cargo_args", []))
            program_args = config.get("program_args", [])
            if program_args:
                cmd.append("--")
                cmd.extend(program_args)

            try:
                # We don't use check=True here because we want to capture the 
                # NotAnAdmin error specifically on Windows.
                process = subprocess.run(cmd, capture_output=True, text=True)
                command_output = "\n".join(
                    part.strip() for part in [process.stderr, process.stdout] if part and part.strip()
                )
                
                if process.returncode == 0:
                    self.cleanup_stack_dump()
                    results.append(
                        {
                            "id": config["id"],
                            "name": config["name"],
                            "path": f"flamegraphs/{config['id']}.svg",
                            "type": "svg",
                        }
                    )
                else:
                    self.cleanup_stack_dump()
                    error_msg = command_output
                    if "NotAnAdmin" in error_msg:
                        print(
                            "Warning: Flamegraph generation requires admin privileges - new flamegraphs will not be generated.",
                            file=sys.stderr,
                        )
                        return self.merge_results(existing_results, results)
                    elif self.is_disk_full_error(error_msg):
                        free_gb = self.free_bytes() / (1024**3)
                        print(
                            (
                                "Warning: Flamegraph generation stopped because the disk filled up "
                                f"during '{config['name']}' ({free_gb:.1f} GB free after cleanup)."
                            ),
                            file=sys.stderr,
                        )
                        return self.merge_results(existing_results, results)
                    else:
                        print(f"Error: {config['id']} failed: {error_msg}", file=sys.stderr)
            except Exception as e:
                self.cleanup_stack_dump()
                print(f"Unexpected error for {config['id']}: {e}", file=sys.stderr)

        return results

    def get_error_data(self, reason: str) -> List[dict]:
        return [
            {
                "id": "error",
                "name": f"Error: {reason}",
                "path": None,
                "type": "error",
                "description": f"Could not generate flamegraphs. Reason: {reason}. Try running this script in an Administrator terminal."
            }
        ]


def main():
    parser = argparse.ArgumentParser(description="Generate flamegraphs for benchmarks.")
    parser.add_argument("--output", type=Path, help="Path to write the index JSON.")
    add_mode_argument(parser, default="visibility")

    args = parser.parse_args()

    generator = FlamegraphGenerator(FLAMEGRAPH_DIR)
    resolved_output = (
        VISIBILITY_OUTPUT if args.mode == "visibility" and args.output is None else args.output
    )
    results = generator.generate(
        skip_if_missing=(args.mode != "cli"),
        existing_index_path=resolved_output,
    )

    def cli_renderer(data):
        if not data:
            return "No flamegraphs generated."
        lines = ["Flamegraph Results:"]
        for item in data:
            if item.get("type") == "error":
                lines.append(f"  [!] {item['name']}")
            else:
                lines.append(f"  - {item['name']}: {item['path']}")
        return "\n".join(lines)

    emit_report(
        results,
        mode=args.mode,
        output_path=args.output,
        visibility_path=VISIBILITY_OUTPUT,
        cli_renderer=cli_renderer,
        label="flamegraph index",
    )


if __name__ == "__main__":
    main()
