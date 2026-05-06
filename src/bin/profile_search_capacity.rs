use scratchpad::app::services::search::{SearchMode, SearchOptions, SearchProgram, search_program};
use std::hint::black_box;
use std::time::Instant;

const MB: usize = 1024 * 1024;
const CAPACITY_SIZES: [usize; 3] = [MB, 50 * MB, 250 * MB];

fn main() {
    let options = SearchOptions {
        mode: SearchMode::PlainText,
        match_case: true,
        whole_word: false,
    };
    let program = SearchProgram::compile("needle", options).expect("literal program compiles");
    for bytes in CAPACITY_SIZES {
        let text = capacity_text(bytes);
        let started = Instant::now();
        let matches = black_box(search_program(black_box(&text), &program).matches.len());
        println!(
            "search_capacity bytes={} matches={} elapsed_ms={}",
            bytes,
            matches,
            started.elapsed().as_millis()
        );
    }
}

fn capacity_text(bytes: usize) -> String {
    let unit = "hay hay hay needle\n";
    let mut text = String::with_capacity(bytes + unit.len());
    while text.len() < bytes {
        text.push_str(unit);
    }
    text.truncate(bytes);
    text
}
