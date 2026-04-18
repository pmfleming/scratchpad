use scratchpad::profile::{
    RECOMMENDED_LARGE_FILE_SPLIT_BYTES_PER_TILE, RECOMMENDED_LARGE_FILE_SPLIT_ITERATIONS,
    RECOMMENDED_LARGE_FILE_SPLIT_TILES, run_large_file_split_profile,
};
use std::hint::black_box;

fn main() {
    let total_actions = black_box(run_large_file_split_profile(
        RECOMMENDED_LARGE_FILE_SPLIT_TILES,
        RECOMMENDED_LARGE_FILE_SPLIT_BYTES_PER_TILE,
        RECOMMENDED_LARGE_FILE_SPLIT_ITERATIONS,
    ));
    println!(
        "large_file_split_profile tiles={} bytes_per_tile={} iterations={} total_actions={}",
        RECOMMENDED_LARGE_FILE_SPLIT_TILES,
        RECOMMENDED_LARGE_FILE_SPLIT_BYTES_PER_TILE,
        RECOMMENDED_LARGE_FILE_SPLIT_ITERATIONS,
        total_actions
    );
}
