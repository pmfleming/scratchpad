use scratchpad::profile::{
    RECOMMENDED_VIEW_NAVIGATION_BYTES_PER_BUFFER, RECOMMENDED_VIEW_NAVIGATION_ITERATIONS,
    RECOMMENDED_VIEW_NAVIGATION_VIEWS, run_view_navigation_profile,
};
use std::hint::black_box;

fn main() {
    let total_activations = black_box(run_view_navigation_profile(
        RECOMMENDED_VIEW_NAVIGATION_VIEWS,
        RECOMMENDED_VIEW_NAVIGATION_BYTES_PER_BUFFER,
        RECOMMENDED_VIEW_NAVIGATION_ITERATIONS,
    ));
    println!(
        "view_navigation_profile views={} bytes_per_buffer={} iterations={} total_activations={}",
        RECOMMENDED_VIEW_NAVIGATION_VIEWS,
        RECOMMENDED_VIEW_NAVIGATION_BYTES_PER_BUFFER,
        RECOMMENDED_VIEW_NAVIGATION_ITERATIONS,
        total_activations
    );
}