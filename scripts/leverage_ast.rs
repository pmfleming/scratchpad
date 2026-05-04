use serde::Serialize;
use std::env;
use std::fs;
use syn::visit::{self, Visit};
use syn::{ExprForLoop, ExprMethodCall, TypePath};

#[derive(Serialize)]
struct LeverageMetrics {
    module_name: String,
    indirection_ratio: f64,
    iterator_leverage_score: f64,
    unsafe_blocks: usize,
    total_leverage_score: f64,
    signals: Vec<String>,
}

struct LeverageVisitor {
    heap_allocating_types: usize,
    inline_types: usize,
    iterator_methods: usize,
    raw_loops: usize,
    unsafe_blocks: usize,
}

impl LeverageVisitor {
    fn new() -> Self {
        Self {
            heap_allocating_types: 0,
            inline_types: 0,
            iterator_methods: 0,
            raw_loops: 0,
            unsafe_blocks: 0,
        }
    }
}

impl<'ast> Visit<'ast> for LeverageVisitor {
    fn visit_type_path(&mut self, i: &'ast TypePath) {
        if let Some(segment) = i.path.segments.last() {
            let ident = segment.ident.to_string();
            match ident.as_str() {
                "Box" | "Rc" | "Arc" | "Mutex" | "RwLock" => {
                    self.heap_allocating_types += 1;
                }
                "Vec" | "Option" | "Result" | "String" | "HashMap" | "HashSet" => {
                    self.inline_types += 1;
                }
                _ => {}
            }
        }
        visit::visit_type_path(self, i);
    }

    fn visit_expr_for_loop(&mut self, i: &'ast ExprForLoop) {
        self.raw_loops += 1;
        visit::visit_expr_for_loop(self, i);
    }

    fn visit_expr_method_call(&mut self, i: &'ast ExprMethodCall) {
        let method_name = i.method.to_string();
        if [
            "iter",
            "into_iter",
            "iter_mut",
            "map",
            "filter",
            "fold",
            "reduce",
            "collect",
        ]
        .contains(&method_name.as_str())
        {
            self.iterator_methods += 1;
        }
        visit::visit_expr_method_call(self, i);
    }

    fn visit_expr_unsafe(&mut self, i: &'ast syn::ExprUnsafe) {
        self.unsafe_blocks += 1;
        visit::visit_expr_unsafe(self, i);
    }

    fn visit_item_impl(&mut self, i: &'ast syn::ItemImpl) {
        if i.unsafety.is_some() {
            self.unsafe_blocks += 1;
        }
        visit::visit_item_impl(self, i);
    }

    fn visit_item_trait(&mut self, i: &'ast syn::ItemTrait) {
        if i.unsafety.is_some() {
            self.unsafe_blocks += 1;
        }
        visit::visit_item_trait(self, i);
    }

    fn visit_item_fn(&mut self, i: &'ast syn::ItemFn) {
        if i.sig.unsafety.is_some() {
            self.unsafe_blocks += 1;
        }
        visit::visit_item_fn(self, i);
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: leverage_ast <paths_file>");
        return;
    }

    let paths_file = &args[1];
    let paths_content = match fs::read_to_string(paths_file) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error reading paths file '{}': {}", paths_file, e);
            return;
        }
    };

    let mut results = Vec::new();

    for line in paths_content.lines() {
        let path = line.trim();
        if path.is_empty() {
            continue;
        }

        let content = match fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Error reading file '{}': {}", path, e);
                continue;
            }
        };

        let file = match syn::parse_file(&content) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("Error parsing file '{}': {}", path, e);
                continue;
            }
        };

        let mut visitor = LeverageVisitor::new();
        visitor.visit_file(&file);

        let total_types = visitor.heap_allocating_types + visitor.inline_types;
        let indirection_ratio = if total_types > 0 {
            (visitor.heap_allocating_types as f64 / total_types as f64) * 100.0
        } else {
            0.0
        };

        let total_loops = visitor.iterator_methods + visitor.raw_loops;
        let iterator_leverage_score = if total_loops > 0 {
            (visitor.iterator_methods as f64 / total_loops as f64) * 100.0
        } else if visitor.raw_loops == 0 {
            100.0
        } else {
            0.0
        };

        let mut signals = Vec::new();
        if indirection_ratio > 20.0 {
            signals.push(format!("high indirection {:.1}%", indirection_ratio));
        }
        if iterator_leverage_score < 50.0 && total_loops > 5 {
            signals.push(format!(
                "low iterator leverage {:.1}%",
                iterator_leverage_score
            ));
        }
        if visitor.unsafe_blocks > 0 {
            signals.push(format!("unsafe blocks {}", visitor.unsafe_blocks));
        }

        let safety_penalty = (visitor.unsafe_blocks as f64 * 5.0).min(50.0);
        let total_leverage_score =
            ((100.0 - indirection_ratio) * 0.4) + (iterator_leverage_score * 0.6) - safety_penalty;
        let total_leverage_score = total_leverage_score.clamp(0.0, 100.0);

        results.push(LeverageMetrics {
            module_name: path.to_string(),
            indirection_ratio,
            iterator_leverage_score,
            unsafe_blocks: visitor.unsafe_blocks,
            total_leverage_score,
            signals,
        });
    }

    println!("{}", serde_json::to_string_pretty(&results).unwrap());
}
