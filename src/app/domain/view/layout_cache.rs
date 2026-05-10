use super::SearchHighlightState;
use eframe::egui;
use std::collections::VecDeque;
use std::ops::Range;
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct LayoutCacheKey {
    pub revision: u64,
    pub char_range: Range<usize>,
    pub font_family: String,
    pub font_size_bits: u32,
    pub wrap_width_bits: u32,
    pub word_wrap: bool,
    pub show_control_chars: bool,
    pub right_to_left_reading_order: bool,
    pub text_color: egui::Color32,
    pub dark_mode: bool,
    pub selection_highlight: Option<Range<usize>>,
    pub search_highlights: SearchHighlightState,
    pub replacement_preview_signature: u64,
}

#[derive(Clone)]
pub struct LayoutCacheEntry {
    pub key: LayoutCacheKey,
    pub galley: Arc<egui::Galley>,
    pub input_bytes: usize,
}

#[derive(Clone, Default)]
pub struct LayoutCache {
    entries: VecDeque<LayoutCacheEntry>,
    bytes: usize,
}

impl LayoutCache {
    const MAX_ENTRIES: usize = 32;
    const MAX_BYTES: usize = 16 * 1024 * 1024;

    pub fn get(&mut self, key: &LayoutCacheKey) -> Option<Arc<egui::Galley>> {
        let index = self.entries.iter().position(|entry| &entry.key == key)?;
        let entry = self.entries.remove(index)?;
        let galley = entry.galley.clone();
        self.entries.push_front(entry);
        Some(galley)
    }

    pub fn insert(&mut self, key: LayoutCacheKey, galley: Arc<egui::Galley>, input_bytes: usize) {
        if let Some(index) = self.entries.iter().position(|entry| entry.key == key)
            && let Some(existing) = self.entries.remove(index)
        {
            self.bytes = self.bytes.saturating_sub(existing.input_bytes);
            crate::app::memory_budget::record_free(
                crate::app::memory_budget::BudgetCategory::Layout,
                existing.input_bytes,
            );
        }
        self.bytes = self.bytes.saturating_add(input_bytes);
        crate::app::memory_budget::record_alloc(
            crate::app::memory_budget::BudgetCategory::Layout,
            input_bytes,
        );
        self.entries.push_front(LayoutCacheEntry {
            key,
            galley,
            input_bytes,
        });
        self.evict_over_budget();
    }

    pub fn retain_revision(&mut self, revision: u64) {
        let mut freed = 0usize;
        self.entries.retain(|entry| {
            if entry.key.revision == revision {
                true
            } else {
                freed = freed.saturating_add(entry.input_bytes);
                false
            }
        });
        self.bytes = self.entries.iter().map(|entry| entry.input_bytes).sum();
        if freed > 0 {
            crate::app::capacity_metrics::record_layout_cache_eviction(freed);
            crate::app::memory_budget::record_free(
                crate::app::memory_budget::BudgetCategory::Layout,
                freed,
            );
        }
    }

    pub fn clear(&mut self) {
        let freed = self.bytes;
        self.entries.clear();
        self.bytes = 0;
        if freed > 0 {
            crate::app::memory_budget::record_free(
                crate::app::memory_budget::BudgetCategory::Layout,
                freed,
            );
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    fn evict_over_budget(&mut self) {
        let mut freed = 0usize;
        let global_pressure = || {
            crate::app::memory_budget::over_budget(
                crate::app::memory_budget::BudgetCategory::Layout,
            )
        };
        while self.entries.len() > Self::MAX_ENTRIES
            || self.bytes > Self::MAX_BYTES
            || (global_pressure() && self.entries.len() > 1)
        {
            let Some(entry) = self.entries.pop_back() else {
                break;
            };
            self.bytes = self.bytes.saturating_sub(entry.input_bytes);
            freed = freed.saturating_add(entry.input_bytes);
        }
        if freed > 0 {
            crate::app::memory_budget::record_free(
                crate::app::memory_budget::BudgetCategory::Layout,
                freed,
            );
        }
    }
}

// Note: LayoutCache stays Clone (so EditorViewState can be cloned for tests
// and snapshotting). We deliberately do not implement Drop because it would
// conflict with Clone; budget accounting is recorded on eviction paths
// (`evict_over_budget`, `retain_revision`, replacing entries in `insert`).
// Stale accounting on rare full-cache drop is acceptable for a soft signal.
