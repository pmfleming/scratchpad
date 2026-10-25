#!/usr/bin/env python3
"""Compare archived lens runs without relaxing any benchmark budgets."""
import argparse
import json
from pathlib import Path

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("baseline", type=Path)
parser.add_argument("current", type=Path)
parser.add_argument("--require-all-promises", action="store_true")
args = parser.parse_args()


def read_run(path):
    # The initial baseline predates the combined quality/performance wrapper.
    marker = path / "check.exit"
    if not marker.exists():
        marker = path / "performance.exit"
    if marker.read_text().strip() != "0":
        raise SystemExit(f"incomplete/failed lens run: {path}")
    return json.loads((path / "performance/performance_review.json").read_text())


def session_rows(review):
    scenario = next(s for s in review["scenarios"] if s["title"] == "Session Persistence Restore")
    return {row["name"]: row for row in scenario["evidence"]["latency"]}


baseline, current = read_run(args.baseline), read_run(args.current)
before, after = session_rows(baseline), session_rows(current)
checks = []
for name in ("session_persist_latency/10000", "session_restore_latency/10000",
             "session_restore_background_completion/10000"):
    old, new = before[name], after[name]
    passed = (new["measurement_status"] == "valid"
              and (not new["threshold_authoritative"] or new["budget_status"] == "pass")
              and new["mean_ns"] <= old["mean_ns"]
              and new["mean_confidence_interval_ns"]["upper_bound"]
              <= old["mean_confidence_interval_ns"]["upper_bound"])
    checks.append(passed)
    print(f"{'PASS' if passed else 'FAIL'} {name}: {old['mean_ns']/1e6:.3f} -> "
          f"{new['mean_ns']/1e6:.3f} ms ({new['mean_ns']/old['mean_ns']:.3f}x baseline)")

old_health = {s["title"]: s["promise_health"] for s in baseline["scenarios"]}
all_pass = True
for scenario in current["scenarios"]:
    health = scenario["promise_health"]
    all_pass &= health == "pass"
    print(f"PROMISE {scenario['title']}: {old_health[scenario['title']]} -> {health}")
print(f"Session latency acceptance: {'PASS' if all(checks) else 'FAIL'}")
print(f"All-project promise compliance: {'PASS' if all_pass else 'NOT MET'}")
raise SystemExit(0 if all(checks) and (all_pass or not args.require_all_promises) else 1)
