use super::{ByteSpan, PieceLineSample};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

type LineSampleCache = HashMap<ByteSpan, Arc<Vec<PieceLineSample>>>;

#[derive(Debug, Default)]
pub(super) struct PieceTreeRuntime {
    generation: u64,
    line_samples: OnceLock<Arc<Mutex<LineSampleCache>>>,
}

impl Clone for PieceTreeRuntime {
    fn clone(&self) -> Self {
        // Line samples are derived from this revision's storage. Sharing them
        // with a snapshot would let the snapshot repopulate offsets reused by
        // a later add-buffer compaction.
        Self {
            generation: self.generation,
            line_samples: OnceLock::new(),
        }
    }
}

impl PieceTreeRuntime {
    pub(super) fn generation(&self) -> u64 {
        self.generation
    }

    pub(super) fn advance_generation(&mut self) -> u64 {
        self.generation = self.generation.wrapping_add(1);
        self.generation
    }

    pub(super) fn clear_line_samples(&mut self) {
        if let Some(cache) = self.line_samples.get() {
            cache
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clear();
        }
    }

    pub(super) fn line_samples(
        &self,
        span: ByteSpan,
        build: impl FnOnce() -> Arc<Vec<PieceLineSample>>,
    ) -> Arc<Vec<PieceLineSample>> {
        let cache = self
            .line_samples
            .get_or_init(|| Arc::new(Mutex::new(HashMap::new())));
        let mut cache = cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        cache.entry(span).or_insert_with(build).clone()
    }
}
