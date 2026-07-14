#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 1 || -z "$1" ]]; then
    echo "Usage: $0 VERSION" >&2
    exit 2
fi

version="$1"
package_name="scratchpad-v${version}-linux-x86_64"
package_root="dist/${package_name}"
archive="dist/${package_name}.tar.gz"

if [[ ! -x target/release/scratchpad ]]; then
    echo "target/release/scratchpad is missing; run cargo build --release --locked first" >&2
    exit 1
fi

binary_version="$(target/release/scratchpad /version)"
if [[ "$binary_version" != "scratchpad $version" ]]; then
    echo "Requested package version $version does not match binary output: $binary_version" >&2
    exit 1
fi

rm -rf "$package_root" "$archive" "${archive}.sha256"
install -Dm755 target/release/scratchpad "$package_root/bin/scratchpad"
install -Dm755 packaging/linux/scratchpad-hyprland "$package_root/bin/scratchpad-hyprland"
install -Dm644 packaging/linux/scratchpad.desktop \
    "$package_root/share/applications/scratchpad.desktop"
install -Dm644 assets/Scratchpad.svg \
    "$package_root/share/icons/hicolor/scalable/apps/scratchpad.svg"
install -Dm644 packaging/linux/README.md "$package_root/README.md"

tar -C dist -czf "$archive" "$package_name"
(
    cd dist
    sha256sum "$(basename "$archive")"
) > "${archive}.sha256"

printf 'Created %s\nCreated %s\n' "$archive" "${archive}.sha256"
