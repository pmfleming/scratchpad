import argparse
import json
import os
from queue import Empty, Queue
import subprocess
import sys
import threading
import time
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Any, Callable, Dict, List
from urllib.parse import unquote, urlparse

from app_package import app_package_payload, clear_app_package_buffers
from measurement_catalog import build_catalog
from perf_report_shared import terminate_process_tree

RUNS_PATH = Path("target/analysis/measurement_runs.json")
LOG_DIR = Path("target/analysis/logs")
ACTIVE_STATUSES = {"queued", "running"}
COMMAND_TIMEOUT_SECONDS = int(
    os.environ.get("SCRATCHPAD_DASHBOARD_COMMAND_TIMEOUT_SECONDS", "1800")
)
STALE_PROCESS_MIN_AGE_SECONDS = int(
    os.environ.get("SCRATCHPAD_DASHBOARD_STALE_PROCESS_MIN_AGE_SECONDS", "10")
)


class RunStore:
    def __init__(self) -> None:
        self.lock = threading.Lock()
        self.runs: List[Dict[str, Any]] = self._load()
        self.mark_loaded_active_runs_interrupted()

    def _load(self) -> List[Dict[str, Any]]:
        if not RUNS_PATH.exists():
            return []
        try:
            payload = json.loads(RUNS_PATH.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError):
            return []
        return payload if isinstance(payload, list) else []

    def save(self) -> None:
        RUNS_PATH.parent.mkdir(parents=True, exist_ok=True)
        RUNS_PATH.write_text(json.dumps(self.runs[-100:], indent=2) + "\n", encoding="utf-8")

    def add(self, run: Dict[str, Any]) -> None:
        with self.lock:
            self.runs.append(run)
            self.save()

    def try_add_queued(self, run: Dict[str, Any]) -> Dict[str, Any] | None:
        with self.lock:
            active = next(
                (item for item in self.runs if item.get("status") in ACTIVE_STATUSES),
                None,
            )
            if active is not None:
                return active
            self.runs.append(run)
            self.save()
            return None

    def update(self, run_id: str, **changes: Any) -> None:
        with self.lock:
            for run in self.runs:
                if run["id"] == run_id:
                    run.update(changes)
                    break
            self.save()

    def snapshot(self) -> List[Dict[str, Any]]:
        with self.lock:
            return list(self.runs)

    def mark_loaded_active_runs_interrupted(self) -> None:
        now = time.time()
        changed = False
        for run in self.runs:
            if run.get("status") not in ACTIVE_STATUSES:
                continue
            started = run.get("started_at") or run.get("created_at") or now
            run.update(
                {
                    "status": "interrupted",
                    "exit_code": None,
                    "finished_at": now,
                    "duration_seconds": round(max(0.0, now - started), 3),
                    "error": "Dashboard server restarted before this run completed.",
                }
            )
            changed = True
        if changed:
            self.save()


STORE = RunStore()
RUNNER_LOCK = threading.Lock()


def json_response(handler: SimpleHTTPRequestHandler, status: int, payload: Any) -> None:
    body = json.dumps(payload, indent=2).encode("utf-8")
    handler.send_response(status)
    handler.send_header("Content-Type", "application/json; charset=utf-8")
    handler.send_header("Cache-Control", "no-store")
    handler.send_header("Content-Length", str(len(body)))
    handler.end_headers()
    handler.wfile.write(body)


def task_catalog() -> Dict[str, Any]:
    return build_catalog()


def selected_tasks(selector: str) -> List[Dict[str, Any]]:
    catalog = task_catalog()
    tasks = catalog["tasks"]
    if selector == "all":
        return tasks
    if selector.startswith("category/"):
        category = selector.split("/", 1)[1]
        return [task for task in tasks if task["category"] == category]
    if selector.startswith("item/"):
        task_id = selector.split("/", 1)[1]
        return [task for task in tasks if task["id"] == task_id]
    return []


def normalize_command(command: List[str]) -> List[str]:
    if command and command[0].endswith("python.exe") and not Path(command[0]).exists():
        return [sys.executable] + command[1:]
    return command


def progress_detail(line: str, limit: int = 160) -> str:
    detail = " ".join(line.strip().split())
    if len(detail) <= limit:
        return detail
    return detail[: limit - 3] + "..."


def append_output(
    item: str,
    stdout_chunks: List[str],
    on_output: Callable[[str], None] | None,
) -> None:
    stdout_chunks.append(item)
    if on_output is not None:
        on_output(item)


def drain_output_queue(
    output_queue: Queue[str | None],
    stdout_chunks: List[str],
    on_output: Callable[[str], None] | None,
    *,
    timeout: float | None = None,
    process: subprocess.Popen[str] | None = None,
) -> bool:
    reader_finished = False
    while True:
        try:
            item = (
                output_queue.get(timeout=timeout)
                if timeout is not None
                else output_queue.get_nowait()
            )
        except Empty:
            return reader_finished or (process is not None and process.poll() is not None)
        if item is None:
            return True
        append_output(item, stdout_chunks, on_output)


def run_command(
    command: List[str],
    *,
    on_output: Callable[[str], None] | None = None,
    on_heartbeat: Callable[[], None] | None = None,
) -> subprocess.CompletedProcess[str]:
    creationflags = (
        getattr(subprocess, "CREATE_NEW_PROCESS_GROUP", 0) if os.name == "nt" else 0
    )
    process = subprocess.Popen(
        command,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        bufsize=1,
        creationflags=creationflags,
    )
    output_queue: Queue[str | None] = Queue()
    stdout_chunks: List[str] = []

    def pump_stdout() -> None:
        assert process.stdout is not None
        try:
            for line in process.stdout:
                output_queue.put(line)
        finally:
            output_queue.put(None)

    reader = threading.Thread(target=pump_stdout, daemon=True)
    reader.start()
    deadline = time.time() + COMMAND_TIMEOUT_SECONDS
    reader_finished = False
    try:
        while True:
            if on_heartbeat is not None:
                on_heartbeat()
            if time.time() >= deadline:
                raise subprocess.TimeoutExpired(command, COMMAND_TIMEOUT_SECONDS)
            try:
                item = output_queue.get(timeout=1)
            except Empty:
                if process.poll() is not None and reader_finished:
                    break
                continue
            if item is None:
                reader_finished = True
                if process.poll() is not None:
                    break
                continue
            append_output(item, stdout_chunks, on_output)

        drain_output_queue(output_queue, stdout_chunks, on_output)

        reader.join(timeout=1)
        return subprocess.CompletedProcess(
            command,
            process.wait(),
            "".join(stdout_chunks),
            "",
        )
    except subprocess.TimeoutExpired:
        terminate_process_tree(process)
        try:
            process.wait(timeout=10)
        except subprocess.TimeoutExpired:
            pass
        drain_output_queue(
            output_queue,
            stdout_chunks,
            on_output,
            timeout=0.2,
            process=process,
        )
        reader.join(timeout=1)
        stderr = (
            f"\nCommand timed out after {COMMAND_TIMEOUT_SECONDS} seconds and was stopped.\n"
        )
        return subprocess.CompletedProcess(command, 124, "".join(stdout_chunks), stderr)


def cleanup_stale_measurement_processes() -> List[Dict[str, Any]]:
    if os.name != "nt":
        return []

    repo = str(Path.cwd().resolve())
    script = r"""
$repo = [System.IO.Path]::GetFullPath($env:SCRATCHPAD_DASHBOARD_REPO_ROOT).TrimEnd('\').ToLowerInvariant()
$target = [System.IO.Path]::Combine($repo, 'target').ToLowerInvariant()
$minAgeSeconds = [double]$env:SCRATCHPAD_DASHBOARD_STALE_MIN_AGE
$now = Get-Date
$killed = @()
Get-CimInstance Win32_Process | ForEach-Object {
    $name = [string]$_.Name
    $path = [string]$_.ExecutablePath
    if (-not [string]::IsNullOrWhiteSpace($path)) {
        $normalized = $path.ToLowerInvariant()
        $ageSeconds = if ($_.CreationDate) { ($now - $_.CreationDate).TotalSeconds } else { 999999 }
        $isTargetProcess = (
            $name -like 'search_speed*.exe' -or
            $name -eq 'capacity_probe.exe' -or
            $name -eq 'resource_probe.exe'
        )
        if ($isTargetProcess -and $ageSeconds -ge $minAgeSeconds -and $normalized.StartsWith($target)) {
            taskkill /PID $_.ProcessId /T /F | Out-Null
            $killed += [PSCustomObject]@{
                process_id = $_.ProcessId
                name = $name
                path = $path
                age_seconds = [Math]::Round($ageSeconds, 1)
            }
        }
    }
}
@($killed) | ConvertTo-Json -Compress
"""
    env = os.environ.copy()
    env["SCRATCHPAD_DASHBOARD_REPO_ROOT"] = repo
    env["SCRATCHPAD_DASHBOARD_STALE_MIN_AGE"] = str(STALE_PROCESS_MIN_AGE_SECONDS)
    result = subprocess.run(
        [
            "powershell.exe",
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            script,
        ],
        capture_output=True,
        env=env,
        text=True,
    )
    if result.returncode != 0:
        return [
            {
                "error": "stale process cleanup failed",
                "stderr": result.stderr.strip(),
            }
        ]
    output = result.stdout.strip()
    if not output:
        return []
    try:
        payload = json.loads(output)
    except json.JSONDecodeError:
        return [{"error": "stale process cleanup returned invalid JSON", "stdout": output}]
    if isinstance(payload, list):
        return payload
    if isinstance(payload, dict):
        return [payload]
    return []


def write_stale_cleanup_log(log: Any, killed: List[Dict[str, Any]]) -> None:
    log.write("## stale-process-cleanup\n")
    if not killed:
        log.write("No stale Scratchpad measurement processes found.\n\n")
        return
    for item in killed:
        if "error" in item:
            log.write(f"- {item['error']}: {item.get('stderr') or item.get('stdout') or '-'}\n")
            continue
        log.write(
            f"- killed pid={item.get('process_id')} name={item.get('name')} "
            f"age={item.get('age_seconds')}s path={item.get('path')}\n"
        )
    log.write("\n")


def run_task_batch(run_id: str, selector: str, tasks: List[Dict[str, Any]]) -> None:
    with RUNNER_LOCK:
        LOG_DIR.mkdir(parents=True, exist_ok=True)
        log_path = LOG_DIR / f"{run_id}.log"
        started = time.time()
        STORE.update(
            run_id, status="running", started_at=started, log_path=log_path.as_posix()
        )
        exit_code = 0
        error_message = None
        artifacts: List[str] = []
        completed_task_ids: List[str] = []
        failed_task_ids: List[str] = []
        total_tasks = len(tasks)
        try:
            with log_path.open("w", encoding="utf-8") as log:
                if selector == "all":
                    write_stale_cleanup_log(log, cleanup_stale_measurement_processes())
                    log.flush()
                for task in tasks:
                    last_store_update = 0.0

                    def update_progress(*, detail: str | None = None, force: bool = False) -> None:
                        nonlocal last_store_update
                        changes: Dict[str, Any] = {
                            "current_task_id": task["id"],
                            "completed_tasks": len(completed_task_ids),
                            "total_tasks": total_tasks,
                            "completed_task_ids": list(completed_task_ids),
                            "failed_task_ids": list(failed_task_ids),
                            "last_update_at": time.time(),
                        }
                        if detail is not None:
                            changes["current_task_detail"] = detail
                        if not force and time.time() - last_store_update < 5:
                            return
                        STORE.update(run_id, **changes)
                        last_store_update = time.time()

                    STORE.update(
                        run_id,
                        current_task_id=task["id"],
                        current_task_detail=None,
                        completed_tasks=len(completed_task_ids),
                        total_tasks=total_tasks,
                        completed_task_ids=list(completed_task_ids),
                        failed_task_ids=list(failed_task_ids),
                        last_update_at=time.time(),
                    )
                    log.write(f"## {task['id']} - {task['title']}\n")
                    log.flush()
                    task_exit_code = 0
                    for raw_command in task["commands"]:
                        command = normalize_command(list(raw_command))
                        log.write(f"$ {' '.join(command)}\n")
                        log.flush()

                        def append_output(chunk: str) -> None:
                            log.write(chunk)
                            log.flush()
                            detail = progress_detail(chunk)
                            if detail:
                                STORE.update(
                                    run_id,
                                    current_task_id=task["id"],
                                    current_task_detail=detail,
                                    completed_tasks=len(completed_task_ids),
                                    total_tasks=total_tasks,
                                    completed_task_ids=list(completed_task_ids),
                                    failed_task_ids=list(failed_task_ids),
                                    last_update_at=time.time(),
                                )

                        process = run_command(
                            command,
                            on_output=append_output,
                            on_heartbeat=update_progress,
                        )
                        if process.stderr:
                            log.write(process.stderr)
                        log.write(f"\nexit={process.returncode}\n\n")
                        log.flush()
                        if process.returncode != 0:
                            task_exit_code = process.returncode
                            if exit_code == 0:
                                exit_code = process.returncode
                            break
                    artifacts.extend(task.get("output_artifacts", []))
                    if task_exit_code != 0:
                        failed_task_ids.append(task["id"])
                        log.write(
                            f"Task {task['id']} failed with exit={task_exit_code}; continuing remaining tasks.\n\n"
                        )
                        log.flush()
                    else:
                        completed_task_ids.append(task["id"])
                    STORE.update(
                        run_id,
                        completed_tasks=len(completed_task_ids),
                        total_tasks=total_tasks,
                        completed_task_ids=list(completed_task_ids),
                        failed_task_ids=list(failed_task_ids),
                    )
        except Exception as exc:
            exit_code = 1
            error_message = str(exc)
            with log_path.open("a", encoding="utf-8") as log:
                log.write(f"\nDashboard runner error: {error_message}\n")
        finished = time.time()
        metrics = collect_run_metrics()
        changes = {
            "status": "completed" if exit_code == 0 else "failed",
            "exit_code": exit_code,
            "finished_at": finished,
            "duration_seconds": round(finished - started, 3),
            "artifacts": sorted(set(artifacts)),
            "metrics": metrics,
            "current_task_id": None,
            "current_task_detail": None,
            "completed_tasks": len(completed_task_ids),
            "total_tasks": total_tasks,
            "completed_task_ids": completed_task_ids,
            "failed_task_ids": failed_task_ids,
            "last_update_at": finished,
        }
        if error_message:
            changes["error"] = error_message
        STORE.update(run_id, **changes)


def load_analysis_artifact(name: str) -> Any:
    path = Path("target/analysis") / name
    if not path.exists():
        return None
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return None


def collect_hotspot_metrics(metrics: Dict[str, Any], hotspots: Any) -> None:
    if not isinstance(hotspots, list) or not hotspots:
        return
    scores = [item.get("quality_score") or item.get("score") or 0 for item in hotspots]
    metrics["quality_risk_count"] = sum(1 for score in scores if score >= 300)
    metrics["quality_worst_score"] = max(scores, default=0)


def collect_escape_hatch_metrics(metrics: Dict[str, Any], rows: Any) -> None:
    if not isinstance(rows, list):
        return
    metrics["escape_hatch_modules"] = len(rows)
    for output_key, source_key in (
        ("escape_hatch_uses", "total_count"),
        ("escape_hatch_unsafe_uses", "unsafe_count"),
        ("escape_hatch_deref_coercions", "deref_coercion_count"),
        ("escape_hatch_glob_imports", "glob_import_count"),
        ("escape_hatch_container_ref_returns", "container_ref_return_count"),
        ("escape_hatch_clippy_suppressions", "clippy_suppression_count"),
    ):
        metrics[output_key] = sum(int(item.get(source_key, 0)) for item in rows)


def collect_type_health_metrics(metrics: Dict[str, Any], rows: Any) -> None:
    if not isinstance(rows, list):
        return
    metrics["type_health_records"] = len(rows)
    risks = [float(item.get("structural_risk") or 0) for item in rows]
    metrics["type_health_worst_score"] = max(risks, default=0)
    metrics["type_health_risk_count"] = sum(1 for risk in risks if risk >= 40)


def collect_capacity_metrics(metrics: Dict[str, Any], speed: Any) -> None:
    if not isinstance(speed, dict):
        return
    triage_summary = speed.get("triage_summary")
    if isinstance(triage_summary, dict):
        critical = triage_summary.get("critical", 0)
        watch = triage_summary.get("watch", 0)
        metrics.update(
            {
                "capacity_critical": critical,
                "capacity_watch": watch,
                "capacity_risk_count": critical + watch,
            }
        )
        return

    summary = speed.get("summary")
    if isinstance(summary, dict):
        metrics["capacity_risk_count"] = (
            (summary.get("over_budget_latency") or 0)
            + (summary.get("near_failure_ceilings") or 0)
        )


def collect_summary_metrics(
    metrics: Dict[str, Any],
    summary: Any,
    mappings: Dict[str, str],
) -> None:
    if not isinstance(summary, dict):
        return
    for output_key, source_key in mappings.items():
        metrics[output_key] = summary.get(source_key, 0)


def collect_correctness_metrics(metrics: Dict[str, Any], correctness: Any) -> None:
    summary = correctness.get("summary") if isinstance(correctness, dict) else None
    if not isinstance(summary, dict):
        return
    total = summary.get("test_count") or 0
    failed = summary.get("failed") or 0
    unknown = summary.get("unknown") or 0
    metrics.update(
        {
            "tests_total": total,
            "tests_failed": failed,
            "tests_unknown": unknown,
            "tests_passed": max(0, total - failed - unknown),
        }
    )


def collect_run_metrics() -> Dict[str, Any]:
    """Read headline summary fields from key artifacts for dashboard trends."""
    metrics: Dict[str, Any] = {}
    artifacts = {
        name: load_analysis_artifact(name)
        for name in (
            "hotspots.json",
            "clones.json",
            "rust_escape_hatches.json",
            "type_health.json",
            "speed_efficiency_report.json",
            "performance_review.json",
            "correctness_review.json",
            "map.json",
            "project_code_metrics.json",
        )
    }

    collect_hotspot_metrics(metrics, artifacts["hotspots.json"])
    clones = artifacts["clones.json"]
    if isinstance(clones, list):
        metrics["clone_groups"] = len(clones)
    collect_escape_hatch_metrics(metrics, artifacts["rust_escape_hatches.json"])
    collect_type_health_metrics(metrics, artifacts["type_health.json"])
    collect_capacity_metrics(metrics, artifacts["speed_efficiency_report.json"])
    performance_review = artifacts["performance_review.json"]
    collect_summary_metrics(
        metrics,
        performance_review.get("summary") if isinstance(performance_review, dict) else None,
        {
            "performance_review_gaps": "coverage_gaps",
            "performance_missing_scale_targets": "missing_scale_targets",
            "performance_covered_scenarios": "covered_scenarios",
            "performance_failed_sources": "failed_source_artifacts",
        },
    )
    collect_correctness_metrics(metrics, artifacts["correctness_review.json"])
    map_doc = artifacts["map.json"]
    collect_summary_metrics(
        metrics,
        (map_doc.get("meta") or {}).get("summary") if isinstance(map_doc, dict) else None,
        {"map_bad": "bad", "map_warn": "warn", "map_good": "good"},
    )
    project_code = artifacts["project_code_metrics.json"]
    collect_summary_metrics(
        metrics,
        project_code.get("current") if isinstance(project_code, dict) else None,
        {
            "project_application_code_lines": "application",
            "project_test_code_lines": "test",
            "project_other_code_lines": "other",
            "project_total_code_lines": "total",
        },
    )
    return metrics


class DashboardHandler(SimpleHTTPRequestHandler):
    def end_headers(self) -> None:
        self.send_header("Access-Control-Allow-Origin", "http://localhost")
        self.send_header("Access-Control-Allow-Methods", "GET, POST, OPTIONS")
        self.send_header("Access-Control-Allow-Headers", "Content-Type")
        super().end_headers()

    def do_OPTIONS(self) -> None:
        self.send_response(204)
        self.end_headers()

    def do_GET(self) -> None:
        parsed = urlparse(self.path)
        path = parsed.path
        if path == "/api/catalog":
            json_response(self, 200, task_catalog())
            return
        if path == "/api/app-package":
            json_response(self, 200, app_package_payload())
            return
        if path == "/api/runs":
            json_response(self, 200, STORE.snapshot())
            return
        if path.startswith("/api/run/") and path.endswith("/log"):
            run_id = unquote(path[len("/api/run/") : -len("/log")])
            run = next((item for item in STORE.snapshot() if item["id"] == run_id), None)
            if not run or not run.get("log_path"):
                json_response(self, 404, {"error": "run log not found"})
                return
            log_path = Path(run["log_path"])
            if not log_path.exists():
                json_response(self, 404, {"error": "run log missing"})
                return
            body = log_path.read_text(encoding="utf-8").encode("utf-8")
            self.send_response(200)
            self.send_header("Content-Type", "text/plain; charset=utf-8")
            self.send_header("Cache-Control", "no-store")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)
            return
        super().do_GET()

    def do_POST(self) -> None:
        parsed = urlparse(self.path)
        path = parsed.path
        selector = ""
        if path == "/api/app-package/clear-buffers":
            payload = clear_app_package_buffers()
            status = 409 if (payload.get("clear_result") or {}).get("blocked") else 200
            json_response(self, status, payload)
            return
        if path == "/api/run/all":
            selector = "all"
        elif path.startswith("/api/run/category/"):
            selector = "category/" + unquote(path[len("/api/run/category/") :])
        elif path.startswith("/api/run/item/"):
            selector = "item/" + unquote(path[len("/api/run/item/") :])
        if not selector:
            json_response(self, 404, {"error": "unknown endpoint"})
            return
        tasks = selected_tasks(selector)
        if not tasks:
            json_response(self, 404, {"error": "no matching tasks"})
            return
        run_id = f"run-{time.strftime('%Y%m%d%H%M%S')}-{len(STORE.snapshot()) + 1}"
        run = {
            "id": run_id,
            "selector": selector,
            "task_ids": [task["id"] for task in tasks],
            "status": "queued",
            "created_at": time.time(),
            "exit_code": None,
            "duration_seconds": None,
            "artifacts": [],
            "current_task_id": None,
            "current_task_detail": None,
            "completed_tasks": 0,
            "total_tasks": len(tasks),
            "completed_task_ids": [],
            "last_update_at": time.time(),
        }
        active = STORE.try_add_queued(run)
        if active is not None:
            json_response(
                self,
                409,
                {
                    "error": "a dashboard refresh is already running",
                    "active_run_id": active.get("id"),
                    "active_status": active.get("status"),
                },
            )
            return
        thread = threading.Thread(
            target=run_task_batch, args=(run_id, selector, tasks), daemon=True
        )
        thread.start()
        json_response(self, 202, {"run_id": run_id, "status": "queued"})


def main() -> None:
    parser = argparse.ArgumentParser(description="Serve the Scratchpad measurement dashboard")
    parser.add_argument("--port", type=int, default=8000)
    args = parser.parse_args()
    server = ThreadingHTTPServer(("127.0.0.1", args.port), DashboardHandler)
    print(f"Measurement dashboard server listening on http://localhost:{args.port}/viewer/")
    server.serve_forever()


if __name__ == "__main__":
    main()
