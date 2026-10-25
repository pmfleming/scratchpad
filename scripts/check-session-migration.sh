#!/usr/bin/env bash
# Preserve fresh, unmodified lens evidence for each migration phase.
set -uo pipefail
cd "$(dirname "$0")/.."
phase=${1:?usage: check-session-migration.sh PHASE}
case "$phase" in *[!a-zA-Z0-9_-]*) echo 'invalid phase name' >&2; exit 2;; esac
out="target/turso-migration/$phase"
mkdir -p "$out"
quality=../rust-quality-lens/target/debug/rqlens
performance=../scratchpad-performance-lens/target/release/splens
status=0
for metric in hotspots clones locality leverage reliability; do
    "$quality" measure "$metric" --config rqlens.toml > "$out/$metric.log" 2>&1 || status=1
done
cp -a target/analysis "$out/quality"
"$performance" measure all --config splens.toml > "$out/performance.log" 2>&1 || status=1
cp -a target/analysis "$out/performance"
printf '%s\n' "$status" > "$out/check.exit"
# A successful producer exit is not proof that every promise passed.
exit "$status"
