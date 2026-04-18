import argparse
import json
import os
import subprocess
import sys
from collections import defaultdict
from pathlib import Path
from typing import Any, Dict, List, Optional

from perf_report_shared import matching_flamegraph_ids
from report_modes import add_mode_argument, emit_report

DEFAULT_OUTPUT = Path("capacity_report.json")
VISIBILITY_OUTPUT = Path("target/analysis/capacity_report.json")
BUILD_CMD = ["cargo", "build", "--release", "--quiet", "--bin", "capacity_probe"]
PROBE_PATH = Path("target/release/capacity_probe.exe" if os.name == "nt" else "target/release/capacity_probe")
MB = 1024 * 1024
GB = 1024 * MB

SCENARIO_CONFIG = {
    "file_size_ceiling": {
        "threshold_ms": 160.0,
        "workload_family": "capacity-stress",
        "cpu_flamegraph_id": None,
        "memory_guidance": "Use allocation or working-set profiling before adding another CPU flamegraph.",
        "cpu_guidance": "If load or redraw dominates, compare against the large-file load and scroll latency rows before adding a dedicated load profile.",
    },
    "tab_count_ceiling": {
        "threshold_ms": 140.0,
        "workload_family": "capacity-stress",
        "cpu_flamegraph_id": "tab_operations_profile",
        "memory_guidance": "Inspect working-set growth and object retention across tab construction and combine operations.",
        "cpu_guidance": "Capture the tab operations flamegraph if the ceiling is CPU-bound.",
    },
    "split_count_ceiling": {
        "threshold_ms": 120.0,
        "workload_family": "capacity-stress",
        "cpu_flamegraph_id": "large_file_split_profile",
        "memory_guidance": "Inspect pane-tree growth and allocation churn before chasing another CPU-only explanation.",
        "cpu_guidance": "Capture the large-file split flamegraph if split rebalance is the limiting path.",
    },
    "paste_size_ceiling": {
        "threshold_ms": 150.0,
        "workload_family": "capacity-stress",
        "cpu_flamegraph_id": "large_file_paste_profile",
        "memory_guidance": "Check working-set growth and page-fault pressure around paste plus metadata refresh.",
        "cpu_guidance": "Capture the large-file paste flamegraph if mutation latency dominates without large memory growth.",
    },
}


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
        if not psapi.GetProcessMemoryInfo(
            process,
            ctypes.byref(counters),
            counters.cb,
        ):
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


def run_probe() -> List[Dict[str, Any]]:
    subprocess.run(BUILD_CMD, check=True, capture_output=True, text=True)
    process = subprocess.Popen(
        [str(PROBE_PATH)],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        bufsize=1,
    )
    samples: List[Dict[str, Any]] = []

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

    stderr = ""
    if process.stderr is not None:
        stderr = process.stderr.read().strip()

    return_code = process.wait()
    if return_code != 0:
        raise RuntimeError(
            f"capacity probe failed with exit code {return_code}: {stderr or 'no stderr'}"
        )

    return samples


def infer_limiting_resource(events: List[Dict[str, Any]]) -> str:
    first = events[0]
    last = events[-1]
    handle_growth = safe_delta(last.get("handle_count"), first.get("handle_count"))
    working_set_growth = safe_delta(
        last.get("working_set_bytes"),
        first.get("working_set_bytes"),
    )
    page_fault_growth = safe_delta(
        last.get("page_fault_count"),
        first.get("page_fault_count"),
    )

    if handle_growth is not None and handle_growth >= 256:
        return "os-handles"
    if page_fault_growth is not None and page_fault_growth >= 10_000:
        return "memory"
    if working_set_growth is not None and working_set_growth >= 128 * MB:
        return "memory"
    return "cpu"


def safe_delta(last: Optional[int], first: Optional[int]) -> Optional[int]:
    if last is None or first is None:
        return None
    return last - first


def diagnosis_guidance(
    scenario: str,
    limiting_resource: str,
    matching_flamegraphs: List[str],
) -> List[str]:
    config = SCENARIO_CONFIG.get(scenario, {})
    guidance = []
    if limiting_resource == "cpu":
        cpu_guidance = config.get("cpu_guidance")
        if cpu_guidance:
            guidance.append(str(cpu_guidance))
        if matching_flamegraphs:
            guidance.append(
                f"Mapped CPU profile coverage: {', '.join(matching_flamegraphs)}."
            )
    elif limiting_resource == "memory":
        memory_guidance = config.get("memory_guidance")
        if memory_guidance:
            guidance.append(str(memory_guidance))
        guidance.append(
            "Prefer allocation, working-set, or page-fault diagnostics before adding another CPU flamegraph."
        )
    else:
        guidance.append(
            "Inspect handle counts, temp files, and other OS limits during the next stress run."
        )
    guidance.append(
        "Use the USE checklist: utilization, saturation, and errors for CPU, memory, I/O, and OS resources."
    )
    return guidance


def resource_checklist(
    limiting_resource: str,
    events: List[Dict[str, Any]],
) -> List[Dict[str, str]]:
    first = events[0]
    last = events[-1]
    working_set_growth = safe_delta(
        last.get("working_set_bytes"),
        first.get("working_set_bytes"),
    )
    page_fault_growth = safe_delta(
        last.get("page_fault_count"),
        first.get("page_fault_count"),
    )
    handle_growth = safe_delta(last.get("handle_count"), first.get("handle_count"))
    return [
        {
            "resource": "cpu",
            "status": "focus" if limiting_resource == "cpu" else "watch",
            "note": "Latency rose before another resource clearly saturated."
            if limiting_resource == "cpu"
            else "Capture a CPU flamegraph only if working-set growth stays modest.",
        },
        {
            "resource": "memory",
            "status": "focus" if limiting_resource == "memory" else "watch",
            "note": f"Working-set growth {human_bytes(working_set_growth)}; page-fault delta {page_fault_growth if page_fault_growth is not None else '-'}.",
        },
        {
            "resource": "i/o",
            "status": "not-measured",
            "note": "These sweeps are in-memory. Re-run with file-backed workloads if open/save ceilings are the concern.",
        },
        {
            "resource": "os-resources",
            "status": "focus" if limiting_resource == "os-handles" else "watch",
            "note": f"Handle growth {handle_growth if handle_growth is not None else '-'}.",
        },
    ]


def summarize_probe(events: List[Dict[str, Any]]) -> Dict[str, Any]:
    grouped: Dict[str, List[Dict[str, Any]]] = defaultdict(list)
    for event in events:
        grouped[str(event["scenario"])].append(event)

    scenarios = []
    for scenario, scenario_events in grouped.items():
        scenario_events.sort(key=lambda item: int(item.get("step_index", 0)))
        config = SCENARIO_CONFIG.get(scenario, {})
        threshold_ms = float(config.get("threshold_ms", 100.0))
        first_failure = None
        last_success = None

        for event in scenario_events:
            elapsed_ms = float(event["elapsed_ns"]) / 1_000_000.0
            if event.get("status") != "ok":
                first_failure = event
                break
            if elapsed_ms > threshold_ms:
                first_failure = event
                break
            last_success = event

        limiting_resource = infer_limiting_resource(scenario_events)
        matching = []
        cpu_flamegraph_id = config.get("cpu_flamegraph_id")
        if isinstance(cpu_flamegraph_id, str) and cpu_flamegraph_id:
            matching.append(cpu_flamegraph_id)
        if not matching:
            matching = matching_flamegraph_ids(scenario)

        first = scenario_events[0]
        last = scenario_events[-1]
        peak_working_set = max(
            (item.get("peak_working_set_bytes") or item.get("working_set_bytes") or 0)
            for item in scenario_events
        ) or None
        scenario_row = {
            "scenario": scenario,
            "scenario_label": first.get("scenario_label", scenario),
            "workload_family": first.get("workload_family", config.get("workload_family", "capacity-stress")),
            "threshold_ms": threshold_ms,
            "failure_mode": (
                "panic"
                if first_failure and first_failure.get("status") != "ok"
                else "unusable_latency"
                if first_failure
                else "not_reached"
            ),
            "ceiling_reached": first_failure is not None,
            "last_successful_workload": last_success.get("workload_value") if last_success else None,
            "last_successful_label": last_success.get("workload_label") if last_success else None,
            "first_failure_workload": first_failure.get("workload_value") if first_failure else None,
            "first_failure_label": first_failure.get("workload_label") if first_failure else None,
            "peak_working_set_bytes": peak_working_set,
            "working_set_growth_bytes": safe_delta(
                last.get("working_set_bytes"),
                first.get("working_set_bytes"),
            ),
            "page_fault_growth": safe_delta(
                last.get("page_fault_count"),
                first.get("page_fault_count"),
            ),
            "handle_growth": safe_delta(
                last.get("handle_count"),
                first.get("handle_count"),
            ),
            "first_saturated_resource": limiting_resource,
            "suspected_limiting_resource": limiting_resource,
            "matching_flamegraphs": matching,
            "diagnosis_guidance": diagnosis_guidance(scenario, limiting_resource, matching),
            "resource_checklist": resource_checklist(limiting_resource, scenario_events),
            "samples": [
                {
                    "workload_value": item.get("workload_value"),
                    "workload_label": item.get("workload_label"),
                    "elapsed_ms": float(item["elapsed_ns"]) / 1_000_000.0,
                    "working_set_bytes": item.get("working_set_bytes"),
                    "page_fault_count": item.get("page_fault_count"),
                    "handle_count": item.get("handle_count"),
                    "status": item.get("status", "ok"),
                }
                for item in scenario_events
            ],
        }
        scenarios.append(scenario_row)

    scenarios.sort(key=lambda item: item["scenario"])
    ceilings_reached = sum(1 for item in scenarios if item["ceiling_reached"])
    memory_bound = sum(
        1 for item in scenarios if item["suspected_limiting_resource"] == "memory"
    )
    return {
        "meta": {
            "generated_from": "scripts/capacity_report.py",
            "probe_command": str(PROBE_PATH),
            "scenario_count": len(scenarios),
        },
        "summary": {
            "scenario_count": len(scenarios),
            "ceilings_reached": ceilings_reached,
            "memory_bound_scenarios": memory_bound,
            "cpu_bound_scenarios": sum(
                1 for item in scenarios if item["suspected_limiting_resource"] == "cpu"
            ),
        },
        "scenarios": scenarios,
    }


def render_cli(payload: object) -> str:
    data = payload if isinstance(payload, dict) else {}
    scenarios = data.get("scenarios", [])
    lines = ["Capacity Report"]
    for item in scenarios:
        ceiling = item.get("first_failure_label") or item.get("last_successful_label") or "-"
        lines.append(
            f"- {item.get('scenario_label', item.get('scenario'))}: ceiling={ceiling} | mode={item.get('failure_mode', '-')} | resource={item.get('suspected_limiting_resource', '-')}"
        )
    if not scenarios:
        lines.append("No capacity scenarios recorded.")
    return "\n".join(lines)


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Run capacity threshold sweeps and emit ceiling, failure-mode, and resource-diagnosis JSON"
    )
    parser.add_argument(
        "--output",
        type=Path,
        default=None,
        help=f"Optional output JSON path. Example: {DEFAULT_OUTPUT}",
    )
    add_mode_argument(parser)
    args = parser.parse_args()

    samples = run_probe()
    payload = summarize_probe(samples)
    emit_report(
        payload,
        mode=args.mode,
        output_path=args.output,
        visibility_path=VISIBILITY_OUTPUT,
        cli_renderer=render_cli,
        label="capacity report",
    )


if __name__ == "__main__":
    main()