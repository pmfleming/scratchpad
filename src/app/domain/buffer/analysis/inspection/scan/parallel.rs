use super::TextScanSummary;
use std::ops::Range;
use std::thread;

const PARALLEL_TEXT_INSPECTION_MIN_BYTES: usize = 4 * 1024 * 1024;
const PARALLEL_TEXT_INSPECTION_MAX_WORKERS: usize = 8;

pub(super) fn scan_text(text: &str) -> Option<TextScanSummary> {
    if text.len() < PARALLEL_TEXT_INSPECTION_MIN_BYTES {
        return None;
    }
    scan_text_with_workers(text, inspection_worker_count(text.len()))
}

pub(super) fn scan_text_with_workers(text: &str, workers: usize) -> Option<TextScanSummary> {
    if workers <= 1 {
        return None;
    }

    let ranges = chunk_ranges_for_text(text, workers);
    if ranges.len() <= 1 {
        return None;
    }

    let mut summaries = Vec::with_capacity(ranges.len());
    thread::scope(|scope| {
        let mut handles = Vec::with_capacity(ranges.len());
        for range in ranges {
            handles.push(scope.spawn(move || TextScanSummary::scan_text_serial(&text[range])));
        }
        for handle in handles {
            if let Ok(summary) = handle.join() {
                summaries.push(summary);
            }
        }
    });

    Some(TextScanSummary::combine(summaries))
}

pub(super) fn scan_span_slice(spans: &[&str]) -> Option<TextScanSummary> {
    let total_bytes = spans.iter().map(|span| span.len()).sum::<usize>();
    if total_bytes < PARALLEL_TEXT_INSPECTION_MIN_BYTES {
        return None;
    }
    let workers = inspection_worker_count(total_bytes).min(spans.len());
    scan_span_slice_with_workers(spans, workers)
}

pub(super) fn scan_span_slice_with_workers(
    spans: &[&str],
    workers: usize,
) -> Option<TextScanSummary> {
    if workers <= 1 {
        return None;
    }

    let chunk_size = spans.len().div_ceil(workers);
    let mut summaries = Vec::with_capacity(workers);
    thread::scope(|scope| {
        let mut handles = Vec::with_capacity(workers);
        for chunk in spans.chunks(chunk_size) {
            handles.push(scope.spawn(move || TextScanSummary::scan_spans(chunk.iter().copied())));
        }
        for handle in handles {
            if let Ok(summary) = handle.join() {
                summaries.push(summary);
            }
        }
    });

    Some(TextScanSummary::combine(summaries))
}

fn inspection_worker_count(total_bytes: usize) -> usize {
    let by_size = (total_bytes / PARALLEL_TEXT_INSPECTION_MIN_BYTES).max(1);
    thread::available_parallelism()
        .map(|parallelism| {
            parallelism
                .get()
                .min(PARALLEL_TEXT_INSPECTION_MAX_WORKERS)
                .min(by_size)
        })
        .unwrap_or(1)
        .max(1)
}

fn chunk_ranges_for_text(text: &str, workers: usize) -> Vec<Range<usize>> {
    let mut ranges = Vec::with_capacity(workers);
    let target = text.len().div_ceil(workers);
    let mut start = 0usize;

    while start < text.len() {
        let mut end = (start + target).min(text.len());
        while end < text.len() && !text.is_char_boundary(end) {
            end += 1;
        }
        if end <= start {
            end = text[start..]
                .char_indices()
                .nth(1)
                .map(|(offset, _)| start + offset)
                .unwrap_or(text.len());
        }
        ranges.push(start..end);
        start = end;
    }

    ranges
}
