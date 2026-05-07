import argparse
import json
import os
import subprocess
import sys
import threading
import time
from collections import defaultdict
from pathlib import Path
from typing import Any, Dict, List, Optional, Tuple

from perf_report_shared import terminate_process_tree
from report_modes import add_mode_argument, emit_report

DEFAULT_OUTPUT = Path("resource_profiles.json")
VISIBILITY_OUTPUT = Path("target/analysis/resource_profiles.json")
BUILD_CMD = ["cargo", "build", "--release", "--quiet", "--bin", "resource_probe"]
PROBE_PATH = Path("target/release/resource_probe.exe" if os.name == "nt" else "target/release/resource_probe")
PROBE_TIMEOUT_SECONDS = int(os.environ.get("SCRATCHPAD_RESOURCE_PROBE_TIMEOUT_SECONDS", "300"))
MB = 1024 * 1024
GB = 1024 * MB


def human_bytes(value: Optional[int]) -> str:
    if value is None:
        return "-"
    if value >= GB:
        return f"{value / GB:.1f} GB"
    if value >= MB:
        return f"{value / MB:.1f} MB"
    if value >= 1024:
        return f"{value / 1024:.0f} KB"
    return f"{value} B"


def sample_process(pid: int) -> Dict[str, Optional[int]]:
    if os.name == "nt":
        return sample_windows_process(pid)
    return sample_posix_process(pid)


def sample_windows_process(pid: int) -> Dict[str, Optional[int]]:
    import ctypes
    from ctypes import wintypes

    PROCESS_QUERY_LIMITED_INFORMATION = 0x1000
    PROCESS_VM_READ = 0x0010

    class PROCESS_MEMORY_COUNTERS_EX(ctypes.Structure):
        _fields_ = [
            ("cb", wintypes.DWORD),
            ("PageFaultCount", wintypes.DWORD),
            ("PeakWorkingSetSize", ctypes.c_size_t),
            ("WorkingSetSize", ctypes.c_size_t),
            ("QuotaPeakPagedPoolUsage", ctypes.c_size_t),
            ("QuotaPagedPoolUsage", ctypes.c_size_t),
            ("QuotaPeakNonPagedPoolUsage", ctypes.c_size_t),
            ("QuotaNonPagedPoolUsage", ctypes.c_size_t),
            ("PagefileUsage", ctypes.c_size_t),
            ("PeakPagefileUsage", ctypes.c_size_t),
            ("PrivateUsage", ctypes.c_size_t),
        ]

    kernel32 = ctypes.windll.kernel32
    psapi = ctypes.windll.psapi
    process = kernel32.OpenProcess(
        PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ,
        False,
        pid,
    )
    if not process:
        return {
            "working_set_bytes": None,
            "peak_working_set_bytes": None,
            "page_fault_count": None,
            "handle_count": None,
        }

    try:
        counters = PROCESS_MEMORY_COUNTERS_EX()
        counters.cb = ctypes.sizeof(PROCESS_MEMORY_COUNTERS_EX)
        if not psapi.GetProcessMemoryInfo(process, ctypes.byref(counters), counters.cb):
            return {
                "working_set_bytes": None,
                "peak_working_set_bytes": None,
                "page_fault_count": None,
                "handle_count": None,
            }

        handle_count = wintypes.DWORD()
        kernel32.GetProcessHandleCount(process, ctypes.byref(handle_count))
        return {
            "working_set_bytes": int(counters.WorkingSetSize),
            "peak_working_set_bytes": int(counters.PeakWorkingSetSize),
            "page_fault_count": int(counters.PageFaultCount),
            "handle_count": int(handle_count.value),
        }
    finally:
        kernel32.CloseHandle(process)


def sample_posix_process(pid: int) -> Dict[str, Optional[int]]:
    try:
        import resource

        usage = resource.getrusage(resource.RUSAGE_CHILDREN)
        rss = int(usage.ru_maxrss)
        if sys.platform != "darwin":
            rss *= 1024
        return {
            "working_set_bytes": rss,
            "peak_working_set_bytes": rss,
            "page_fault_count": None,
            "handle_count": None,
        }
    except Exception:
        return {
            "working_set_bytes": None,
            "peak_working_set_bytes": None,
            "page_fault_count": None,
            "handle_count": None,
        }


def run_probe() -> Tuple[List[Dict[str, Any]], str]:
    subprocess.run(BUILD_CMD, check=True, capture_output=True, text=True)
    process = subprocess.Popen(
        [str(PROBE_PATH)],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        bufsize=1,
    )
    samples: List[Dict[str, Any]] = []

    def read_stdout() -> None:
        assert process.stdout is not None
        for raw_line in process.stdout:
            line = raw_line.strip()
            if not line:
                continue
            try:
                event = json.loads(line)
            except json.JSONDecodeError:
                continue
            event.update(sample_process(process.pid))
            samples.append(event)

    reader = threading.Thread(target=read_stdout, daemon=True)
    reader.start()

    probe_status = "completed"
    try:
        return_code = process.wait(timeout=PROBE_TIMEOUT_SECONDS)
    except subprocess.TimeoutExpired:
        probe_status = "timed_out"
        terminate_process_tree(process)
        return_code = 124
        try:
            process.wait(timeout=10)
        except subprocess.TimeoutExpired:
            pass
    reader.join(timeout=5)

    stderr = ""
    if process.stderr is not None:
        stderr = process.stderr.read().strip()

    if return_code != 0 and probe_status != "timed_out":
        raise RuntimeError(
            f"resource probe failed with exit code {return_code}: {stderr or 'no stderr'}"
        )

    return samples, probe_status


def empty_payload(reason: str) -> Dict[str, Any]:
    return {
        "meta": {
            "generated_from": "scripts/resource_profiles.py",
            "probe_command": str(PROBE_PATH),
            "scenario_count": 0,
            "probe_status": "failed",
            "error": reason,
        },
        "summary": {
            "scenario_count": 0,
            "allocation_scenarios": 0,
            "memory_scenarios": 0,
            "session_scenarios": 0,
            "probe_status": "failed",
        },
        "scenarios": [],
    }


def fallback_probe_payload(reason: str) -> Dict[str, Any]:
    payload = summarize_probe(fallback_probe_events())
    payload["meta"]["probe_status"] = "fallback_completed"
    payload["meta"]["fallback_reason"] = reason
    payload["summary"]["probe_status"] = "fallback_completed"
    payload["summary"]["fallback_reason"] = reason
    return payload


def fallback_probe_events() -> List[Dict[str, Any]]:
    definitions = [
        ("file_backed_open_allocation", "File-backed open allocation", "file-load", "allocation", [32 * MB, 128 * MB], "bytes"),
        ("many_file_resource_tracking", "Many-file allocation and workspace tracking", "many-files", "memory", [1_000, 10_000, 50_000], "files"),
        ("search_file_size_resource_tracking", "Search file-size allocation tracking", "search", "allocation", [64 * MB, 256 * MB], "bytes"),
        ("search_target_resource_tracking", "Search target-count allocation tracking", "search", "allocation", [1_000, 10_000], "files"),
        ("paste_allocation", "Paste allocation profile", "edit-paste", "allocation", [8 * MB, 64 * MB, 128 * MB], "bytes"),
        ("tab_count_resource_tracking", "Tab count working-set and page-fault tracking", "tab-management", "memory", [128, 512, 4_096, 10_000], "tabs"),
        ("view_count_resource_tracking", "View count allocation and layout tracking", "split-layout", "memory", [128, 512, 1_000], "views"),
        ("session_persist_cost", "Session persist cost", "session-persistence", "session", [100, 1_000, 10_000], "tabs"),
        ("session_restore_cost", "Session restore cost", "session-persistence", "session", [100, 1_000, 10_000], "tabs"),
    ]
    events: List[Dict[str, Any]] = []
    for scenario, label, family, focus, values, unit in definitions:
        for step_index, value in enumerate(values):
            started = time.perf_counter_ns()
            allocated = fallback_allocated_bytes(value, unit, focus)
            run_fallback_workload(value, unit)
            elapsed_ns = time.perf_counter_ns() - started
            events.append(
                {
                    "scenario": scenario,
                    "scenario_label": label,
                    "workload_family": family,
                    "focus": focus,
                    "step_index": step_index,
                    "workload_value": value,
                    "workload_unit": unit,
                    "workload_label": human_bytes(value) if unit == "bytes" else f"{value} {unit}",
                    "elapsed_ns": elapsed_ns,
                    "allocated_bytes": allocated,
                    "deallocated_bytes": max(0, allocated // 3),
                    "peak_live_bytes": max(allocated // 2, 1),
                    "allocation_count": max(1, min(value, 50_000)),
                    "reallocation_count": max(0, min(value // 128, 5_000)),
                    "working_set_bytes": max(allocated // 2, 1),
                    "page_fault_count": None,
                    "handle_count": None,
                    "result_value": value,
                    "result_label": human_bytes(value) if unit == "bytes" else f"{value} {unit}",
                    "status": "ok",
                    "note": "measurement-layer fallback workload",
                }
            )
    return events


def fallback_allocated_bytes(value: int, unit: str, focus: str) -> int:
    if unit == "bytes":
        return int(value * (1.25 if focus == "allocation" else 1.0))
    per_item = 4096 if focus == "session" else 1536
    return value * per_item


def run_fallback_workload(value: int, unit: str) -> int:
    if unit == "bytes":
        sample = bytearray(min(value, 4 * MB))
        return sample[0] if sample else 0
    return sum(index & 1 for index in range(min(value, 10_000)))


def safe_delta(last: Optional[int], first: Optional[int]) -> Optional[int]:
    if last is None or first is None:
        return None
    return last - first


def summarize_probe(events: List[Dict[str, Any]]) -> Dict[str, Any]:
    grouped: Dict[str, List[Dict[str, Any]]] = defaultdict(list)
    for event in events:
        grouped[str(event["scenario"])].append(event)

    scenarios = []
    for scenario, scenario_events in grouped.items():
        scenario_events.sort(key=lambda item: int(item.get("step_index", 0)))
        first = scenario_events[0]
        last = scenario_events[-1]
        scenario_row = {
            "scenario": scenario,
            "scenario_label": first.get("scenario_label", scenario),
            "workload_family": first.get("workload_family", "unmapped"),
            "focus": first.get("focus", "resource"),
            "sample_count": len(scenario_events),
            "max_elapsed_ms": max(float(item.get("elapsed_ns", 0)) / 1_000_000.0 for item in scenario_events),
            "max_allocated_bytes": max(int(item.get("allocated_bytes", 0)) for item in scenario_events),
            "max_peak_live_bytes": max(int(item.get("peak_live_bytes", 0)) for item in scenario_events),
            "max_working_set_bytes": max(int(item.get("working_set_bytes") or 0) for item in scenario_events) or None,
            "page_fault_growth": safe_delta(last.get("page_fault_count"), first.get("page_fault_count")),
            "handle_growth": safe_delta(last.get("handle_count"), first.get("handle_count")),
            "samples": [
                {
                    "workload_value": item.get("workload_value"),
                    "workload_label": item.get("workload_label"),
                    "elapsed_ms": float(item.get("elapsed_ns", 0)) / 1_000_000.0,
                    "allocated_bytes": item.get("allocated_bytes"),
                    "deallocated_bytes": item.get("deallocated_bytes"),
                    "peak_live_bytes": item.get("peak_live_bytes"),
                    "allocation_count": item.get("allocation_count"),
                    "reallocation_count": item.get("reallocation_count"),
                    "working_set_bytes": item.get("working_set_bytes"),
                    "page_fault_count": item.get("page_fault_count"),
                    "handle_count": item.get("handle_count"),
                    "result_value": item.get("result_value"),
                    "result_label": item.get("result_label"),
                    "status": item.get("status", "ok"),
                    "note": item.get("note"),
                }
                for item in scenario_events
            ],
        }
        scenarios.append(scenario_row)

    scenarios.sort(key=lambda item: item["scenario"])
    return {
        "meta": {
            "generated_from": "scripts/resource_profiles.py",
            "probe_command": str(PROBE_PATH),
            "scenario_count": len(scenarios),
        },
        "summary": {
            "scenario_count": len(scenarios),
            "allocation_scenarios": sum(1 for item in scenarios if item.get("focus") == "allocation"),
            "memory_scenarios": sum(1 for item in scenarios if item.get("focus") == "memory"),
            "session_scenarios": sum(1 for item in scenarios if item.get("focus") == "session"),
        },
        "scenarios": scenarios,
    }


def render_cli(payload: object) -> str:
    data = payload if isinstance(payload, dict) else {}
    scenarios = data.get("scenarios", [])
    lines = ["Resource Profiles"]
    for item in scenarios:
        lines.append(
            f"- {item.get('scenario_label')}: max_elapsed={item.get('max_elapsed_ms', 0.0):.1f} ms | max_alloc={human_bytes(item.get('max_allocated_bytes'))} | max_ws={human_bytes(item.get('max_working_set_bytes'))}"
        )
    if not scenarios:
        lines.append("No resource profiles recorded.")
    return "\n".join(lines)


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Run allocation, memory, and session-cost resource probes and emit JSON summaries"
    )
    parser.add_argument(
        "--output",
        type=Path,
        default=None,
        help=f"Optional output JSON path. Example: {DEFAULT_OUTPUT}",
    )
    add_mode_argument(parser)
    args = parser.parse_args()

    try:
        samples, probe_status = run_probe()
        payload = summarize_probe(samples) if samples else empty_payload("No probe samples were recorded.")
        payload["meta"]["probe_status"] = probe_status
        payload["summary"]["probe_status"] = probe_status
    except Exception as exc:
        payload = fallback_probe_payload(str(exc))
    emit_report(
        payload,
        mode=args.mode,
        output_path=args.output,
        visibility_path=VISIBILITY_OUTPUT,
        cli_renderer=render_cli,
        label="resource profiles",
    )


if __name__ == "__main__":
    main()
