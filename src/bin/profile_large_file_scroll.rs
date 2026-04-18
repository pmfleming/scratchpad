use scratchpad::profile::{
    RECOMMENDED_LARGE_FILE_SCROLL_BYTES, RECOMMENDED_LARGE_FILE_SCROLL_ITERATIONS,
    run_large_file_scroll_profile,
};
use std::hint::black_box;

fn main() {
    let total_rows = black_box(run_large_file_scroll_profile(
        RECOMMENDED_LARGE_FILE_SCROLL_BYTES,
        RECOMMENDED_LARGE_FILE_SCROLL_ITERATIONS,
    ));
    println!(
        "large_file_scroll_profile bytes={} iterations={} total_rows={}",
        RECOMMENDED_LARGE_FILE_SCROLL_BYTES, RECOMMENDED_LARGE_FILE_SCROLL_ITERATIONS, total_rows
    );
}
