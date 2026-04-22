use scratchpad::profile::{
    RECOMMENDED_SEARCH_DISPATCH_ALL_TABS, RECOMMENDED_SEARCH_DISPATCH_BYTES_PER_ITEM,
    RECOMMENDED_SEARCH_DISPATCH_CURRENT_FILES, RECOMMENDED_SEARCH_DISPATCH_ITERATIONS,
    run_search_dispatch_all_tabs_profile, run_search_dispatch_current_profile,
};
use std::hint::black_box;

fn main() {
    let current_total = black_box(run_search_dispatch_current_profile(
        RECOMMENDED_SEARCH_DISPATCH_CURRENT_FILES,
        RECOMMENDED_SEARCH_DISPATCH_BYTES_PER_ITEM,
        RECOMMENDED_SEARCH_DISPATCH_ITERATIONS,
    ));
    let all_total = black_box(run_search_dispatch_all_tabs_profile(
        RECOMMENDED_SEARCH_DISPATCH_ALL_TABS,
        RECOMMENDED_SEARCH_DISPATCH_BYTES_PER_ITEM,
        RECOMMENDED_SEARCH_DISPATCH_ITERATIONS,
    ));
    println!(
        "search_dispatch_profile current_files={} all_tabs={} bytes_per_item={} iterations={} current_total={} all_total={}",
        RECOMMENDED_SEARCH_DISPATCH_CURRENT_FILES,
        RECOMMENDED_SEARCH_DISPATCH_ALL_TABS,
        RECOMMENDED_SEARCH_DISPATCH_BYTES_PER_ITEM,
        RECOMMENDED_SEARCH_DISPATCH_ITERATIONS,
        current_total,
        all_total
    );
}
