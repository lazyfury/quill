//! The quill CPU pipeline benchmark suite.
//!
//! Run with `cargo bench -p draw_bench_suite`. Every benchmark is named
//! `<group>/<scenario>/<size>` so `--filter` can select a slice:
//!
//! ```bash
//! cargo bench -p draw_bench_suite -- --filter scene/update
//! cargo bench -p draw_bench_suite -- --filter list/
//! cargo bench -p draw_bench_suite -- --save-baseline benches/baseline.txt
//! cargo bench -p draw_bench_suite -- --baseline benches/baseline.txt
//! ```
//!
//! `--baseline` exits with code 1 when any benchmark regresses beyond
//! `--threshold` (percent), which is the CI gate.
//!
//! After the timing table the list scenarios also print *what* they are billed
//! in — nodes and draw commands per frame — plus the full/virtual ratio. Those
//! numbers are machine-independent, so they are the part of the comparison that
//! can be quoted as a fact rather than as a measurement.

use std::fmt::Write as _;

use draw_bench::{
    black_box, finish, format_count, format_time, BenchResult, BenchRunner, RunConfig,
};
use draw_bench_suite::scenarios::{
    ListFullFixture, ListVirtualFixture, SceneFixture, UiFixture, LIST_SCROLL_STEP, LIST_SIZES,
    SIZES,
};
use draw_bench_suite::SinkBackend;
use draw_core::Vec2;
use draw_render::{PaintContext, RenderBackend};

fn main() {
    let config = RunConfig::from_env();
    let runner = BenchRunner::new(&config);
    let mut results = Vec::new();

    for &n in &SIZES {
        // SceneTree update with nothing dirty: measures the dirty-flag traversal.
        push(
            &mut results,
            runner.run(
                format!("scene/update_clean/{n}"),
                || SceneFixture::new(n),
                |fixture| {
                    black_box(fixture.tree.update());
                },
            ),
        );

        // Move every node, then update: measures dirty propagation + recompute.
        push(
            &mut results,
            runner.run(
                format!("scene/update_dirty_all/{n}"),
                || SceneFixture::new(n),
                |fixture| {
                    let ids = &fixture.ids;
                    let tree = &mut fixture.tree;
                    for &id in ids {
                        tree.set_position(id, Vec2::new(1.0, 1.0));
                    }
                    black_box(tree.update());
                },
            ),
        );

        // Paint the whole scene into a fresh DrawList.
        push(
            &mut results,
            runner.run(
                format!("scene/paint/{n}"),
                || SceneFixture::new(n),
                |fixture| {
                    let mut ctx = PaintContext::with_capacity(fixture.expected_commands());
                    fixture.tree.paint(&mut ctx);
                    black_box(ctx.into_draw_list());
                },
            ),
        );

        // UI layout across the control tree.
        push(
            &mut results,
            runner.run(
                format!("ui/layout/{n}"),
                || UiFixture::new(n),
                |fixture| {
                    let viewport = fixture.viewport;
                    fixture.layout(viewport);
                },
            ),
        );

        // Worst-case reverse hit test (point over the bottom-most label).
        push(
            &mut results,
            runner.run(
                format!("ui/hit_test/{n}"),
                || UiFixture::new(n),
                |fixture| {
                    black_box(fixture.hit_test(fixture.first_center));
                },
            ),
        );

        // Paint the whole UI into a fresh DrawList.
        push(
            &mut results,
            runner.run(
                format!("ui/paint/{n}"),
                || UiFixture::new(n),
                |fixture| {
                    let mut ctx = PaintContext::with_capacity(fixture.ids.len() * 3 + 8);
                    fixture.paint(&mut ctx);
                    black_box(ctx.into_draw_list());
                },
            ),
        );

        // End-to-end CPU frame: layout -> paint -> submit -> end.
        push(
            &mut results,
            runner.run(
                format!("pipeline/ui_frame/{n}"),
                || (UiFixture::new(n), SinkBackend::new()),
                |(fixture, sink)| {
                    let viewport = fixture.viewport;
                    let mut ctx = PaintContext::with_capacity(fixture.ids.len() * 3 + 8);
                    fixture.layout(viewport);
                    fixture.paint(&mut ctx);
                    let list = ctx.into_draw_list();
                    sink.begin_frame(viewport).unwrap();
                    sink.submit(&list).unwrap();
                    sink.end_frame().unwrap();
                    black_box(sink.commands());
                },
            ),
        );
    }

    for &n in &LIST_SIZES {
        // One scrolling frame of the naive list: everything mounted, the
        // container moves. Scrolling is what the list component exists for, so
        // it is what gets measured — a couple of rows per frame.
        push(
            &mut results,
            runner.run(
                format!("list/scroll_full/{n}"),
                || ListFullFixture::new(n),
                |fixture| {
                    fixture.scroll_by(LIST_SCROLL_STEP);
                    fixture.layout();
                    let mut ctx = PaintContext::with_capacity(fixture.command_hint());
                    fixture.paint(&mut ctx);
                    black_box(ctx.into_draw_list());
                },
            ),
        );

        // The same scrolling frame over the pool: the offset moves the rows the
        // viewport already holds and rebinds their text, and nothing is mounted.
        push(
            &mut results,
            runner.run(
                format!("list/scroll_virtual/{n}"),
                || ListVirtualFixture::new(n),
                |fixture| {
                    fixture.scroll_by(LIST_SCROLL_STEP);
                    fixture.layout();
                    let mut ctx = PaintContext::with_capacity(fixture.command_hint());
                    fixture.paint(&mut ctx);
                    black_box(ctx.into_draw_list());
                },
            ),
        );
    }

    let code = finish(&config, &results);
    print_list_summary(&config, &results);
    std::process::exit(code);
}

/// The structural half of the list comparison, plus the ratio the timings give.
///
/// The shapes are rebuilt here rather than captured during the run because the
/// harness owns its fixtures; only the sizes the filter kept are built.
fn print_list_summary(config: &RunConfig, results: &[BenchResult]) {
    let sizes: Vec<usize> = LIST_SIZES
        .into_iter()
        .filter(|n| {
            config.matches(&format!("list/scroll_full/{n}"))
                || config.matches(&format!("list/scroll_virtual/{n}"))
        })
        .collect();
    if sizes.is_empty() {
        return;
    }

    let mut out = String::new();
    let _ = writeln!(out, "\nlist frame shape (what one frame is billed in)");
    let _ = writeln!(
        out,
        "{:>9}  {:>10} {:>10}  {:>10} {:>10} {:>6} {:>8}  {:>7}",
        "rows", "full.ctrl", "full.cmds", "virt.ctrl", "virt.cmds", "pool", "visible", "cmds"
    );
    for n in &sizes {
        let full = ListFullFixture::new(*n).shape();
        let virtual_list = ListVirtualFixture::new(*n);
        let virt = virtual_list.shape();
        let ratio = full.commands as f64 / virt.commands.max(1) as f64;
        let _ = writeln!(
            out,
            "{:>9}  {:>10} {:>10}  {:>10} {:>10} {:>6} {:>8}  {:>6.1}x",
            n,
            format_count(full.controls as f64),
            format_count(full.commands as f64),
            format_count(virt.controls as f64),
            format_count(virt.commands as f64),
            virtual_list.pool_size(),
            virtual_list.visible_rows(),
            ratio
        );
    }

    let _ = writeln!(out, "\nlist scroll frame (median per frame)");
    let _ = writeln!(
        out,
        "{:>9}  {:>10}  {:>10}  {:>8}",
        "rows", "full", "virtual", "speedup"
    );
    for n in &sizes {
        let (Some(full), Some(virt)) = (
            median(results, &format!("list/scroll_full/{n}")),
            median(results, &format!("list/scroll_virtual/{n}")),
        ) else {
            continue;
        };
        let _ = writeln!(
            out,
            "{:>9}  {:>10}  {:>10}  {:>7.1}x",
            n,
            format_time(full),
            format_time(virt),
            full / virt.max(1.0)
        );
    }

    print!("{out}");
}

/// Median nanoseconds per iteration for `name`, if that scenario ran.
fn median(results: &[BenchResult], name: &str) -> Option<f64> {
    results
        .iter()
        .find(|result| result.name == name)
        .map(BenchResult::median_ns)
}

fn push(results: &mut Vec<BenchResult>, result: Option<BenchResult>) {
    if let Some(result) = result {
        results.push(result);
    }
}
