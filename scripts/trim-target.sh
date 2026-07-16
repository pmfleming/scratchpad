#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: scripts/trim-target.sh [--dry-run]

Clean the Cargo dev profile when the target directory exceeds the configured
size limit. Release builds and target/analysis are left intact.

Environment:
  SCRATCHPAD_TARGET_MAX_GIB  Limit in GiB (default: 10; 0 disables trimming)
  CARGO_TARGET_DIR           Cargo target directory (default: ./target)
EOF
}

dry_run=false
case "${1:-}" in
  "") ;;
  --dry-run) dry_run=true ;;
  -h|--help) usage; exit 0 ;;
  *) usage >&2; exit 2 ;;
esac

max_gib="${SCRATCHPAD_TARGET_MAX_GIB:-10}"
if [[ ! "$max_gib" =~ ^[0-9]+$ ]]; then
  echo "SCRATCHPAD_TARGET_MAX_GIB must be a non-negative integer" >&2
  exit 2
fi
if (( max_gib == 0 )); then
  exit 0
fi

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
project_root="$(cd -- "$script_dir/.." && pwd)"
target_dir="${CARGO_TARGET_DIR:-target}"
if [[ "$target_dir" != /* ]]; then
  target_dir="$project_root/$target_dir"
fi
if [[ ! -d "$target_dir" ]]; then
  exit 0
fi

size_kib="$(du -sk -- "$target_dir" | awk '{print $1}')"
limit_kib=$((max_gib * 1024 * 1024))
if (( size_kib <= limit_kib )); then
  exit 0
fi

size_human="$(du -sh -- "$target_dir" | awk '{print $1}')"
echo "Cargo target is $size_human (limit: ${max_gib} GiB); cleaning dev artifacts."

clean_command=(
  cargo clean
  --manifest-path "$project_root/Cargo.toml"
  --target-dir "$target_dir"
  --profile dev
)
if [[ "$dry_run" == true ]]; then
  printf 'Would run:'
  printf ' %q' "${clean_command[@]}"
  printf '\n'
  exit 0
fi

"${clean_command[@]}"

remaining_human="$(du -sh -- "$target_dir" | awk '{print $1}')"
echo "Cargo target trimmed to $remaining_human; release and analysis artifacts were kept."
