use scratchpad::profile::{
    RECOMMENDED_PASTE_STRESS_BASE_BYTES, RECOMMENDED_PASTE_STRESS_INSERT_BYTES,
    RECOMMENDED_PASTE_STRESS_ITERATIONS, run_paste_stress_profile,
};
use std::hint::black_box;

fn main() {
    let total_bytes = black_box(run_paste_stress_profile(
        RECOMMENDED_PASTE_STRESS_BASE_BYTES,
        RECOMMENDED_PASTE_STRESS_INSERT_BYTES,
        RECOMMENDED_PASTE_STRESS_ITERATIONS,
    ));
    println!(
        "paste_stress_profile base_bytes={} insert_bytes={} iterations={} total_work={}",
        RECOMMENDED_PASTE_STRESS_BASE_BYTES,
        RECOMMENDED_PASTE_STRESS_INSERT_BYTES,
        RECOMMENDED_PASTE_STRESS_ITERATIONS,
        total_bytes
    );
}