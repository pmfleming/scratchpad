use scratchpad::profile::{
    RECOMMENDED_VIEWPORT_EXTRACTION_BYTES, RECOMMENDED_VIEWPORT_EXTRACTION_ITERATIONS,
    run_viewport_extraction_profile,
};
use std::hint::black_box;

fn main() {
    let total = black_box(run_viewport_extraction_profile(
        RECOMMENDED_VIEWPORT_EXTRACTION_BYTES,
        RECOMMENDED_VIEWPORT_EXTRACTION_ITERATIONS,
    ));
    println!(
        "viewport_extraction_profile bytes={} iterations={} total={}",
        RECOMMENDED_VIEWPORT_EXTRACTION_BYTES, RECOMMENDED_VIEWPORT_EXTRACTION_ITERATIONS, total
    );
}
