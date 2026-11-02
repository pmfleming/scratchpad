//! State positions shared by the rail and scrubber. Position k is *after* k edits.
use super::model::TextHistoryAction;
use crate::app::text_history::TextHistoryEntryView;

pub(super) struct States<'a> {
    pub entries: Vec<&'a TextHistoryEntryView>,
    pub position: usize,
    pub first: usize,
    pub last: usize,
}

impl<'a> States<'a> {
    pub fn new(entries: &'a [TextHistoryEntryView], buffer_id: u64) -> Self {
        let mut entries: Vec<_> = entries
            .iter()
            .filter(|e| e.buffer_id == buffer_id)
            .collect();
        entries.sort_by_key(|e| e.global_seq);
        let position = entries.iter().take_while(|e| !e.undone).count();
        let first = entries[..position]
            .iter()
            .rposition(|e| !e.replayable)
            .map_or(0, |i| i + 1);
        let last = entries[position..]
            .iter()
            .position(|e| !e.replayable)
            .map_or(entries.len(), |i| position + i);
        Self {
            entries,
            position,
            first,
            last,
        }
    }

    pub fn reachable(&self, position: usize) -> bool {
        (self.first..=self.last).contains(&position)
    }

    pub fn after(&self, position: usize) -> Option<u64> {
        position
            .checked_sub(1)
            .and_then(|i| self.entries.get(i))
            .map(|e| e.id)
    }

    pub fn index(&self, after: Option<u64>) -> Option<usize> {
        match after {
            None => Some(0),
            Some(id) => self.entries.iter().position(|e| e.id == id).map(|i| i + 1),
        }
    }

    /// Translate a destination state into the existing inclusive replay API.
    pub fn target(&self, position: usize) -> Option<TextHistoryAction> {
        if !self.reachable(position) || position == self.position {
            return None;
        }
        let index = if position < self.position {
            position
        } else {
            position - 1
        };
        let entry = self.entries[index];
        Some(TextHistoryAction {
            buffer_id: entry.buffer_id,
            entry_id: entry.id,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{States, TextHistoryEntryView};
    use crate::app::domain::PieceSource;

    fn entries() -> Vec<TextHistoryEntryView> {
        (0..5)
            .map(|i| TextHistoryEntryView {
                id: i + 1,
                global_seq: i + 1,
                buffer_id: 7,
                label: String::new(),
                source: PieceSource::Edit,
                summary: String::new(),
                undone: i >= 3,
                replayable: true,
                edit_count: 1,
                deleted_preview: String::new(),
                inserted_preview: String::new(),
            })
            .collect()
    }

    #[test]
    fn destinations_are_after_edits_and_current_is_a_noop() {
        let entries = entries();
        let states = States::new(&entries, 7);
        assert_eq!(states.position, 3);
        assert_eq!(states.target(0).unwrap().entry_id, 1);
        assert_eq!(states.target(1).unwrap().entry_id, 2);
        assert!(states.target(3).is_none());
        assert_eq!(states.target(4).unwrap().entry_id, 4);
        assert_eq!(states.target(5).unwrap().entry_id, 5);
        assert_eq!(states.index(None), Some(0));
        assert_eq!(states.index(Some(999)), None);
        for k in 0..=5 {
            assert_eq!(states.index(states.after(k)), Some(k));
        }
    }

    #[test]
    fn barriers_block_the_entire_path_in_both_directions() {
        let mut entries = entries();
        entries[1].replayable = false;
        entries[4].replayable = false;
        let states = States::new(&entries, 7);
        assert_eq!((states.first, states.last), (2, 4));
        assert!(states.target(0).is_none());
        assert!(states.target(1).is_none());
        assert!(states.target(2).is_some());
        assert!(states.target(4).is_some());
        assert!(states.target(5).is_none());
        assert!(States::new(&entries, 99).target(0).is_none());
    }
}
