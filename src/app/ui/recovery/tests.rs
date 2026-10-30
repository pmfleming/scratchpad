use super::{query_with_interval, timestamp};
use eframe::egui;
use std::{
    io,
    sync::{Arc, Mutex, mpsc},
    time::{Duration, Instant},
};

fn frame<T: Send + Sync + 'static>(
    ctx: &egui::Context,
    interval: Duration,
    load: impl Fn() -> io::Result<T> + Send + Sync + 'static,
) -> Option<Result<Arc<T>, String>> {
    let load = Arc::new(load);
    let mut result = None;
    let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
        let load = load.clone();
        result = query_with_interval(ui, "test", interval, move || load());
    });
    output.textures_delta.clear();
    result
}

#[test]
fn retry_expires_the_cache_without_discarding_an_inflight_worker() {
    let ctx = egui::Context::default();
    let key = egui::Id::unique(("recovery.query", "retry"));
    let cache = Arc::new(Mutex::new(super::QueryCache::<u64> {
        result: Some(Err("offline".into())),
        checked: Some(Instant::now()),
        pending: true,
    }));
    ctx.data_mut(|data| data.insert_temp(key, cache.clone()));
    let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
        super::refresh_query::<u64>(ui, "retry");
    });
    output.textures_delta.clear();
    let state = cache.lock().unwrap();
    assert!(state.pending);
    assert!(state.checked.is_none());
    assert_eq!(
        state.result.as_ref().unwrap().as_ref().unwrap_err(),
        "offline"
    );
    let stored = ctx
        .data_mut(|data| data.get_temp::<Arc<Mutex<super::QueryCache<u64>>>>(key))
        .unwrap();
    assert!(Arc::ptr_eq(&cache, &stored));
}

#[test]
fn query_shares_nonclone_results_suppresses_duplicate_loads_and_refreshes_errors() {
    struct Payload(Vec<u8>); // Intentionally not Clone: UI frames must not copy payloads.
    let ctx = egui::Context::default();
    let ttl = Duration::from_mins(1);
    let (release, wait) = mpsc::channel();
    let wait = Mutex::new(wait);
    assert!(
        frame(&ctx, ttl, move || {
            wait.lock()
                .unwrap()
                .recv_timeout(Duration::from_secs(5))
                .unwrap();
            Ok(Payload(vec![42; 4096]))
        })
        .is_none()
    );
    let unexpected = || -> io::Result<Payload> { panic!("duplicate/cached load") };
    assert!(frame(&ctx, ttl, unexpected).is_none());
    release.send(()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    let first = loop {
        if let Some(result) = frame(&ctx, ttl, unexpected) {
            break result.unwrap();
        }
        assert!(Instant::now() < deadline, "query did not complete");
        std::thread::sleep(Duration::from_millis(1));
    };
    let second = frame(&ctx, ttl, unexpected).unwrap().unwrap();
    assert!(Arc::ptr_eq(&first, &second));
    assert_eq!(second.0.len(), 4096);
    let refreshing = frame(&ctx, Duration::ZERO, || {
        Err::<Payload, _>(io::Error::other("offline"))
    })
    .unwrap()
    .unwrap();
    assert!(
        Arc::ptr_eq(&first, &refreshing),
        "refresh keeps last successful result visible"
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(Err(error)) = frame(&ctx, ttl, unexpected) {
            assert_eq!(error, "offline");
            break;
        }
        assert!(Instant::now() < deadline, "error did not arrive");
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(
        frame(&ctx, ttl, unexpected).unwrap().err().unwrap(),
        "offline"
    );
    assert_eq!(timestamp(i64::MAX), "Unknown date");
    assert_eq!(timestamp(0), "1970-01-01 00:00 UTC");
}
