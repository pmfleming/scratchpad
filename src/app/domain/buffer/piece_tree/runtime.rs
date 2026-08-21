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
        let line_samples = OnceLock::new();
        if let Some(cache) = self.line_samples.get() {
            let _ = line_samples.set(cache.clone());
        }
        Self {
            generation: self.generation,
            line_samples,
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
