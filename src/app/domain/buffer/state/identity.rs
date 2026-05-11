use super::BufferId;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static NEXT_BUFFER_ID: AtomicU64 = AtomicU64::new(1);
static NEXT_TEMP_BUFFER_ID: AtomicU64 = AtomicU64::new(1);

pub(super) fn next_buffer_id() -> BufferId {
    NEXT_BUFFER_ID.fetch_add(1, Ordering::Relaxed)
}

pub(super) fn register_existing_buffer_id(id: BufferId) {
    let next_id = id.saturating_add(1);
    let mut current = NEXT_BUFFER_ID.load(Ordering::Relaxed);

    while current < next_id {
        match NEXT_BUFFER_ID.compare_exchange(
            current,
            next_id,
            Ordering::Relaxed,
            Ordering::Relaxed,
        ) {
            Ok(_) => break,
            Err(observed) => current = observed,
        }
    }
}

pub(super) fn next_temp_id() -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let sequence = NEXT_TEMP_BUFFER_ID.fetch_add(1, Ordering::Relaxed);
    format!("buffer-{timestamp}-{sequence}")
}
