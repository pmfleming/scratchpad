use crate::app::{
    app_state::ScratchpadApp,
    domain::BufferState,
    services::session_store::timeline::{TimelineCursor, TimelineRow},
};
use std::collections::{BTreeMap, HashMap};

#[derive(Default)]
struct DocumentRows {
    baseline: u64,
    len: usize,
    last: u64,
    name: String,
    path: Option<String>,
    keys: HashMap<u64, TimelineCursor>,
}

/// Ordered metadata for hydrated documents. Coalescing updates one stable row;
/// navigation does not rebuild the index or touch any closed edit tree.
#[derive(Default)]
pub(super) struct LiveRows {
    pub rows: BTreeMap<TimelineCursor, TimelineRow>,
    pub retired: BTreeMap<TimelineCursor, TimelineRow>,
    documents: HashMap<String, DocumentRows>,
    search: String,
}

impl LiveRows {
    pub fn update(&mut self, app: &ScratchpadApp, search: &str) -> (Vec<String>, bool) {
        let mut changed = false;
        if self.search != search {
            *self = Self {
                search: search.to_owned(),
                ..Self::default()
            };
            changed = true;
        }
        let mut loaded = Vec::new();
        for (index, tab) in app.tab_manager.tabs.as_slice().iter().enumerate() {
            if app.tab_manager.cold_session_tabs().contains_key(&index) {
                continue;
            }
            for buffer in tab.buffers().filter(|b| !b.is_loading_preview) {
                loaded.push(buffer.temp_id.clone());
                changed |= self.update_buffer(buffer, search);
            }
        }
        loaded.sort();
        loaded.dedup();
        changed |= self.retire_absent(&loaded);
        (loaded, changed)
    }

    fn update_buffer(&mut self, buffer: &BufferState, search: &str) -> bool {
        let id = &buffer.temp_id;
        let entries = buffer.document().history_entries();
        let baseline = entries.first().map_or(0, |e| e.global_seq);
        let last = entries.last().map_or(0, |e| e.global_seq);
        // UTF-8 paths stay borrowed on the unchanged-frame path.
        let path = buffer.path.as_deref().map(std::path::Path::to_string_lossy);
        if self.documents.get(id).is_some_and(|cache| {
            (cache.baseline, cache.len, cache.last) == (baseline, entries.len(), last)
                && cache.name == buffer.name
                && cache.path.as_deref() == path.as_deref()
        }) {
            return false;
        }
        let cache = self.documents.entry(id.clone()).or_default();
        let reset = cache.baseline != baseline
            || entries.len() < cache.len
            || cache.name != buffer.name
            || cache.path.as_deref() != path.as_deref();
        let start = if reset {
            0
        } else {
            cache.len.saturating_sub(1)
        };
        if reset {
            for (_, key) in cache.keys.drain() {
                self.rows.remove(&key);
            }
        }
        let path = path.map(std::borrow::Cow::into_owned);
        let matches = buffer.name.to_lowercase().contains(search)
            || path
                .as_ref()
                .is_some_and(|p| p.to_lowercase().contains(search));
        if matches {
            for entry in entries
                .iter()
                .skip(start)
                .filter(|e| e.summary != "Initial contents")
            {
                let cursor = TimelineCursor {
                    sequence: entry.global_seq,
                    document: id.clone(),
                    node: entry.id,
                };
                if let Some(old) = cache.keys.insert(entry.id, cursor.clone()) {
                    self.rows.remove(&old);
                }
                self.rows.insert(
                    cursor.clone(),
                    TimelineRow {
                        cursor,
                        timestamp: entry.edited_at,
                        name: buffer.name.clone(),
                        path: path.clone(),
                        summary: entry.summary.clone(),
                        kind: crate::app::services::session_store::timeline::TimelineKind::Edit(
                            entry.source,
                        ),
                    },
                );
            }
        }
        cache.baseline = baseline;
        cache.len = entries.len();
        cache.last = last;
        cache.name.clone_from(&buffer.name);
        cache.path = path;
        true
    }

    fn retire_absent(&mut self, loaded: &[String]) -> bool {
        // update_buffer has ensured that every loaded ID exists in documents.
        // Equal cardinality therefore means no document needs retiring.
        if loaded.len() == self.documents.len() {
            return false;
        }
        let mut changed = false;
        self.documents.retain(|id, cache| {
            if loaded.binary_search(id).is_ok() {
                return true;
            }
            for (_, key) in cache.keys.drain() {
                if let Some(row) = self.rows.remove(&key) {
                    self.retired.insert(key, row);
                }
            }
            changed = true;
            false
        });
        changed
    }
}
