use scratchpad::app::services::search::{SearchMode, SearchOptions, SearchProgram, search_program};
use std::hint::black_box;
use std::time::Instant;

const MB: usize = 1024 * 1024;
const CAPACITY_SIZES: [usize; 3] = [MB, 50 * MB, 250 * MB];
const UTF8_SEARCH_UNIT: &str = "hay café 東京 needle Привет مرحبا\n";

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
    let mut text = String::with_capacity(bytes + UTF8_SEARCH_UNIT.len());
    while text.len() < bytes {
        text.push_str(UTF8_SEARCH_UNIT);
    }
    truncate_to_char_boundary(&mut text, bytes);
    text
}

fn truncate_to_char_boundary(text: &mut String, max_bytes: usize) {
    if max_bytes >= text.len() {
        return;
    }
    let mut end = max_bytes;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    text.truncate(end);
}
