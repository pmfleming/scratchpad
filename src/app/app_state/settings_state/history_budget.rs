use super::ScratchpadApp;
use crate::app::domain::BufferId;

impl ScratchpadApp {
    pub(crate) fn apply_history_budget_to_open_buffers(&mut self) {
        let budget = self.app_settings.history_budget;
        for tab in self.tabs_mut() {
            for buffer in tab.buffers_mut() {
                buffer.document_mut().set_history_budget(budget);
            }
        }
        self.enforce_aggregate_text_history_budget();
    }

    pub(crate) fn enforce_aggregate_text_history_budget(&mut self) {
        let aggregate_budget = self.app_settings.history_budget.aggregate_byte_budget;
        while self.aggregate_text_history_usage() > aggregate_budget {
            let Some((tab_index, buffer_id)) = self.oldest_history_buffer() else {
                break;
            };
            if !self.drop_oldest_history_entry(tab_index, buffer_id) {
                break;
            }
        }
    }

    fn aggregate_text_history_usage(&self) -> u64 {
        self.tabs()
            .iter()
            .flat_map(|tab| tab.buffers())
            .map(|buffer| buffer.document().history_byte_usage() as u64)
            .sum()
    }

    fn oldest_history_buffer(&self) -> Option<(usize, BufferId)> {
        self.tabs()
            .iter()
            .enumerate()
            .flat_map(|(tab_index, tab)| {
                tab.buffers().filter_map(move |buffer| {
                    buffer
                        .document()
                        .oldest_history_global_seq()
                        .map(|global_seq| (global_seq, tab_index, buffer.id))
                })
            })
            .min_by_key(|(global_seq, _, buffer_id)| (*global_seq, *buffer_id))
            .map(|(_, tab_index, buffer_id)| (tab_index, buffer_id))
    }

    fn drop_oldest_history_entry(&mut self, tab_index: usize, buffer_id: BufferId) -> bool {
        let Some(buffer) = self
            .tabs_mut()
            .get_mut(tab_index)
            .and_then(|tab| tab.buffer_by_id_mut(buffer_id))
        else {
            return false;
        };
        let Some(removed) = buffer.document_mut().drop_oldest_history_entry() else {
            return false;
        };
        crate::app::capacity_metrics::record_history_eviction_aggregate(removed.byte_cost());
        true
    }
}
