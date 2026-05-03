# Error Log

Scratchpad writes application diagnostics to `error.log` in the same root directory used for session and settings state. In the default runtime configuration this is the app's `scratchpad` temp/session directory; tests that inject a custom `SessionStore` write under that store's root.

The log is append-only JSON Lines. Each line is one diagnostic record with a timestamp, diagnostic kind, message, optional source, optional widget ID, optional rectangle data, and optional frame number.

Captured diagnostic kinds include:

- `session_started`: written when diagnostics are initialized.
- `egui_id_conflict`: written when the app-owned widget ID registry sees a duplicate tracked egui ID in the same frame.
- `egui_warning`: written for captured egui/eframe warning log records and ID-related warning text.
- `panic`: written from the panic hook before delegating to Rust's previous panic hook.
- `io` and `other`: reserved categories for broader diagnostics.

Logging is best effort. Failure to create or append to `error.log` must not crash the app or interrupt rendering.
