# Debugging & performance inspection

quill provides two independent, backend-neutral debug tools:

| Tool | Crate | Draws |
|---|---|---|
| Component debug drawing | `draw_ui` + `draw_debug_ui::DebugOverlay` | yellow border + `Name #id` per control |
| Performance panel | `draw_profile` + `draw_debug_ui::PerformanceOverlay` | FPS, phase timings, counters, findings |

Both emit ordinary `DrawCommand`s, so any backend renders them, and both are
verified with native `cargo test`.

---

## 1. Component debug drawing

Every visible `Control` gets a border (yellow by default) and a `Name #id` label
in its top-left corner — the classic engine "debug bounds" view.

### Directly from the UI

```rust
use draw_ui::DebugDrawOptions;

// after painting the app UI into `ctx`:
ui.paint_debug(&mut ctx, &DebugDrawOptions::default());
```

`DebugDrawOptions` controls the look:

```rust
use draw_core::Color;
use draw_ui::DebugDrawOptions;

let options = DebugDrawOptions {
    border_color: Color::YELLOW,
    text_color: Color::YELLOW,
    width: 1.0,
    font_size: 11.0,
    label_offset: draw_core::Vec2::new(2.0, 0.0), // from the control's top-left
    show_names: true,
    show_ids: true,
};
```

The label is `"{name} #{id}"`; disable `show_names` / `show_ids` to change it.

### With a togglable overlay

`draw_debug_ui::DebugOverlay` wraps the above with an open/closed state:

```rust
use draw_debug_ui::DebugOverlay;

let mut debug = DebugOverlay::new(); // visible by default

// per frame, after `app_ui.paint(&mut ctx)`:
debug.paint(&app_ui, &mut ctx);

// toggle (e.g. an F3 key binding)
debug.toggle();
```

It owns no tree, so it draws over any `Ui` you pass in.

---

## 2. Performance inspection (`draw_profile`)

`draw_profile` never reads the clock itself. The host samples `Instant` per
pipeline phase and feeds milliseconds in, which keeps the model deterministic
and the tests exact.

```rust
use std::time::Instant;
use draw_profile::{inspect, FrameCounters, FrameStats, Profiler, StageTimes};

let mut profiler = Profiler::new();

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

### Summary

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
}
```

### Findings

`inspect` returns an `InspectionReport`; findings are aggregated by
`FindingCode` (one entry per kind, with a `count`).

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

Thresholds:

```rust
use draw_profile::{inspect_with, InspectionConfig};

let config = InspectionConfig {
    max_draw_commands: 2048, // default
    max_frame_ms: 16.7,      // ~60 FPS
    max_entities: 10_000,
};
let report = inspect_with(&list, &stats, &config);
```

---

## 3. Performance panel (`PerformanceOverlay`)

`PerformanceOverlay` renders a `Profiler` + `InspectionReport` as an ordinary
`draw_ui` panel in a viewport corner. It owns its own `Ui` tree, so it does not
disturb the application's layout or hit-testing.

```rust
use draw_debug_ui::{Corner, OverlayConfig, PerformanceOverlay};

let mut perf = PerformanceOverlay::new();

// per frame, after the component debug draw:
perf.update(&profiler, &report, viewport);
perf.paint(&mut ctx);

// input: the panel consumes pointer events over itself
if !perf.handle_input(event).is_handled() {
    app.event(event);
}

perf.toggle();
```

The panel shows FPS, current frame time (avg/max), profiler state (`on` /
`paused`), per-phase averages, commands (current/peak), node & control counts,
findings by severity, the top findings, and a key legend.

```rust
let perf = PerformanceOverlay::with_config(OverlayConfig {
    corner: Corner::BottomLeft,
    width: 340.0,
    max_finding_rows: 6,
    ..OverlayConfig::default()
});
```

---

## 4. wgpu demo

`demos/wgpu_demo` wires both tools around its frame loop:

```text
demo.paint -> debug.paint(component bounds) -> perf.paint -> submit
```

```bash
cargo run -p wgpu_demo --release
cargo run -p wgpu_demo --release -- --no-debug-ui          # hide component bounds
cargo run -p wgpu_demo --release -- --performance          # show the perf panel
```

Shortcuts: **F3** / `` ` `` / **d** toggles component bounds, **F4** / **p**
toggles the performance panel, **F5** / **o** toggles the profiler. On macOS the
top-row F-keys are often system keys — use the `` ` `` / **d** / **p** / **o**
fallbacks or hold **Fn**.

The window overlays are not screenshot-verified (see `docs/testing.md`); the
component bounds, profiler, inspector and panel are all covered by native
`cargo test`.
