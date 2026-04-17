use scratchpad::profile::{
    RECOMMENDED_TAB_TILE_BYTES, RECOMMENDED_TAB_TILE_COUNT, RECOMMENDED_TAB_TILE_ITERATIONS,
    run_tab_tile_layout_profile,
};
use std::hint::black_box;

fn main() {
    let total_actions = black_box(run_tab_tile_layout_profile(
        RECOMMENDED_TAB_TILE_COUNT,
        RECOMMENDED_TAB_TILE_BYTES,
        RECOMMENDED_TAB_TILE_ITERATIONS,
    ));
    println!(
        "tab_tile_layout_profile tiles={} bytes_per_tile={} iterations={} total_actions={}",
        RECOMMENDED_TAB_TILE_COUNT,
        RECOMMENDED_TAB_TILE_BYTES,
        RECOMMENDED_TAB_TILE_ITERATIONS,
        total_actions
    );
}
