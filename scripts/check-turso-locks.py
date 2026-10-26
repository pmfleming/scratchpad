#!/usr/bin/env python3
"""CI policy: engine upgrades require matching app/probe locks and contract review."""
from pathlib import Path
import tomllib

ROOT = Path(__file__).resolve().parent.parent
EXPECTED = {
    "turso": "0.8.0",
    "turso_core": "0.8.3",
    "turso_ext": "0.8.3",
    "turso_macros": "0.8.3",
    "turso_parser": "0.8.3",
    "turso_sdk_kit": "0.8.3",
    "turso_sdk_kit_macros": "0.8.3",
    "turso_sync_engine": "0.8.3",
    "turso_sync_sdk_kit": "0.8.3",
}


def engine_identity(lockfile):
    packages = tomllib.loads(lockfile.read_text())["package"]
    identity = {}
    for name, version in EXPECTED.items():
        matches = [package for package in packages if package["name"] == name]
        if len(matches) != 1 or matches[0]["version"] != version:
            raise SystemExit(f"{lockfile}: expected exactly {name} {version}; review the engine contract before upgrading")
        package = matches[0]
        if not package.get("source", "").startswith("registry+") or not package.get("checksum"):
            raise SystemExit(f"{lockfile}: {name} must identify a checksummed registry artifact")
        identity[name] = (version, package["source"], package["checksum"])
    return identity


def main():
    app = engine_identity(ROOT / "Cargo.lock")
    probe = engine_identity(ROOT / "tools/turso-contract/Cargo.lock")
    if app != probe:
        raise SystemExit("Application and contract probe engine sources/checksums differ")
    print("Turso bindings 0.8.0, engine/SDK 0.8.3: matching lockfile identities")


if __name__ == "__main__":
    main()
