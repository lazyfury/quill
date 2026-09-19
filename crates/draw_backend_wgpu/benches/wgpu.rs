//! Optional GPU benchmark for the `wgpu` backend.
//!
//! This measures the offscreen path end to end: CPU command execution, GPU
//! submission, and a blocking `read_pixels` sync. The readback makes every frame
//! deterministic but dominates the number, so read these results as "cost of a
//! readback-synchronized frame", not raw GPU raster time.
//!
//! If no adapter is available (GPU-less CI, headless box) the benchmarks are
//! skipped and the process exits successfully, matching the crate's pixel tests.
//!
//! ```bash
//! cargo bench -p draw_backend_wgpu --bench wgpu
//! ```

use draw_backend_wgpu::WgpuBackend;
use draw_bench::{black_box, finish, BenchResult, BenchRunner, RunConfig};
use draw_core::{Color, Rect, Size, Vec2, ViewportSize};
use draw_render::{PaintContext, RenderBackend};

/// Entity counts; readback cost dominates, so the range is modest.
const SIZES: [usize; 3] = [16, 256, 2_048];
const TARGET_SIZE: f32 = 512.0;

fn main() {
    let config = RunConfig::from_env();
    let runner = BenchRunner::new(&config);

    if let Err(error) = WgpuBackend::new() {
        eprintln!("skipping wgpu benchmarks: {error}");
        return;
    }

    let viewport = ViewportSize::new(Size::new(TARGET_SIZE, TARGET_SIZE));
    let mut results = Vec::new();

    for &n in &SIZES {
        let result = runner.run(
            format!("wgpu/render/{n}"),
            || {
                let backend = WgpuBackend::new().expect("probe established an adapter");
                let list = build_list(n);
                (backend, list)
            },
            |(backend, list)| {
                backend.begin_frame(viewport).unwrap();
                backend.submit(list).unwrap();
                backend.end_frame().unwrap();
                let pixels = backend.read_pixels().unwrap();
                black_box(pixels.pixel(0, 0));
            },
        );
        push(&mut results, result);
    }

    std::process::exit(finish(&config, &results));
}

/// Builds a `DrawList` of `n` colored rectangles in a deterministic grid.
fn build_list(n: usize) -> draw_render::DrawList {
    let columns = (n as f64).sqrt().ceil().max(1.0) as usize;
    let cell = TARGET_SIZE / columns as f32;
    let mut ctx = PaintContext::with_capacity(n);
    for i in 0..n {
        let x = (i % columns) as f32 * cell;
        let y = (i / columns) as f32 * cell;
        let shade = (i % 255) as f32 / 255.0;
        ctx.fill_rect(
            Rect::from_min_size(Vec2::new(x, y), Size::splat(cell)),
            Color::new(shade, 0.5, 1.0 - shade, 1.0),
        );
    }
    ctx.into_draw_list()
}

fn push(results: &mut Vec<BenchResult>, result: Option<BenchResult>) {
    if let Some(result) = result {
        results.push(result);
    }
}
