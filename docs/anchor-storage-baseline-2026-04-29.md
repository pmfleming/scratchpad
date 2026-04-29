# Anchor Storage Performance Baseline

Date: 2026-04-29

Command:

```powershell
cargo bench --bench anchor_storage
```

Raw output:

```text
target/analysis/anchor_storage_baseline_2026-04-29.txt
```

## Summary

The benchmark completed successfully in the optimized bench profile. Insert and remove costs remain below one millisecond through 10,000 live anchors in this workload.

The curve is mostly flat from 1 through 1,000 anchors, then rises at 10,000 anchors. That is worth watching, but this baseline does not show a catastrophic full-anchor scan in ordinary anchor counts.

## Criterion Mean Estimates

| Operation | Live anchors | Mean | 95% confidence interval |
| --- | ---: | ---: | ---: |
| insert | 1 | 25.57 us | 24.10-27.23 us |
| insert | 10 | 31.00 us | 29.44-32.63 us |
| insert | 100 | 33.91 us | 31.40-37.27 us |
| insert | 1,000 | 43.67 us | 41.96-45.40 us |
| insert | 10,000 | 177.83 us | 166.49-190.16 us |
| remove | 1 | 184.56 us | 178.37-191.56 us |
| remove | 10 | 193.48 us | 187.54-199.92 us |
| remove | 100 | 203.56 us | 198.93-208.18 us |
| remove | 1,000 | 230.96 us | 218.83-243.73 us |
| remove | 10,000 | 390.40 us | 368.91-411.14 us |

## Build Notes

Cargo emitted existing unused-variable warnings in `src/app/ui/widget_ids.rs` during the bench build. They do not block the benchmark, but they should be cleaned up separately if this warning noise starts hiding new performance-build warnings.

Criterion reported that `gnuplot` is not installed and used the plotters backend. This only affects graph generation, not the timing baseline.

## Follow-Up

- Keep this file as the first baseline for future anchor-storage comparisons.
- If future 10,000-anchor timings climb sharply, inspect anchor redistribution around edited leaves before changing editor-level anchoring behavior.
- The next anchor-plan item is a combined lifecycle leak test that exercises every runtime owner kind.