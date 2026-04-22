import json
from pathlib import Path
from typing import Any, Dict, List

BENCHMARK_METADATA_PATHS = (
    Path("benches/benchmark_targets.json"),
    Path("benches/search_benchmark_targets.json"),
)

FLAMEGRAPH_CONFIGS = [
    {
        "id": "tab_operations_profile",
        "name": "Tab Operations Profile",
        "cargo_args": ["--bin", "profile_tab_operations"],
        "benchmark_keys": ["tab_stress_operations", "tab_count_scale"],
        "workload_families": ["tab-management"],
        "coverage_role": "report-driven",
        "resource_focus": "cpu",
        "description": "Tab activation, reorder, and multi-tab movement hot path.",
    },
    {
        "id": "tab_tile_layout_profile",
        "name": "Tab Tile Layout Profile",
        "cargo_args": ["--bin", "profile_tab_tile_layout"],
        "benchmark_keys": ["tile_count_scale"],
        "workload_families": ["split-layout"],
        "coverage_role": "report-driven",
        "resource_focus": "cpu",
        "description": "Split resize, rebalance, and tile layout hot path.",
    },
    {
        "id": "view_navigation_profile",
        "name": "View Navigation Profile",
        "cargo_args": ["--bin", "profile_view_navigation"],
        "benchmark_keys": [],
        "workload_families": ["exploratory"],
        "coverage_role": "exploratory",
        "resource_focus": "cpu",
        "description": "Exploratory editor-view navigation profile without a dedicated broad benchmark family.",
    },
    {
        "id": "search_current_app_state_profile",
        "name": "Search Current App-State Profile",
        "cargo_args": ["--bin", "profile_search_current_app_state"],
        "benchmark_keys": [
            "search_active_completion_file_size",
            "search_active_first_response_file_size",
            "search_current_completion_file_size",
            "search_current_completion_aggregate_size",
            "search_current_app_state_completion_aggregate_size",
        ],
        "workload_families": ["search"],
        "coverage_role": "report-driven",
        "resource_focus": "cpu",
        "description": "Active-file and current-tab search hot path through the Scratchpad search pipeline.",
    },
    {
        "id": "search_all_tabs_profile",
        "name": "Search All Tabs Profile",
        "cargo_args": ["--bin", "profile_search_all_tabs"],
        "benchmark_keys": [
            "search_all_completion_file_size",
            "search_all_completion_aggregate_size",
        ],
        "workload_families": ["search"],
        "coverage_role": "report-driven",
        "resource_focus": "cpu",
        "description": "All-open-tabs search hot path across the global workspace tab manager.",
    },
    {
        "id": "large_file_scroll_profile",
        "name": "Large File Scroll Profile",
        "cargo_args": ["--bin", "profile_large_file_scroll"],
        "benchmark_keys": ["large_file_scroll_latency"],
        "workload_families": ["scroll"],
        "coverage_role": "report-driven",
        "resource_focus": "cpu",
        "description": "Headless editor layout and repaint work representative of large-file scroll redraw.",
    },
    {
        "id": "large_file_paste_profile",
        "name": "Large File Paste Profile",
        "cargo_args": ["--bin", "profile_large_file_paste"],
        "benchmark_keys": ["large_file_paste_latency"],
        "workload_families": ["edit-paste"],
        "coverage_role": "report-driven",
        "resource_focus": "cpu",
        "description": "Large insert into an already large buffer, including metadata refresh and undo state updates.",
    },
    {
        "id": "large_file_split_profile",
        "name": "Large File Split Profile",
        "cargo_args": ["--bin", "profile_large_file_split"],
        "benchmark_keys": ["large_file_split_latency"],
        "workload_families": ["split-layout"],
        "coverage_role": "report-driven",
        "resource_focus": "cpu",
        "description": "Repeated splitting and rebalance work on large file tiles.",
    },
]


def benchmark_key_from_name(benchmark_name: str) -> str:
    return benchmark_name.split("/", 1)[0]


def flamegraph_configs() -> List[Dict[str, Any]]:
    return [dict(config) for config in FLAMEGRAPH_CONFIGS]


def matching_flamegraph_ids(benchmark_key: str) -> List[str]:
    matches = []
    for config in FLAMEGRAPH_CONFIGS:
        if benchmark_key in config.get("benchmark_keys", []):
            matches.append(str(config["id"]))
    return matches


def load_benchmark_metadata(default_threshold: float = 50.0) -> Dict[str, Dict[str, Any]]:
    metadata: Dict[str, Dict[str, Any]] = {}

    for path in BENCHMARK_METADATA_PATHS:
        if not path.exists():
            continue

        with path.open("r", encoding="utf-8") as handle:
            data = json.load(handle)

        is_search_metadata = path.name == "search_benchmark_targets.json"
        for key, value in data.items():
            normalized = {
                "targets": list(value.get("targets", [])),
                "kind": value.get("kind", "workflow" if is_search_metadata else "unmapped"),
                "threshold_ms": float(value.get("threshold_ms", default_threshold)),
                "workload_family": value.get(
                    "family",
                    "search" if is_search_metadata else "unmapped",
                ),
                "limiting_resource_hint": value.get("limiting_resource_hint", "cpu"),
            }
            for extra_key, extra_value in value.items():
                if extra_key == "family":
                    continue
                normalized[extra_key] = extra_value
            metadata[key] = normalized

    return metadata


def metadata_for_benchmark(
    benchmark_name: str,
    metadata: Dict[str, Dict[str, Any]],
    default_threshold: float,
) -> Dict[str, Any]:
    benchmark_key = benchmark_key_from_name(benchmark_name)
    return metadata.get(
        benchmark_key,
        {
            "targets": [],
            "kind": "unmapped",
            "threshold_ms": default_threshold,
            "workload_family": "unmapped",
            "limiting_resource_hint": "cpu",
        },
    )
