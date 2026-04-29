import argparse
import json
import os
import subprocess
import sys
import threading
import time
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Any, Dict, List
from urllib.parse import unquote, urlparse

from measurement_catalog import build_catalog

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


def terminate_process_tree(process: subprocess.Popen[str]) -> None:
    if process.poll() is not None:
        return
    if os.name == "nt":
        subprocess.run(
            ["taskkill", "/PID", str(process.pid), "/T", "/F"],
            capture_output=True,
            text=True,
        )
        return
    process.kill()


def run_command(command: List[str]) -> subprocess.CompletedProcess[str]:
    creationflags = (
        getattr(subprocess, "CREATE_NEW_PROCESS_GROUP", 0) if os.name == "nt" else 0
    )
    process = subprocess.Popen(
        command,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        creationflags=creationflags,
    )
    try:
        stdout, stderr = process.communicate(timeout=COMMAND_TIMEOUT_SECONDS)
        return subprocess.CompletedProcess(command, process.returncode, stdout, stderr)
    except subprocess.TimeoutExpired:
        terminate_process_tree(process)
        try:
            stdout, stderr = process.communicate(timeout=10)
        except subprocess.TimeoutExpired:
            stdout, stderr = "", ""
        stderr = (
            stderr
            + f"\nCommand timed out after {COMMAND_TIMEOUT_SECONDS} seconds and was stopped.\n"
        )
        return subprocess.CompletedProcess(command, 124, stdout, stderr)


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
        total_tasks = len(tasks)
        try:
            with log_path.open("w", encoding="utf-8") as log:
                if selector == "all":
                    write_stale_cleanup_log(log, cleanup_stale_measurement_processes())
                    log.flush()
                for task in tasks:
                    STORE.update(
                        run_id,
                        current_task_id=task["id"],
                        completed_tasks=len(completed_task_ids),
                        total_tasks=total_tasks,
                        completed_task_ids=list(completed_task_ids),
                    )
                    log.write(f"## {task['id']} - {task['title']}\n")
                    log.flush()
                    for raw_command in task["commands"]:
                        command = normalize_command(list(raw_command))
                        log.write(f"$ {' '.join(command)}\n")
                        log.flush()
                        process = run_command(command)
                        if process.stdout:
                            log.write(process.stdout)
                        if process.stderr:
                            log.write(process.stderr)
                        log.write(f"\nexit={process.returncode}\n\n")
                        log.flush()
                        if process.returncode != 0:
                            exit_code = process.returncode
                            break
                    artifacts.extend(task.get("output_artifacts", []))
                    if exit_code != 0:
                        break
                    completed_task_ids.append(task["id"])
                    STORE.update(
                        run_id,
                        completed_tasks=len(completed_task_ids),
                        total_tasks=total_tasks,
                        completed_task_ids=list(completed_task_ids),
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
            "completed_tasks": len(completed_task_ids),
            "total_tasks": total_tasks,
            "completed_task_ids": completed_task_ids,
        }
        if error_message:
            changes["error"] = error_message
        STORE.update(run_id, **changes)


def collect_run_metrics() -> Dict[str, Any]:
    """Read headline summary fields from key artifacts so the dashboard
    can plot sparklines and deltas without re-fetching old artifacts."""
    base = Path("target/analysis")
    metrics: Dict[str, Any] = {}

    def _load(name: str) -> Any:
        path = base / name
        if not path.exists():
            return None
        try:
            return json.loads(path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError):
            return None

    hotspots = _load("hotspots.json") or []
    if isinstance(hotspots, list) and hotspots:
        bad = sum(1 for h in hotspots if (h.get("quality_score") or h.get("score") or 0) >= 600)
        warn = sum(
            1
            for h in hotspots
            if 300 <= (h.get("quality_score") or h.get("score") or 0) < 600
        )
        metrics["quality_risk_count"] = bad + warn
        metrics["quality_worst_score"] = max(
            (h.get("quality_score") or h.get("score") or 0 for h in hotspots),
            default=0,
        )

    clones = _load("clones.json") or []
    if isinstance(clones, list):
        metrics["clone_groups"] = len(clones)

    speed = _load("speed_efficiency_report.json") or {}
    triage_summary = speed.get("triage_summary") if isinstance(speed, dict) else None
    if isinstance(triage_summary, dict):
        metrics["capacity_critical"] = triage_summary.get("critical", 0)
        metrics["capacity_watch"] = triage_summary.get("watch", 0)
        metrics["capacity_risk_count"] = (
            triage_summary.get("critical", 0) + triage_summary.get("watch", 0)
        )
    else:
        summary = speed.get("summary") if isinstance(speed, dict) else None
        if isinstance(summary, dict):
            metrics["capacity_risk_count"] = (
                (summary.get("over_budget_latency") or 0)
                + (summary.get("near_failure_ceilings") or 0)
            )

    correctness = _load("correctness_review.json") or {}
    summary = correctness.get("summary") if isinstance(correctness, dict) else None
    if isinstance(summary, dict):
        total = summary.get("test_count") or 0
        failed = summary.get("failed") or 0
        unknown = summary.get("unknown") or 0
        metrics["tests_total"] = total
        metrics["tests_failed"] = failed
        metrics["tests_unknown"] = unknown
        metrics["tests_passed"] = max(0, total - failed - unknown)

    map_doc = _load("map.json") or {}
    map_summary = (map_doc.get("meta") or {}).get("summary") if isinstance(map_doc, dict) else None
    if isinstance(map_summary, dict):
        metrics["map_bad"] = map_summary.get("bad", 0)
        metrics["map_warn"] = map_summary.get("warn", 0)
        metrics["map_good"] = map_summary.get("good", 0)

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
            "completed_tasks": 0,
            "total_tasks": len(tasks),
            "completed_task_ids": [],
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
