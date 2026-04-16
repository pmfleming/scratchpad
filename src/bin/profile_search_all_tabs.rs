use scratchpad::profile::{
    RECOMMENDED_SEARCH_ALL_BYTES_PER_TAB, RECOMMENDED_SEARCH_ALL_ITERATIONS,
    RECOMMENDED_SEARCH_ALL_TABS, run_search_all_tabs_profile,
};
use std::hint::black_box;

fn main() {
    let total_matches = black_box(run_search_all_tabs_profile(
        RECOMMENDED_SEARCH_ALL_TABS,
        RECOMMENDED_SEARCH_ALL_BYTES_PER_TAB,
        RECOMMENDED_SEARCH_ALL_ITERATIONS,
    ));
    println!(
        "search_all_tabs_profile tabs={} bytes_per_tab={} iterations={} total_matches={}",
        RECOMMENDED_SEARCH_ALL_TABS,
        RECOMMENDED_SEARCH_ALL_BYTES_PER_TAB,
        RECOMMENDED_SEARCH_ALL_ITERATIONS,
        total_matches
    );
}
