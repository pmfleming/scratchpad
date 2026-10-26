#!/usr/bin/env python3
"""Run the lens-owned resource scenario in independent, isolated processes."""
import argparse
import datetime
import hashlib
import json
import pathlib
import platform
import statistics
import subprocess

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--binary", type=pathlib.Path, default=pathlib.Path("target/release/resource_probe"))
parser.add_argument("--output", type=pathlib.Path, required=True)
parser.add_argument("--launches", type=int, default=3)
args = parser.parse_args()
if not 1 <= args.launches <= 20:
    parser.error("launches must be between 1 and 20")
args.output.mkdir(parents=True, exist_ok=False)
binary = args.binary.resolve(strict=True)
metadata = {"utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
            "binary": str(binary), "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
            "platform": platform.platform(), "launches": args.launches,
            "io_definition": "process write_bytes delta; WAL plus automatic checkpoint writes, not device traffic"}
(args.output / "metadata.json").write_text(json.dumps(metadata, indent=2) + "\n")
summaries = []
for launch in range(args.launches):
    run = subprocess.run([str(binary), "--large-dirty-save"], capture_output=True, text=True, timeout=300)
    (args.output / f"run-{launch}.jsonl").write_text(run.stdout)
    (args.output / f"run-{launch}.stderr").write_text(run.stderr)
    run.check_returncode()
    samples = [json.loads(line) for line in run.stdout.splitlines()]
    if len(samples) != 9 or any(s["status"] != "ok" for s in samples):
        raise RuntimeError("missing or failed save samples")
    writes = [s["process_resources"]["write_bytes"] for s in samples]
    summaries.append({"launch": launch,
                      "median_save_ms": statistics.median(s["phase_elapsed_ns"]["save_ns"] / 1e6 for s in samples),
                      "median_write_bytes": statistics.median(writes) if all(v is not None for v in writes) else None,
                      "write_bytes_available": all(v is not None for v in writes)})
(args.output / "summary.json").write_text(json.dumps(summaries, indent=2) + "\n")
print(json.dumps(summaries, indent=2))
