use crate::app::{
    app_state::ScratchpadApp,
    services::session_store::timeline::{TimelineCursor, TimelineRow},
};
use std::collections::{BTreeMap, HashMap, HashSet};

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
                let id = buffer.temp_id.clone();
                loaded.push(id.clone());
                let entries = buffer.document().history_entries();
                let baseline = entries.first().map_or(0, |e| e.global_seq);
                let last = entries.last().map_or(0, |e| e.global_seq);
                let path = buffer
                    .path
                    .as_ref()
                    .map(|p| p.to_string_lossy().into_owned());
                let cache = self.documents.entry(id.clone()).or_default();
                if (cache.baseline, cache.len, cache.last) == (baseline, entries.len(), last)
                    && cache.name == buffer.name
                    && cache.path == path
                {
                    continue;
                }
                changed = true;
                let reset = cache.baseline != baseline
                    || entries.len() < cache.len
                    || cache.name != buffer.name
                    || cache.path != path;
                let start = if reset {
                    0
                } else {
                    cache.len.saturating_sub(1)
                };
                if reset {
                    for key in cache.keys.values() {
                        self.rows.remove(key);
                    }
                    cache.keys.clear();
                }
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
                                    kind: crate::app::services::session_store::timeline::TimelineKind::Edit(entry.source),
                            },
                        );
                    }
                }
                cache.baseline = baseline;
                cache.len = entries.len();
                cache.last = last;
                cache.name.clone_from(&buffer.name);
                cache.path = path;
            }
        }
        loaded.sort();
        loaded.dedup();
        let live: HashSet<_> = loaded.iter().cloned().collect();
        self.documents.retain(|id, cache| {
            if live.contains(id) {
                true
            } else {
                for key in cache.keys.values() {
                    self.rows.remove(key);
                }
                changed = true;
                false
            }
        });
        (loaded, changed)
    }
}
