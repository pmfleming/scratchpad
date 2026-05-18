import sys
from pathlib import Path


def main() -> None:
    lens_root = Path(__file__).resolve().parents[2] / "scratchpad-performance-lens"
    src_root = lens_root / "src"
    if not src_root.exists():
        raise SystemExit(
            f"scratchpad-performance-lens was not found at {lens_root}. "
            "Set up the sibling repo or update scripts/splens.py."
        )

    sys.path.insert(0, str(src_root))
    if "--config" not in sys.argv:
        sys.argv.extend(["--config", "splens.toml"])

    from scratchpad_performance_lens.cli import main as lens_main

    lens_main()


if __name__ == "__main__":
    main()
