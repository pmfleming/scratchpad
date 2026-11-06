//! Prepared resource fixtures keep setup and teardown outside operation counters.
use super::{PROFILE_QUERY, support};
use crate::ScratchpadApp;
use crate::app::app_state::SearchScope;
use crate::app::services::session_store::SessionStore;
use std::time::{Duration, Instant};

pub struct SearchResultFixture {
    // Drop the app/readers before removing the isolated store directory.
    app: ScratchpadApp,
    _root: tempfile::TempDir,
    expected_matches: usize,
    targets: usize,
    bytes_per_target: usize,
    actual_total_bytes: usize,
    completed: bool,
}

impl SearchResultFixture {
    #[must_use]
    pub fn new(targets: usize, bytes_per_target: usize) -> Self {
        assert!(targets > 0);
        let root = tempfile::tempdir().expect("search resource fixture root");
        let mut app =
            ScratchpadApp::with_session_store(SessionStore::new(root.path().join("session")));
        app.set_session_persist_on_drop(false);
        let expected_matches =
            support::install_search_all_tabs(&mut app, targets, bytes_per_target);
        let actual_total_bytes = app
            .tab_manager
            .tabs
            .as_slice()
            .iter()
            .flat_map(|tab| tab.buffers())
            .map(|buffer| buffer.document().piece_tree().len_bytes())
            .sum();
        app.open_search();
        app.set_search_scope(SearchScope::AllOpenTabs);
        assert!(expected_matches > 0);
        Self {
            app,
            _root: root,
            expected_matches,
            targets,
            bytes_per_target,
            actual_total_bytes,
            completed: false,
        }
    }

    /// Dispatch through the production app and retain all delivered results.
    /// A fixture is single-use so a cached completed query cannot become a fast trial.
    pub fn run(&mut self) -> serde_json::Value {
        assert!(!self.completed, "search resource fixture is single-use");
        let deadline = Instant::now() + Duration::from_secs(30);
        self.app.set_search_query(PROFILE_QUERY);
        loop {
            self.app.poll_search();
            let runtime = &self.app.state.search_state.runtime;
            if !runtime.searching && !runtime.dirty {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "search result resource fixture timed out"
            );
            std::thread::yield_now();
        }
        let matches = self.app.search_match_count();
        assert_eq!(
            matches, self.expected_matches,
            "all results must remain in the app"
        );
        self.completed = true;
        serde_json::json!({"fixture_revision":2,"target_count":self.targets,
            "bytes_per_target_requested":self.bytes_per_target,"actual_total_bytes":self.actual_total_bytes,
            "expected_matches":self.expected_matches,"result_matches":matches,"search_complete":true,
            "publication_endpoint":"completed_results_retained_in_app"})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn completed_search_retains_results_and_cannot_be_reused_as_a_cached_trial() {
        let mut fixture = SearchResultFixture::new(8, 4096);
        let sample = fixture.run();
        assert_eq!(sample["target_count"], 8);
        assert!(sample["result_matches"].as_u64().unwrap() > 8);
        assert_eq!(fixture.app.search_match_count(), fixture.expected_matches);
        assert!(!fixture.app.state.search_state.runtime.searching);
        assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| fixture.run())).is_err());
    }
}
