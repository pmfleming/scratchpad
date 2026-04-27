use scratchpad::profile::{
    RECOMMENDED_SCROLL_STRESS_BYTES, RECOMMENDED_SCROLL_STRESS_ITERATIONS,
    run_scroll_stress_profile,
};
use std::hint::black_box;

fn main() {
    let total_rows = black_box(run_scroll_stress_profile(
        RECOMMENDED_SCROLL_STRESS_BYTES,
        RECOMMENDED_SCROLL_STRESS_ITERATIONS,
    ));
    println!(
        "scroll_stress_profile bytes={} iterations={} total_rows={}",
        RECOMMENDED_SCROLL_STRESS_BYTES, RECOMMENDED_SCROLL_STRESS_ITERATIONS, total_rows
    );
}