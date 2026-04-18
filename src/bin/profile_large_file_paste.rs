use scratchpad::profile::{
    RECOMMENDED_LARGE_FILE_PASTE_BASE_BYTES, RECOMMENDED_LARGE_FILE_PASTE_INSERT_BYTES,
    RECOMMENDED_LARGE_FILE_PASTE_ITERATIONS, run_large_file_paste_profile,
};
use std::hint::black_box;

fn main() {
    let total_bytes = black_box(run_large_file_paste_profile(
        RECOMMENDED_LARGE_FILE_PASTE_BASE_BYTES,
        RECOMMENDED_LARGE_FILE_PASTE_INSERT_BYTES,
        RECOMMENDED_LARGE_FILE_PASTE_ITERATIONS,
    ));
    println!(
        "large_file_paste_profile base_bytes={} insert_bytes={} iterations={} total_work={}",
        RECOMMENDED_LARGE_FILE_PASTE_BASE_BYTES,
        RECOMMENDED_LARGE_FILE_PASTE_INSERT_BYTES,
        RECOMMENDED_LARGE_FILE_PASTE_ITERATIONS,
        total_bytes
    );
}
