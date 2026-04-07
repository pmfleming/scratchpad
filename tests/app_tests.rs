use rand::RngExt;
use rand::seq::SliceRandom;
use scratchpad::ScratchpadApp;
use scratchpad::app::{paths_match, services::session_store::SessionStore};
use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn path_match_is_case_insensitive_on_windows_paths() {
    assert!(paths_match(
        Path::new(r"C:\Temp\notes.txt"),
        Path::new(r"c:\temp\NOTES.txt")
    ));
}

#[test]
fn path_match_rejects_different_files() {
    assert!(!paths_match(
        Path::new(r"C:\Temp\notes.txt"),
        Path::new(r"C:\Temp\other.txt")
    ));
}

#[test]
fn opens_configurable_number_of_tabs_defaulting_to_1000() {
    let tab_count = std::env::var("SCRATCHPAD_TAB_STRESS_COUNT")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|count| *count > 0)
        .unwrap_or(1000);
    let session_root = std::env::temp_dir().join(format!(
        "scratchpad-tab-stress-test-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let session_store = SessionStore::new(session_root.clone());
    let mut app = ScratchpadApp::with_session_store(session_store);

    for _ in 1..tab_count {
        app.create_untitled_tab();
    }

    assert_eq!(app.tabs().len(), tab_count);

    let mut rng = rand::rng();
    let tabs_to_populate = (tab_count / 10).max(1);
    let mut indices: Vec<usize> = (0..tab_count).collect();
    indices.shuffle(&mut rng);

    for &index in &indices[..tabs_to_populate] {
        let line_count = rng.random_range(1..=1000);
        let mut content = String::new();
        for i in 0..line_count {
            content.push_str(&format!("This is random line {} in tab {}\n", i, index));
        }
        app.tabs_mut()[index].buffer.content = content;
        app.tabs_mut()[index].buffer.is_dirty = true;
    }

    app.session_store()
        .persist(
            app.tabs(),
            app.active_tab_index(),
            app.font_size(),
            app.word_wrap(),
        )
        .unwrap();

    assert_eq!(app.active_tab_index(), tab_count - 1);

    let restored = app.session_store().load().unwrap().unwrap();
    assert_eq!(restored.tabs.len(), tab_count);
    assert_eq!(restored.active_tab_index, tab_count - 1);

    for &index in &indices[..tabs_to_populate] {
        assert!(!restored.tabs[index].buffer.content.is_empty());
    }

    let mut close_indices: Vec<usize> = (0..tab_count).collect();
    close_indices.shuffle(&mut rng);

    for _ in close_indices {
        let current_count = app.tabs().len();
        let random_idx = rng.random_range(0..current_count);
        app.perform_close_tab_no_persist(random_idx);
    }

    assert_eq!(app.tabs().len(), 1);

    drop(app);
    fs::remove_dir_all(session_root).unwrap();
}
