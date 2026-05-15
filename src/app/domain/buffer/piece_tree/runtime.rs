#[derive(Clone, Debug, Default)]
pub(super) struct PieceTreeRuntime {
    generation: u64,
}

impl PieceTreeRuntime {
    pub(super) fn generation(&self) -> u64 {
        self.generation
    }

    pub(super) fn advance_generation(&mut self) -> u64 {
        self.generation = self.generation.wrapping_add(1);
        self.generation
    }
}
