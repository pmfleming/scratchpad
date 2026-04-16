use scratchpad::profile::{
    RECOMMENDED_TAB_OPERATION_ITERATIONS, RECOMMENDED_TAB_OPERATION_TABS,
    run_tab_operations_profile,
};
use std::hint::black_box;

fn main() {
    let total_views = black_box(run_tab_operations_profile(
        RECOMMENDED_TAB_OPERATION_TABS,
        RECOMMENDED_TAB_OPERATION_ITERATIONS,
    ));
    println!(
        "tab_operations_profile tabs={} iterations={} total_views={}",
        RECOMMENDED_TAB_OPERATION_TABS, RECOMMENDED_TAB_OPERATION_ITERATIONS, total_views
    );
}
