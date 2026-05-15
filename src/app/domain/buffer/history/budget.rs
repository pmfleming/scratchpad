const MIB: u64 = 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TextHistoryBudget {
    pub per_file_entry_limit: usize,
    pub per_file_byte_budget: u64,
    pub aggregate_byte_budget: u64,
    pub persisted_payload_budget: u64,
    pub derived_from_memory: bool,
}

impl Default for TextHistoryBudget {
    fn default() -> Self {
        Self::derive_from_available_memory()
    }
}

impl TextHistoryBudget {
    #[must_use]
    pub fn derive_from_available_memory() -> Self {
        let available = available_memory_bytes().unwrap_or(2 * 1024 * MIB);
        let aggregate = clamp_u64(available / 50, 16 * MIB, 512 * MIB);
        let per_file = clamp_u64(aggregate / 8, 4 * MIB, 64 * MIB);
        let persisted = clamp_u64(aggregate / 16, MIB, 16 * MIB);
        let entries = clamp_u64(per_file / (8 * 1024), 500, 10_000) as usize;
        Self {
            per_file_entry_limit: entries,
            per_file_byte_budget: per_file,
            aggregate_byte_budget: aggregate,
            persisted_payload_budget: persisted,
            derived_from_memory: true,
        }
    }

    #[must_use]
    pub fn sanitized(mut self) -> Self {
        self.per_file_entry_limit = self.per_file_entry_limit.clamp(100, 100_000);
        self.per_file_byte_budget = self.per_file_byte_budget.clamp(MIB, 1024 * MIB);
        self.aggregate_byte_budget = self.aggregate_byte_budget.clamp(4 * MIB, 4096 * MIB);
        self.persisted_payload_budget = self.persisted_payload_budget.clamp(0, 1024 * MIB);
        self
    }
}

fn clamp_u64(value: u64, min: u64, max: u64) -> u64 {
    value.clamp(min, max)
}

fn available_memory_bytes() -> Option<u64> {
    let mut system = sysinfo::System::new();
    system.refresh_memory();
    let available = system.available_memory();
    (available > 0).then_some(available)
}
