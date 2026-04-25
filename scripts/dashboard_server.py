import argparse
import json
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


class RunStore:
    def __init__(self) -> None:
        self.lock = threading.Lock()
        self.runs: List[Dict[str, Any]] = self._load()

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


STORE = RunStore()


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


def run_task_batch(run_id: str, tasks: List[Dict[str, Any]]) -> None:
    LOG_DIR.mkdir(parents=True, exist_ok=True)
    log_path = LOG_DIR / f"{run_id}.log"
    started = time.time()
    STORE.update(run_id, status="running", started_at=started, log_path=log_path.as_posix())
    exit_code = 0
    artifacts: List[str] = []
    with log_path.open("w", encoding="utf-8") as log:
        for task in tasks:
            log.write(f"## {task['id']} - {task['title']}\n")
            log.flush()
            for raw_command in task["commands"]:
                command = normalize_command(list(raw_command))
                log.write(f"$ {' '.join(command)}\n")
                log.flush()
                process = subprocess.run(command, capture_output=True, text=True)
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
    finished = time.time()
    STORE.update(
        run_id,
        status="completed" if exit_code == 0 else "failed",
        exit_code=exit_code,
        finished_at=finished,
        duration_seconds=round(finished - started, 3),
        artifacts=sorted(set(artifacts)),
    )


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
        STORE.add(
            {
                "id": run_id,
                "selector": selector,
                "task_ids": [task["id"] for task in tasks],
                "status": "queued",
                "created_at": time.time(),
                "exit_code": None,
                "duration_seconds": None,
                "artifacts": [],
            }
        )
        thread = threading.Thread(target=run_task_batch, args=(run_id, tasks), daemon=True)
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
