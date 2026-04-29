use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use scratchpad::app::domain::AnchorBias;
use scratchpad::app::domain::buffer::PieceTreeLite;
use std::time::Duration;

const DOCUMENT_CHARS: usize = 4 * 1024 * 1024;
const ANCHOR_COUNTS: [usize; 5] = [1, 10, 100, 1_000, 10_000];

fn build_tree(anchor_count: usize) -> PieceTreeLite {
    let mut tree = PieceTreeLite::from_string("a".repeat(DOCUMENT_CHARS));
    if anchor_count == 0 {
        return tree;
    }

    for index in 0..anchor_count {
        let offset = (index * DOCUMENT_CHARS / anchor_count).min(DOCUMENT_CHARS);
        let bias = if index % 2 == 0 {
            AnchorBias::Left
        } else {
            AnchorBias::Right
        };
        tree.create_anchor(offset, bias);
    }
    tree
}

fn bench_anchor_insert(c: &mut Criterion) {
    let mut group = c.benchmark_group("piece_tree_anchor_insert");
    group.sample_size(20);
    group.measurement_time(Duration::from_secs(4));

    for anchor_count in ANCHOR_COUNTS {
        group.bench_with_input(
            BenchmarkId::from_parameter(anchor_count),
            &anchor_count,
            |bench, &anchor_count| {
                bench.iter_batched(
                    || build_tree(anchor_count),
                    |mut tree| {
                        tree.insert(DOCUMENT_CHARS / 2, "inserted text\n");
                        criterion::black_box(tree)
                    },
                    criterion::BatchSize::SmallInput,
                );
            },
        );
    }

    group.finish();
}

fn bench_anchor_remove(c: &mut Criterion) {
    let mut group = c.benchmark_group("piece_tree_anchor_remove");
    group.sample_size(20);
    group.measurement_time(Duration::from_secs(4));

    for anchor_count in ANCHOR_COUNTS {
        group.bench_with_input(
            BenchmarkId::from_parameter(anchor_count),
            &anchor_count,
            |bench, &anchor_count| {
                bench.iter_batched(
                    || build_tree(anchor_count),
                    |mut tree| {
                        let start = DOCUMENT_CHARS / 2;
                        tree.remove_char_range(start..start + 16);
                        criterion::black_box(tree)
                    },
                    criterion::BatchSize::SmallInput,
                );
            },
        );
    }

    group.finish();
}

criterion_group!(benches, bench_anchor_insert, bench_anchor_remove);
criterion_main!(benches);
