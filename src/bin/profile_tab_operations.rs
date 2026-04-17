use scratchpad::profile::{
    RECOMMENDED_TAB_OPERATION_BYTES_PER_BUFFER, RECOMMENDED_TAB_OPERATION_ITERATIONS,
    RECOMMENDED_TAB_OPERATION_TABS, RECOMMENDED_TAB_OPERATION_VIEWS_PER_TAB,
    run_tab_operations_profile,
};
use std::hint::black_box;

fn main() {
    let total_views = black_box(run_tab_operations_profile(
        RECOMMENDED_TAB_OPERATION_TABS,
        RECOMMENDED_TAB_OPERATION_ITERATIONS,
    ));
    println!(
        "tab_operations_profile tabs={} views_per_tab={} bytes_per_buffer={} iterations={} total_actions={}",
        RECOMMENDED_TAB_OPERATION_TABS,
        RECOMMENDED_TAB_OPERATION_VIEWS_PER_TAB,
        RECOMMENDED_TAB_OPERATION_BYTES_PER_BUFFER,
        RECOMMENDED_TAB_OPERATION_ITERATIONS,
        total_views
    );
}
