//! Local scrolling primitives for the editor.
//!
//! Concepts adapted from egui's `ScrollArea`
//! (https://github.com/emilk/egui/blob/master/crates/egui/src/containers/scroll_area.rs,
//! MIT/Apache-2.0). This is a focused editor-specific subset, not a general
//! container: it owns persistent scroll state, viewport callback rendering,
//! content-size based clamping, scrollbar visibility, and explicit input source
//! gating. Higher-level concepts (display rows, scroll anchors, cursor reveal,
//! intents) live in Phase 2's `ScrollManager`.

mod anchor;
mod area;
mod display;
mod intent;
mod manager;
mod metrics;
mod source;
mod state;
mod target;

pub use anchor::ScrollAnchor;
pub use area::{ScrollArea, ScrollAreaOutput};
pub use display::{DisplayPoint, DisplayRow, DisplaySnapshot, ViewportSlice};
pub use intent::{Axis, ScrollIntent};
pub use manager::{
    ScrollManager, display_aware_anchor_to_row, naive_anchor_to_row, naive_row_to_anchor,
};
pub use metrics::{ContentExtent, ViewportMetrics};
pub use source::ScrollSource;
pub use state::ScrollState;
pub use target::{ScrollAlign, ScrollTarget, ScrollbarPolicy};
