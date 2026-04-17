use proc_macro2::{Span, TokenStream, TokenTree};
use std::collections::HashSet;
use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use syn::spanned::Spanned;
use syn::visit::Visit;

#[derive(Clone, Copy, Default)]
struct CountOptions {
    exclude_tests: bool,
}

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

fn count_lines_in_file(path: &Path, options: CountOptions) -> io::Result<Stats> {
    if options.exclude_tests && is_test_only_file(path) {
        return Ok(Stats::new());
    }

    let source = fs::read_to_string(path)?;
    let excluded_lines = excluded_test_lines(&source, options.exclude_tests);
    Ok(count_lines_in_source(&source, &excluded_lines))
}

fn count_lines_in_source(source: &str, excluded_lines: &HashSet<usize>) -> Stats {
    let mut stats = Stats::new();
    let mut counted_any_line = false;

    for (index, line) in source.lines().enumerate() {
        let line_number = index + 1;
        if excluded_lines.contains(&line_number) {
            continue;
        }

        counted_any_line = true;
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

    if counted_any_line {
        stats.files = 1;
    }

    stats
}

fn visit_dirs(dir: &Path, stats: &mut Stats, options: CountOptions) -> io::Result<()> {
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
                if options.exclude_tests && name == Some("tests") {
                    continue;
                }
                visit_dirs(&path, stats, options)?;
            } else if path.extension().and_then(|s| s.to_str()) == Some("rs")
                && let Ok(file_stats) = count_lines_in_file(&path, options)
            {
                stats.add(&file_stats);
            }
        }
    }
    Ok(())
}

fn is_test_only_file(path: &Path) -> bool {
    path.file_stem().and_then(|stem| stem.to_str()) == Some("tests")
        || path
            .components()
            .any(|component| component.as_os_str() == "tests")
}

fn excluded_test_lines(source: &str, exclude_tests: bool) -> HashSet<usize> {
    if !exclude_tests {
        return HashSet::new();
    }

    syn::parse_file(source)
        .map(|file| {
            let mut visitor = TestLineCollector::default();
            visitor.visit_file(&file);
            visitor.lines
        })
        .unwrap_or_default()
}

#[derive(Default)]
struct TestLineCollector {
    lines: HashSet<usize>,
}

impl TestLineCollector {
    fn exclude_span(&mut self, span: Span) {
        for line in span.start().line..=span.end().line {
            self.lines.insert(line);
        }
    }
}

impl<'ast> Visit<'ast> for TestLineCollector {
    fn visit_item(&mut self, item: &'ast syn::Item) {
        if item_is_test_only(item) {
            self.exclude_span(item.span());
            return;
        }

        syn::visit::visit_item(self, item);
    }

    fn visit_impl_item(&mut self, item: &'ast syn::ImplItem) {
        if impl_item_is_test_only(item) {
            self.exclude_span(item.span());
            return;
        }

        syn::visit::visit_impl_item(self, item);
    }
}

fn item_is_test_only(item: &syn::Item) -> bool {
    match item {
        syn::Item::Const(item) => has_test_only_attr(&item.attrs),
        syn::Item::Enum(item) => has_test_only_attr(&item.attrs),
        syn::Item::ExternCrate(item) => has_test_only_attr(&item.attrs),
        syn::Item::Fn(item) => has_test_only_attr(&item.attrs),
        syn::Item::ForeignMod(item) => has_test_only_attr(&item.attrs),
        syn::Item::Impl(item) => has_test_only_attr(&item.attrs),
        syn::Item::Macro(item) => has_test_only_attr(&item.attrs),
        syn::Item::Mod(item) => has_test_only_attr(&item.attrs),
        syn::Item::Static(item) => has_test_only_attr(&item.attrs),
        syn::Item::Struct(item) => has_test_only_attr(&item.attrs),
        syn::Item::Trait(item) => has_test_only_attr(&item.attrs),
        syn::Item::TraitAlias(item) => has_test_only_attr(&item.attrs),
        syn::Item::Type(item) => has_test_only_attr(&item.attrs),
        syn::Item::Union(item) => has_test_only_attr(&item.attrs),
        syn::Item::Use(item) => has_test_only_attr(&item.attrs),
        _ => false,
    }
}

fn impl_item_is_test_only(item: &syn::ImplItem) -> bool {
    match item {
        syn::ImplItem::Const(item) => has_test_only_attr(&item.attrs),
        syn::ImplItem::Fn(item) => has_test_only_attr(&item.attrs),
        syn::ImplItem::Macro(item) => has_test_only_attr(&item.attrs),
        syn::ImplItem::Type(item) => has_test_only_attr(&item.attrs),
        syn::ImplItem::Verbatim(_) => false,
        _ => false,
    }
}

fn has_test_only_attr(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(attribute_is_test_only)
}

fn attribute_is_test_only(attr: &syn::Attribute) -> bool {
    attr.path().is_ident("test")
        || (attr.path().is_ident("cfg")
            && token_stream_mentions_test(
                attr.meta
                    .require_list()
                    .map(|list| list.tokens.clone())
                    .unwrap_or_default(),
            ))
}

fn token_stream_mentions_test(tokens: TokenStream) -> bool {
    tokens.into_iter().any(|token| match token {
        TokenTree::Ident(ident) => ident == "test",
        TokenTree::Group(group) => token_stream_mentions_test(group.stream()),
        _ => false,
    })
}

fn parse_args() -> Result<CountOptions, String> {
    let mut options = CountOptions::default();

    for arg in env::args().skip(1) {
        match arg.as_str() {
            "--exclude-tests" => options.exclude_tests = true,
            "-h" | "--help" => {
                print_usage();
                std::process::exit(0);
            }
            _ => return Err(format!("unrecognized argument: {arg}")),
        }
    }

    Ok(options)
}

fn print_usage() {
    println!("Usage: cargo run --bin count_lines -- [--exclude-tests]");
}

fn main() -> io::Result<()> {
    let options =
        parse_args().map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
    let mut stats = Stats::new();
    let root = PathBuf::from(".");
    visit_dirs(&root, &mut stats, options)?;

    if options.exclude_tests {
        println!("Rust Code Metrics for this project (excluding test code):");
    } else {
        println!("Rust Code Metrics for this project:");
    }
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

#[cfg(test)]
mod tests {
    use super::{count_lines_in_source, excluded_test_lines, is_test_only_file};
    use std::path::Path;

    #[test]
    fn test_only_paths_are_detected() {
        assert!(is_test_only_file(Path::new("tests/app_tests.rs")));
        assert!(is_test_only_file(Path::new("src/app/commands/tests.rs")));
        assert!(!is_test_only_file(Path::new("src/app/commands.rs")));
    }

    #[test]
    fn excluding_tests_removes_cfg_test_modules_from_counts() {
        let source = concat!(
            "fn production() {}\n",
            "\n",
            "#[cfg(test)]\n",
            "mod tests {\n",
            "    #[test]\n",
            "    fn counts_as_test() {}\n",
            "}\n"
        );

        let stats = count_lines_in_source(source, &excluded_test_lines(source, true));

        assert_eq!(stats.files, 1);
        assert_eq!(stats.total_lines, 2);
        assert_eq!(stats.code_lines, 1);
        assert_eq!(stats.blank_lines, 1);
        assert_eq!(stats.comment_lines, 0);
    }

    #[test]
    fn excluding_tests_removes_test_functions_from_counts() {
        let source = concat!("fn production() {}\n", "#[test]\n", "fn unit_test() {}\n");

        let stats = count_lines_in_source(source, &excluded_test_lines(source, true));

        assert_eq!(stats.files, 1);
        assert_eq!(stats.total_lines, 1);
        assert_eq!(stats.code_lines, 1);
    }
}
