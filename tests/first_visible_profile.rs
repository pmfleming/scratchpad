//! Integration tests intentionally use the production staged-open thresholds.
//! Library unit tests lower those thresholds and cannot reproduce the 1 MiB
//! asynchronous-open race found by the performance lens.
use scratchpad::profile::{run_file_first_render_preparation, run_many_file_first_visible_profile};

#[test]
fn first_visible_waits_for_async_file_instead_of_rendering_untitled_frame() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("async.txt");
    std::fs::write(&path, "content\n".repeat(128 * 1024)).unwrap();
    let (elapsed_ns, primitives) = run_file_first_render_preparation(&path);
    assert!(elapsed_ns > 0 && primitives > 0);

    let profile = run_many_file_first_visible_profile(vec![path]);
    assert_eq!(profile.active_buffer_bytes, 1024 * 1024);
    assert!(profile.first_visible_ns <= profile.background_completion_ns);
}

#[test]
fn an_opened_empty_file_is_valid_first_visible_content() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("empty.txt");
    std::fs::write(&path, "").unwrap();
    let (elapsed_ns, primitives) = run_file_first_render_preparation(&path);
    assert!(elapsed_ns > 0 && primitives > 0);
}
