//! Opt-in window/repaint diagnostics. No document text or key events are recorded.

use std::io::Write;
use std::sync::OnceLock;
use std::time::Instant;

static ENABLED: OnceLock<bool> = OnceLock::new();
static STARTED: OnceLock<Instant> = OnceLock::new();

/// Install logging before eframe creates its window. Trace output goes to stderr.
pub fn initialize() {
    if enabled() {
        STARTED.get_or_init(Instant::now);
        super::install_logger();
    }
}

pub(super) fn enabled() -> bool {
    *ENABLED.get_or_init(|| std::env::var_os("SCRATCHPAD_TRACE_WINDOW").is_some_and(|v| v == "1"))
}

pub(super) fn captures(target: &str) -> bool {
    enabled() && (target.starts_with("eframe::native") || target == "scratchpad::window")
}

pub(super) fn record(record: &log::Record<'_>) {
    let elapsed = STARTED.get_or_init(Instant::now).elapsed();
    let _ = writeln!(
        std::io::stderr().lock(),
        "[window +{:.6}s] {} {}: {}",
        elapsed.as_secs_f64(),
        record.level(),
        record.target(),
        record.args()
    );
}
