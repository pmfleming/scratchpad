# Scratchpad Measurement Boundary

As of May 19, 2026, Scratchpad no longer owns the dashboard or the measurement
wrapper scripts. This repo is the measured Rust application. The measurement
and dashboard repos live beside it:

- `rust-quality-lens`: reusable Rust quality, correctness, and architecture-map
  JSON producers.
- `scratchpad-performance-lens`: Scratchpad-specific performance, overview, and
  telemetry JSON producers.
- `project-management-board`: Vite, React, and TypeScript dashboard host, task
  catalog, refresh API, and run-log store.

Scratchpad still contains Rust probes and benchmarks that compile against the
application crate. Generated artifacts continue to be written under
`target/analysis/` so the sibling dashboard can read stable JSON contracts.

## Local Dashboard

From a sibling checkout:

```powershell
cd ..\project-management-board
npm install
npm run dev
```

Then open `http://127.0.0.1:5173/`.

The dashboard assumes this checkout layout by default:

```text
D:\Code\scratchpad
D:\Code\rust-quality-lens
D:\Code\scratchpad-performance-lens
D:\Code\project-management-board
```

Override paths with `SCRATCHPAD_ROOT`, `RUST_QUALITY_LENS_ROOT`, or
`SCRATCHPAD_PERFORMANCE_LENS_ROOT` when needed.

## Scratchpad-Owned Measurement Surface

Scratchpad keeps only the Rust-side pieces that must compile against the app:

- `src/bin/capacity_probe.rs`
- `src/bin/frame_metrics.rs`
- `src/bin/resource_probe.rs` and its modules
- `src/bin/profile_*.rs`
- `benches/search_speed.rs`
- `benches/frame_budget.rs`
- `benches/search_benchmark_targets.json`

Everything else in the measurement workflow should be maintained in the sibling
repos above.
