#!/usr/bin/env bash
set -euo pipefail

host_target="$(rustc -vV | awk '/^host:/ { print $2 }')"
if [[ $# -eq 0 ]]; then
  requested_targets=(x86_64-unknown-linux-gnu x86_64-pc-windows-gnu)
else
  requested_targets=("$@")
fi

target_is_available() {
  local target="$1"

  if [[ "$target" == "$host_target" ]]; then
    return 0
  fi

  if command -v rustup >/dev/null 2>&1; then
    rustup target list --installed | grep -Fxq "$target"
    return $?
  fi

  return 1
}

for target in "${requested_targets[@]}"; do
  if target_is_available "$target"; then
    echo "==> cargo check --target $target"
    cargo check --target "$target"
  else
    echo "==> skipping $target; target is not installed for this toolchain" >&2
  fi
done
