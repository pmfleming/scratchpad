use scratchpad::profile::{
    RECOMMENDED_SPLIT_STRESS_BYTES_PER_TILE, RECOMMENDED_SPLIT_STRESS_ITERATIONS,
    RECOMMENDED_SPLIT_STRESS_TILES, run_split_stress_profile,
};
use std::hint::black_box;

fn main() {
    let total_actions = black_box(run_split_stress_profile(
        RECOMMENDED_SPLIT_STRESS_TILES,
        RECOMMENDED_SPLIT_STRESS_BYTES_PER_TILE,
        RECOMMENDED_SPLIT_STRESS_ITERATIONS,
    ));
    println!(
        "split_stress_profile tiles={} bytes_per_tile={} iterations={} total_actions={}",
        RECOMMENDED_SPLIT_STRESS_TILES,
        RECOMMENDED_SPLIT_STRESS_BYTES_PER_TILE,
        RECOMMENDED_SPLIT_STRESS_ITERATIONS,
        total_actions
    );
}