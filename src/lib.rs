//! Shared implementation of the Scratchpad desktop editor.
//!
//! The desktop binary, integration tests, benchmarks and profiling binaries share
//! these modules and [`ScratchpadApp`]. Public visibility supports those consumers;
//! it is not a certification of a stable embedding API or untested platforms.
//!
//! [`app`] contains the editor implementation. [`profile`] exposes explicit
//! profiling workloads and measurement helpers; its process-counter measurements
//! include background and sampling overhead, not isolated per-operation costs.

#![forbid(unsafe_code)]

/// Editor domain, state, services, commands and user interface.
pub mod app;
/// Profiling workloads and best-effort process measurement support.
pub mod profile;

/// The eframe application type shared with test and profiling harnesses.
pub use app::ScratchpadApp;
