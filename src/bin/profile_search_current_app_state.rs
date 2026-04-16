use scratchpad::profile::{
    RECOMMENDED_SEARCH_CURRENT_BYTES_PER_FILE, RECOMMENDED_SEARCH_CURRENT_FILES,
    RECOMMENDED_SEARCH_CURRENT_ITERATIONS, run_search_current_app_state_profile,
};
use std::hint::black_box;

fn main() {
    let total_matches = black_box(run_search_current_app_state_profile(
        RECOMMENDED_SEARCH_CURRENT_FILES,
        RECOMMENDED_SEARCH_CURRENT_BYTES_PER_FILE,
        RECOMMENDED_SEARCH_CURRENT_ITERATIONS,
    ));
    println!(
        "search_current_app_state_profile files={} bytes_per_file={} iterations={} total_matches={}",
        RECOMMENDED_SEARCH_CURRENT_FILES,
        RECOMMENDED_SEARCH_CURRENT_BYTES_PER_FILE,
        RECOMMENDED_SEARCH_CURRENT_ITERATIONS,
        total_matches
    );
}
