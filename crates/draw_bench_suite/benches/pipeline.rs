//! The quill CPU pipeline benchmark suite.
//!
//! Run with `cargo bench -p draw_bench_suite`. Every benchmark is named
//! `<group>/<scenario>/<size>` so `--filter` can select a slice:
//!
//! ```bash
//! cargo bench -p draw_bench_suite -- --filter scene/update
//! cargo bench -p draw_bench_suite -- --save-baseline benches/baseline.txt
//! cargo bench -p draw_bench_suite -- --baseline benches/baseline.txt
//! ```
//!
//! `--baseline` exits with code 1 when any benchmark regresses beyond
//! `--threshold` (percent), which is the CI gate.

use draw_bench::{black_box, finish, BenchResult, BenchRunner, RunConfig};
use draw_bench_suite::scenarios::{SceneFixture, UiFixture, SIZES};
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
                    fixture.ui.layout(viewport);
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
                    black_box(fixture.ui.hit_test(fixture.first_center));
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
                    fixture.ui.paint(&mut ctx);
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
                    fixture.ui.layout(viewport);
                    fixture.ui.paint(&mut ctx);
                    let list = ctx.into_draw_list();
                    sink.begin_frame(viewport).unwrap();
                    sink.submit(&list).unwrap();
                    sink.end_frame().unwrap();
                    black_box(sink.commands());
                },
            ),
        );
    }

    std::process::exit(finish(&config, &results));
}

fn push(results: &mut Vec<BenchResult>, result: Option<BenchResult>) {
    if let Some(result) = result {
        results.push(result);
    }
}
