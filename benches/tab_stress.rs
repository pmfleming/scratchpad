use criterion::{BatchSize, BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use rand::RngExt;
use scratchpad::app::domain::{BufferState, SplitAxis, WorkspaceTab};
use scratchpad::app::ui::editor_content::{make_control_chars_clean, make_control_chars_visible};

const KB: usize = 1024;
const MB: usize = 1024 * KB;

fn plain_text_of_size(target_bytes: usize) -> String {
    let line = "The quick brown fox jumps over the lazy dog 0123456789.\n";
    let repeats = (target_bytes / line.len()).max(1);
    let mut text = String::with_capacity(repeats * line.len());
    for _ in 0..repeats {
        text.push_str(line);
    }
    text
}

fn noisy_text_of_size(target_bytes: usize) -> String {
    let chunk = "\u{001B}[31mALERT\u{001B}[0m\tpayload\u{0008}\u{0008}ok\r\n\u{0007}\u{000C}\n";
    let repeats = (target_bytes / chunk.len()).max(1);
    let mut text = String::with_capacity(repeats * chunk.len());
    for _ in 0..repeats {
        text.push_str(chunk);
    }
    text
}

fn make_buffer(name: String, content: String) -> BufferState {
    BufferState::new(name, content, None)
}

fn build_tabs(tab_count: usize) -> Vec<WorkspaceTab> {
    (0..tab_count)
        .map(|i| {
            let buffer = make_buffer(
                format!("tab_{i}.txt"),
                format!("Content for tab {i}\n{}", "x".repeat(256)),
            );
            WorkspaceTab::new(buffer)
        })
        .collect()
}

fn combine_tabs(tabs: &mut Vec<WorkspaceTab>, source_idx: usize, target_idx: usize) {
    if source_idx == target_idx || source_idx >= tabs.len() || target_idx >= tabs.len() {
        return;
    }

    let source_tab = tabs.remove(source_idx);
    let adjusted_target_idx = if source_idx < target_idx {
        target_idx - 1
    } else {
        target_idx
    };
    let target_tab = &mut tabs[adjusted_target_idx];
    let _ = target_tab.combine_with_tab(source_tab, SplitAxis::Horizontal, false, 0.5);
}

fn run_tab_stress_cycle(tab_count: usize) {
    let mut tabs = build_tabs(tab_count);
    let mut rng = rand::rng();
    let iterations = tab_count.clamp(5, 32) / 2;

    for _ in 0..iterations.max(1) {
        let tab_index = rng.random_range(0..tabs.len());
        let tab = &mut tabs[tab_index];
        tab.split_active_view(SplitAxis::Vertical);

        if tab.views.len() > 1 {
            let view_id = tab.views[0].id;
            if let Some(promoted) = tab.promote_view_to_new_tab(view_id) {
                tabs.push(promoted);
            }
        }
    }

    if tabs.len() > 2 {
        combine_tabs(&mut tabs, 0, 1);
    }
}

fn build_tile_heavy_tab(tile_count: usize, content: &str) -> WorkspaceTab {
    let mut tab = WorkspaceTab::new(make_buffer("root.txt".to_owned(), content.to_owned()));
    for i in 1..tile_count {
        let axis = if i % 2 == 0 {
            SplitAxis::Vertical
        } else {
            SplitAxis::Horizontal
        };
        let _ = tab.open_buffer_with_balanced_layout(make_buffer(
            format!("tile_{i}.txt"),
            content.to_owned(),
        ));
        let _ = tab.split_active_view(axis);
    }
    tab
}

fn exercise_tile_heavy_tab(tab: &mut WorkspaceTab) {
    let _ = tab.rebalance_views_equally();
    let _ = tab.split_active_view(SplitAxis::Vertical);
    if tab.views.len() > 2 {
        let close_index = tab.views.len() / 3;
        let view_id = tab.views[close_index].id;
        let _ = tab.close_view(view_id);
    }
}

fn bench_large_file_loads(c: &mut Criterion) {
    let mut group = c.benchmark_group("large_file_load");
    for bytes in [64 * KB, 256 * KB, MB] {
        group.throughput(Throughput::Bytes(bytes as u64));
        let text = plain_text_of_size(bytes);
        group.bench_with_input(BenchmarkId::from_parameter(bytes), &text, |b, text| {
            b.iter_batched(
                || text.clone(),
                |content| {
                    let buffer = make_buffer(format!("plain_{bytes}.txt"), content);
                    criterion::black_box(buffer.line_count);
                    criterion::black_box(buffer.artifact_summary.has_control_chars());
                },
                BatchSize::LargeInput,
            );
        });
    }
    group.finish();
}

fn bench_control_char_file_workflows(c: &mut Criterion) {
    {
        let mut load_group = c.benchmark_group("control_char_load");
        for bytes in [64 * KB, 256 * KB] {
            let text = noisy_text_of_size(bytes);
            load_group.throughput(Throughput::Bytes(bytes as u64));
            load_group.bench_with_input(BenchmarkId::from_parameter(bytes), &text, |b, text| {
                b.iter_batched(
                    || text.clone(),
                    |content| {
                        let buffer = make_buffer(format!("noisy_{bytes}.txt"), content);
                        criterion::black_box(buffer.artifact_summary.status_text());
                    },
                    BatchSize::LargeInput,
                );
            });
        }
        load_group.finish();
    }

    {
        let mut visible_group = c.benchmark_group("control_char_visible");
        for bytes in [64 * KB, 256 * KB] {
            let text = noisy_text_of_size(bytes);
            visible_group.throughput(Throughput::Bytes(bytes as u64));
            visible_group.bench_with_input(BenchmarkId::from_parameter(bytes), &text, |b, text| {
                b.iter(|| criterion::black_box(make_control_chars_visible(text)));
            });
        }
        visible_group.finish();
    }

    {
        let mut clean_group = c.benchmark_group("control_char_clean");
        for bytes in [64 * KB, 256 * KB] {
            let text = noisy_text_of_size(bytes);
            clean_group.throughput(Throughput::Bytes(bytes as u64));
            clean_group.bench_with_input(BenchmarkId::from_parameter(bytes), &text, |b, text| {
                b.iter(|| criterion::black_box(make_control_chars_clean(text)));
            });
        }
        clean_group.finish();
    }
}

fn bench_tab_scaling(c: &mut Criterion) {
    let mut group = c.benchmark_group("tab_count_scale");
    for tab_count in [10usize, 100, 500] {
        group.bench_with_input(
            BenchmarkId::from_parameter(tab_count),
            &tab_count,
            |b, &count| {
                b.iter(|| run_tab_stress_cycle(count));
            },
        );
    }
    group.finish();
}

fn bench_tile_scaling(c: &mut Criterion) {
    let mut group = c.benchmark_group("tile_count_scale");
    let content = plain_text_of_size(64 * KB);
    for tile_count in [4usize, 16, 64] {
        group.bench_with_input(
            BenchmarkId::from_parameter(tile_count),
            &tile_count,
            |b, &count| {
                b.iter_batched(
                    || build_tile_heavy_tab(count, &content),
                    |mut tab| exercise_tile_heavy_tab(&mut tab),
                    BatchSize::LargeInput,
                );
            },
        );
    }
    group.finish();
}

fn bench_tab_operations(c: &mut Criterion) {
    c.bench_function("tab_stress_operations", |b| {
        b.iter(|| run_tab_stress_cycle(10));
    });
}

criterion_group!(
    benches,
    bench_tab_operations,
    bench_large_file_loads,
    bench_control_char_file_workflows,
    bench_tab_scaling,
    bench_tile_scaling
);
criterion_main!(benches);
