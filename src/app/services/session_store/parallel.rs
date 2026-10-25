//! Small ordered, fallible scoped batches. Callers bound the number of jobs.
use std::{io, thread};

pub(super) fn worker_count(limit: usize) -> usize {
    thread::available_parallelism().map_or(1, |count| count.get().min(limit))
}

pub(super) fn ordered<T: Send, R: Send>(
    name: &str,
    jobs: impl IntoIterator<Item = T>,
    execute: impl Fn(T) -> io::Result<R> + Sync,
) -> io::Result<Vec<R>> {
    thread::scope(|scope| {
        let execute = &execute;
        // Collect spawn results without short-circuiting, then join every worker.
        // Neither an OS spawn failure nor a panic may silently drop recovered tabs.
        let handles = jobs
            .into_iter()
            .map(|job| {
                thread::Builder::new()
                    .name(name.into())
                    .spawn_scoped(scope, move || execute(job))
            })
            .collect::<Vec<_>>();
        let results = handles
            .into_iter()
            .map(|handle| {
                handle.and_then(|handle| {
                    handle.join().unwrap_or_else(|_| {
                        Err(io::Error::other(format!("{name} worker panicked")))
                    })
                })
            })
            .collect::<Vec<_>>();
        results.into_iter().collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ordered_batches_report_panics_without_losing_siblings() {
        let finished = std::sync::atomic::AtomicUsize::new(0);
        let result = ordered("test", 0..8, |index| {
            finished.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            assert_ne!(index, 3, "injected worker panic");
            Ok(index)
        });
        assert!(result.is_err());
        assert_eq!(finished.load(std::sync::atomic::Ordering::Relaxed), 8);
        assert_eq!(
            ordered("test", 0..8, Ok).unwrap(),
            (0..8).collect::<Vec<_>>()
        );
    }
}
