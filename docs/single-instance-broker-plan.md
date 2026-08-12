# Single-instance broker implementation and measurement plan

## Goal

Make the desktop Scratchpad binary a single-instance application. A later invocation forwards its launch request to the existing process and exits without loading or writing settings or session state.

Library-created applications, benchmarks, profiles, and probes remain broker-free.

## Baseline (phase 0)

Captured on branch `feature/single-instance-broker` before broker code was added.

### Performance lens

- All seven project promises: pass.
- Performance-review budget misses: 0.
- `ui_render_frame_120hz/steady_workspace`: 191.02 us mean.
- `session_restore_latency/10000`: 14.87 ms mean.
- `session_restore_background_completion/10000`: 156.84 ms mean.
- `session_persist_latency/10000`: 67.79 ms Criterion mean in the baseline run.
- The baseline run reported normal Criterion variance in several unrelated search and session comparisons, but no authoritative threshold failure.

The complete pre-change analysis artifacts are retained locally under `target/broker-baseline/analysis`; generated analysis remains excluded from source control.

### Quality lens

- Authoritative verification completed successfully with the checked-in `rqlens.toml` scope.
- `rust_practices.json` reported conformant, with no failed errors.
- Existing architecture threshold and static-finding reports are baseline evidence, not broker regressions.
- Production broker code may not add `unsafe`, `unwrap`, `expect`, or `panic!` paths.

## Performance constraints

- Election happens before settings/session access and before eframe initialization.
- The primary uses one blocking listener thread; no IPC polling loop runs while idle.
- The listener validates and decodes requests away from the UI thread.
- A bounded channel carries accepted launch requests to the app.
- The listener calls `egui::Context::request_repaint`; it does not mutate app state.
- Broker-free constructors remain unchanged for benchmarks and probes.
- No changes are made to session capture, persistence, restore, search, editor, or rendering algorithms.

## Quality constraints

- Keep protocol, election/transport, and app application logic in narrow modules.
- Keep the protocol versioned and length-prefixed with strict request and queue limits.
- Preserve non-Unicode paths in the protocol.
- Use an OS-held file lock, not a PID-file ownership claim.
- Return explicit errors for malformed, oversized, busy, and incompatible requests.
- Keep transport errors recoverable and avoid production panic paths.

## Delivery phases

1. Typed protocol, launch policy, and path normalization.
2. Race-safe election and local transport, before persistent-store access.
3. Event-driven app inbox, restore deferral, and window activation.
4. Process integration tests and broker-specific latency coverage.
5. Full performance-lens and rust-quality-lens release gates.

Each phase is committed separately so measurements and architecture effects are attributable.

## Final verification (phase 5)

Completed after the process integration phase:

- `cargo check --all-targets --all-features`: pass.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`: pass.
- `cargo test --workspace`: pass.
- Performance lens slowspots: authoritative thresholds pass on the confirmation run.
- Frame metrics: 4 MiB event-to-tessellation p99 0.34 ms against the 8.33 ms budget.
- Performance review: all seven promises pass; budget misses remain 0.
- Rust quality lens `verify`, `measure all`, and `check`: pass.
- Policy-controlled production findings: 14 `expect` (limit 14), 3 `panic` (limit 3), and 0 undocumented unsafe; the broker added no production occurrences.

Criterion showed substantial host variance between repeated runs, including unrelated viewport, paste, and search rows moving in both directions. A confirmation run cleared all authoritative thresholds, while session persistence/restore remained within budget. Broker-free profile constructors do not start election, IPC, or listener threads.
