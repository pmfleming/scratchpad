use std::fs;
use std::io::{self, BufRead};
use std::path::{Path, PathBuf};

struct Stats {
    files: usize,
    total_lines: usize,
    code_lines: usize,
    blank_lines: usize,
    comment_lines: usize,
}

impl Stats {
    fn new() -> Self {
        Self {
            files: 0,
            total_lines: 0,
            code_lines: 0,
            blank_lines: 0,
            comment_lines: 0,
        }
    }

    fn add(&mut self, other: &Stats) {
        self.files += other.files;
        self.total_lines += other.total_lines;
        self.code_lines += other.code_lines;
        self.blank_lines += other.blank_lines;
        self.comment_lines += other.comment_lines;
    }
}

fn count_lines_in_file(path: &Path) -> io::Result<Stats> {
    let file = fs::File::open(path)?;
    let reader = io::BufReader::new(file);
    let mut stats = Stats::new();
    stats.files = 1;

    for line in reader.lines() {
        let line = line?;
        stats.total_lines += 1;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            stats.blank_lines += 1;
        } else if trimmed.starts_with("//") || trimmed.starts_with("/*") {
            // Simplified comment detection
            stats.comment_lines += 1;
        } else {
            stats.code_lines += 1;
        }
    }

    Ok(stats)
}

fn visit_dirs(dir: &Path, stats: &mut Stats) -> io::Result<()> {
    if dir.is_dir() {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                // Ignore target and .git directories
                let name = path.file_name().and_then(|n| n.to_str());
                if name == Some("target") || name == Some(".git") || name == Some(".venv") {
                    continue;
                }
                visit_dirs(&path, stats)?;
            } else if path.extension().and_then(|s| s.to_str()) == Some("rs")
                && let Ok(file_stats) = count_lines_in_file(&path)
            {
                stats.add(&file_stats);
            }
        }
    }
    Ok(())
}

fn main() -> io::Result<()> {
    let mut stats = Stats::new();
    let root = PathBuf::from(".");
    visit_dirs(&root, &mut stats)?;

    println!("Rust Code Metrics for this project:");
    println!("-----------------------------------");
    println!("Files:          {:>10}", stats.files);
    println!("Total Lines:    {:>10}", stats.total_lines);
    println!("Code Lines:     {:>10}", stats.code_lines);
    println!("Comment Lines:  {:>10}", stats.comment_lines);
    println!("Blank Lines:    {:>10}", stats.blank_lines);
    println!("-----------------------------------");
    if stats.files > 0 {
        println!(
            "Avg Lines/File: {:>10.1}",
            stats.total_lines as f64 / stats.files as f64
        );
    }
    println!("-----------------------------------");

    Ok(())
}
