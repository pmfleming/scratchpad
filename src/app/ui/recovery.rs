//! Small asynchronous caches for recovery menus. Database work never runs each
//! paint frame; failures are displayed, not converted into empty history lists.
use eframe::egui;
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

struct QueryCache<T> {
    result: Option<Result<T, String>>,
    checked: Option<Instant>,
    pending: bool,
}
impl<T> Default for QueryCache<T> {
    fn default() -> Self {
        Self {
            result: None,
            checked: None,
            pending: false,
        }
    }
}

pub(crate) fn query<T: Clone + Send + Sync + 'static>(
    ui: &egui::Ui,
    key: impl std::hash::Hash + std::fmt::Debug,
    load: impl FnOnce() -> std::io::Result<T> + Send + 'static,
) -> Option<Result<T, String>> {
    query_with_interval(ui, key, Duration::from_secs(2), load)
}

pub(crate) fn query_with_interval<T: Clone + Send + Sync + 'static>(
    ui: &egui::Ui,
    key: impl std::hash::Hash + std::fmt::Debug,
    interval: Duration,
    load: impl FnOnce() -> std::io::Result<T> + Send + 'static,
) -> Option<Result<T, String>> {
    let key = egui::Id::unique(("recovery.query", key));
    let cache = ui.ctx().data_mut(|data| {
        data.get_temp_mut_or_default::<Arc<Mutex<QueryCache<T>>>>(key)
            .clone()
    });
    let mut state = cache.lock().unwrap_or_else(|e| e.into_inner());
    if !state.pending && state.checked.is_none_or(|at| at.elapsed() >= interval) {
        state.pending = true;
        let cache = cache.clone();
        let ctx = ui.ctx().clone();
        if let Err(error) = std::thread::Builder::new()
            .name("recovery-query".into())
            .spawn(move || {
                let result = load().map_err(|error| error.to_string());
                let mut state = cache.lock().unwrap_or_else(|e| e.into_inner());
                state.result = Some(result);
                state.checked = Some(Instant::now());
                state.pending = false;
                ctx.request_repaint();
            })
        {
            state.pending = false;
            state.checked = Some(Instant::now());
            state.result = Some(Err(error.to_string()));
        }
    }
    ui.ctx().request_repaint_after(interval);
    state.result.clone()
}

pub(crate) fn timestamp(seconds: i64) -> String {
    chrono::DateTime::from_timestamp(seconds, 0).map_or_else(
        || "Unknown date".into(),
        |date| date.format("%Y-%m-%d %H:%M UTC").to_string(),
    )
}
