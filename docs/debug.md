# Debugging & performance inspection

quill ships a backend-neutral performance inspection system split across two
crates:

| Crate | Role |
|---|---|
| `draw_profile` | collects `FrameStats`, aggregates them, audits a frame's `DrawList` |
| `draw_debug_ui` | draws a `Profiler` + `InspectionReport` as a `draw_ui` panel |

Neither crate references a backend, browser API, or GPU object, so both are
verified with native `cargo test`.

## 1. Measure phases in the host

`draw_profile` deliberately never calls `Instant::now()`. The host owns the frame
loop, samples each pipeline phase, and feeds milliseconds in. This keeps the
model deterministic and the tests exact.

```rust
use std::time::Instant;
use draw_profile::{inspect, FrameCounters, FrameStats, Profiler, StageTimes};

let mut profiler = Profiler::new(); // bounded history, enabled

// ... once per frame ...
let t0 = Instant::now();
demo.update(viewport, dt);
let t1 = Instant::now();
demo.layout(viewport);
let t2 = Instant::now();

let mut ctx = draw_render::PaintContext::new();
demo.paint(&mut ctx);
let list = ctx.into_draw_list();
let t3 = Instant::now();

// ... submit `list` to a RenderBackend ...
let t4 = Instant::now();

let ms = |d: std::time::Duration| d.as_secs_f32() * 1000.0;
let stats = FrameStats {
    index: profiler.next_index(),
    frame_ms: ms(t4 - t0),
    stages: StageTimes::new(ms(t1 - t0), ms(t2 - t1), ms(t3 - t2), ms(t4 - t3)),
    counters: FrameCounters::new(scene_nodes, controls, list.len(), 1),
};
profiler.record(stats);

// Audit the frame we just produced.
let report = inspect(&list, &stats);
```

Disable profiling on the hot path with `profiler.set_enabled(false)` /
`profiler.toggle()` — while disabled `record` is a no-op.

## 2. Read the summary

```rust
if let Some(summary) = profiler.summary() {
    println!(
        "fps {:.0}  avg {:.2}ms  min {:.2}ms  max {:.2}ms  peak commands {}",
        summary.fps(),
        summary.avg_frame_ms,
        summary.min_frame_ms,
        summary.max_frame_ms,
        summary.max_draw_commands,
    );
    for phase in draw_profile::Phase::ALL {
        println!("  {}: {:.2}ms", phase.label(), summary.avg_stages.get(phase));
    }
}
```

## 3. Read the findings

`inspect` returns an `InspectionReport`. Findings are aggregated by
`FindingCode` (one entry per kind, with a `count`), so a noisy frame stays
readable.

```rust
for finding in report.findings() {
    println!(
        "[{}] {} x{}: {}",
        finding.severity.label(),
        finding.code.label(),
        finding.count,
        finding.message,
    );
}
```

### Finding codes

| Code | Severity | Meaning |
|---|---|---|
| `UnbalancedSaveRestore` | Error | `Save` without matching `Restore` |
| `UnmatchedRestore` | Error | `Restore` with an empty stack |
| `NonFiniteGeometry` | Error | `NaN`/`inf` geometry, transform or font size |
| `DegenerateRect` | Warning | zero/negative-width or height rect |
| `DegenerateCircle` | Warning | radius ≤ 0 |
| `DegenerateStroke` | Warning | stroke width ≤ 0 |
| `DegenerateClip` | Warning | clip rect with zero/negative area |
| `OpacityOutOfRange` | Warning | opacity outside `0.0..=1.0` |
| `CommandBudgetExceeded` | Warning → Error | more commands than the budget (Error at > 2×) |
| `FrameTimeBudgetExceeded` | Warning → Error | frame slower than the budget (Error at > 2×) |
| `EntityBudgetExceeded` | Warning → Error | scene nodes + controls over budget |
| `EmptyDrawList` | Info | a `DrawList` with no commands |
| `EmptyText` | Info | a text command with no visible glyphs |

### Thresholds

```rust
use draw_profile::{inspect_with, InspectionConfig};

let config = InspectionConfig {
    max_draw_commands: 2048, // default
    max_frame_ms: 16.7,      // ~60 FPS
    max_entities: 10_000,
};
let report = inspect_with(&list, &stats, &config);
```

## 4. Show the debug overlay

`DebugOverlay` owns its own `Ui` tree, so it never disturbs the application's
layout or hit testing. Paint it after the application UI:

```rust
use draw_debug_ui::DebugOverlay;

let mut overlay = DebugOverlay::new(); // top-right by default

// per frame, after `demo.paint(&mut ctx)`:
overlay.update(&profiler, &report, viewport); // uses the previous frame's report
overlay.paint(&mut ctx);

// input: forward events; the panel consumes pointer events over itself
if !overlay.handle_input(event).is_handled() {
    demo.event(event);
}

// toggle visibility (no-op while closed)
overlay.toggle();
```

The panel shows: FPS, current frame time (avg/max), per-phase averages,
commands (current/peak), node & control counts, finding counts by severity, and
the top findings.

### Styling / placement

```rust
use draw_debug_ui::{Corner, DebugOverlay, OverlayConfig};

let overlay = DebugOverlay::with_config(OverlayConfig {
    corner: Corner::BottomLeft,
    width: 340.0,
    max_finding_rows: 6,
    ..OverlayConfig::default()
});
```

## 5. Demo

`demos/wgpu_demo` measures every phase, records it in a `Profiler`, audits the
frame, and shows the overlay. Press `` ` `` (backtick) to toggle the panel.

```bash
cargo run -p wgpu_demo --release
```

The window overlay is not screenshot-verified (see `docs/testing.md`); the
profiler, inspector and overlay are all covered by native `cargo test`.
