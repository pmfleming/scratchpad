# Scratchpad Measurement Tools

Scratchpad includes a small analysis toolchain for maintainability, clone drift, benchmark visibility, and architecture mapping.

## Scripts

- `scripts/hotspots.py`: complexity and maintainability analysis
- `scripts/slowspots.py`: benchmark-oriented performance and degradation analysis
- `scripts/clone_alert.py`: token-based clone and duplication analysis
- `scripts/map.py`: architecture map output enriched with dependencies and analysis signals
- `scripts/ci.ps1`: local and CI entry point for formatting, linting, tests, and analysis checks
- `scripts/open-overview.ps1`: launches the static viewer against the analysis output

## Output Location

The analysis scripts write JSON artifacts under `target/analysis/`.

The static viewer under `viewer/` reads those files directly.

## Common Commands

```powershell
.venv\Scripts\python.exe scripts\hotspots.py --mode cli --paths src --scope all
.venv\Scripts\python.exe scripts\clone_alert.py --mode cli --paths src
.venv\Scripts\python.exe scripts\clone_alert.py --mode analysis --paths src --output target/analysis/clones.json
.venv\Scripts\python.exe scripts\clone_alert.py --mode analysis --paths src --engine all --output target/analysis/clones.json
.venv\Scripts\python.exe scripts\hotspots.py --mode visibility --paths src
.venv\Scripts\python.exe scripts\slowspots.py --mode analysis --skip-bench --output target/analysis/slowspots.json
.venv\Scripts\python.exe scripts\map.py --mode visibility
.venv\Scripts\python.exe scripts\map.py --refresh --mode visibility
```

## Viewer

Start a simple local server:

```powershell
.venv\Scripts\python.exe -m http.server 8000
```

Then browse to `http://localhost:8000/viewer/`.

## Overview Launcher

Fast mode:

```powershell
powershell -ExecutionPolicy Bypass -File scripts\open-overview.ps1
```

Refresh mode:

```powershell
powershell -ExecutionPolicy Bypass -File scripts\open-overview.ps1 -Refresh
```

CloneCheck mode:

```powershell
powershell -ExecutionPolicy Bypass -File scripts\open-overview.ps1 -CloneCheck
```

## Notes

- The Python tools produce JSON rather than HTML.
- The viewer is intentionally decoupled from the analysis scripts.
- The current workflow is aimed at local review and CI visibility rather than polished end-user reporting.
