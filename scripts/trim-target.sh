#!/usr/bin/env bash
# Compatibility entry point; SCRATCHPAD_TARGET_MAX_GIB remains a fallback alias.
set -euo pipefail
project_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
exec python3 "$project_root/scripts/trim-build-cache.py" "$@"
