use scratchpad::profile::{
    RECOMMENDED_DOCUMENT_SNAPSHOT_BYTES, RECOMMENDED_DOCUMENT_SNAPSHOT_ITERATIONS,
    run_document_snapshot_profile,
};
use std::hint::black_box;

fn main() {
    let total = black_box(run_document_snapshot_profile(
        RECOMMENDED_DOCUMENT_SNAPSHOT_BYTES,
        RECOMMENDED_DOCUMENT_SNAPSHOT_ITERATIONS,
    ));
    println!(
        "document_snapshot_profile bytes={} iterations={} total={}",
        RECOMMENDED_DOCUMENT_SNAPSHOT_BYTES, RECOMMENDED_DOCUMENT_SNAPSHOT_ITERATIONS, total
    );
}
