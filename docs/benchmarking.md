# Benchmarking

quill has two performance tools, and they answer different questions:

| Tool | Crate | Question | Output |
|---|---|---|---|
| Profiler | `draw_profile` | *Where* did this frame's time go? | per-phase ms, counts, findings |
| Benchmark | `draw_bench` | *Is this path faster or slower than before?* | median/p95 ns, regression verdict |

The profiler is a microscope; a benchmark is a ruler. Use the profiler to find
*what* to optimize, and the benchmark to prove the change helped and to keep it
from regressing.

---

## 1. The harness (`draw_bench`)

`draw_bench` is a small, dependency-free benchmarking framework (`std` only, no
randomness). It warms up, calibrates an iteration count so one sample runs for
about `sample_time`, then collects `samples` timed samples.

```rust
use draw_bench::{finish, BenchRunner, RunConfig};

fn main() {
    let config = RunConfig::from_env();
    let runner = BenchRunner::new(&config);
    let mut results = Vec::new();

    if let Some(result) = runner.run(
        "my/path/1000",
        || build_state(1000),          // setup, run once, not timed
        |state| { state.step(); },     // measured routine, called in a loop
    ) {
        results.push(result);
    }

    std::process::exit(finish(&config, &results));
}
```

A bench target uses `harness = false` in `Cargo.toml`, so `cargo bench` runs
`main`:

```toml
[[bench]]
name = "pipeline"
harness = false
```

### Important

- **Always consume the result.** Wrap the value you care about in
  `draw_bench::black_box(..)` so the optimizer cannot delete the work under
  test.
- **Do not benchmark debug builds.** `cargo bench` uses the `bench` profile,
  which the workspace pins to `opt-level = 3` (the `release` profile is
  size-optimized).
- **Keep scenarios deterministic.** No randomness, no I/O, no wall-clock input.
  Build fixtures from a fixed size so a saved baseline stays meaningful.

### CLI

```text
--filter <substr>        only run benchmarks whose name contains <substr>
--baseline <path>        compare against a saved baseline
--save-baseline <path>   write the current results as a baseline
--threshold <percent>    regression threshold (default 5)
--warmup-ms <ms>         warmup per benchmark (default 200)
--sample-ms <ms>         target duration of one sample (default 30)
--samples <n>            timed samples per benchmark (default 50)
--max-iters <n>          cap on iterations per sample
-h, --help               usage
```

`cargo bench` also runs the crate's lib-test target with libtest, which does not
understand these flags. To pass harness flags, target the bench explicitly:

```bash
cargo bench -p draw_bench_suite --bench pipeline -- --filter scene/update
```

Running `cargo bench -p draw_bench_suite` with no flags works as-is and runs
every benchmark.

### Baselines & regression gating

A baseline is plain text (diff-friendly, no serde):

```text
# draw_bench baseline v1
scene/update_clean/1000	2384.7
```

`--baseline` prints a comparison table and exits with code `1` when any
benchmark is slower than its baseline by more than `--threshold` percent. That is
the CI gate:

```bash
# record a baseline once (on a quiet, otherwise-idle machine)
cargo bench -p draw_bench_suite --bench pipeline -- --save-baseline benches/cpu.baseline.txt

# later / in CI: fail on a >5% regression
cargo bench -p draw_bench_suite --bench pipeline -- --baseline benches/cpu.baseline.txt
```

Benchmarks are machine- and load-sensitive. Pin the toolchain, close other work,
and prefer comparing on the same machine; only the direction and rough magnitude
are meaningful across machines.

---

## 2. CPU pipeline suite (`draw_bench_suite`)

`draw_bench_suite` owns the scenarios; `draw_bench` owns the measurement. Every
fixture is deterministic and every scenario runs at `100 / 1_000 / 10_000`
entities to expose scaling curves — the list scenarios are the exception, at
`1_000 / 10_000 / 100_000` rows (see below).

| Benchmark | Measures |
|---|---|
| `scene/update_clean/{n}` | `SceneTree::update` with nothing dirty (dirty-flag traversal) |
| `scene/update_dirty_all/{n}` | move every node, then `update` (propagation + recompute) |
| `scene/paint/{n}` | `SceneTree::paint` into a fresh `DrawList` |
| `ui/layout/{n}` | `draw_ui::layout` across the control tree |
| `ui/hit_test/{n}` | worst-case reverse hit test (point over the bottom-most control) |
| `ui/paint/{n}` | `draw_ui::paint` into a fresh `DrawList` |
| `pipeline/ui_frame/{n}` | end-to-end CPU frame: layout → paint → submit → end |
| `list/scroll_full/{n}` | one scrolling frame of a list with **every** row mounted (the naive shape) |
| `list/scroll_virtual/{n}` | the same frame through `draw_components::List` (the viewport's rows, recycled) |

```bash
cargo bench -p draw_bench_suite
cargo bench -p draw_bench_suite --bench pipeline -- --filter ui/
cargo bench -p draw_bench_suite --bench pipeline -- --filter list/
```

The suite submits through `draw_bench_suite::SinkBackend`, a consuming
`RenderBackend` that counts commands and retains nothing, so a long loop does not
grow memory the way `RecordingBackend` (which clones commands) would.

### The list comparison

The two list scenarios are one experiment run at `1_000 / 10_000 / 100_000`
rows: the same rows (two text cells, same padding and gap), the same columns, the
same viewport, the same scrolling step. The only difference is whether every row
is mounted or only the viewport's. Both shapes scroll through a *reflecting*
offset — a monotonic scroll parks at the bottom, and since the harness calls the
routine tens of thousands of times per benchmark, a one-way scroll would spend
most of its samples on a stopped list (and *when* it parks depends on the row
count, which would compare a moving list against a static one; a static frame
also skips its whole subtree through partial relayout, so it is cheap for reasons
unrelated to the shape).

After the timing table the bench target prints what a frame is billed in, at
every size (Apple Silicon laptop, `bench` profile):

```text
list frame shape (what one frame is billed in)
     rows   full.ctrl  full.cmds   virt.ctrl  virt.cmds   pool  visible     cmds
     1000       3.00K      2.00K       107.0       72.0     35       34    27.8x
    10000      30.00K     20.00K       107.0       72.0     35       34   277.8x
   100000     300.00K    200.00K       107.0       72.0     35       34  2777.8x

list scroll frame (median per frame)
     rows        full     virtual   speedup
     1000      1.75ms     71.19µs     24.5x
    10000     22.50ms     71.31µs    315.5x
   100000    264.86ms     71.58µs   3700.4x
```

Two things to read out of it:

- **The virtual frame is flat**: ~71.2 / 71.3 / 71.6 µs and 72 commands whether
  the folder holds a thousand entries or a hundred thousand. That is the whole
  point of the component — the cost follows the viewport (`ceil(800/24) + 1 = 35`
  rows), not the data.
- **The naive frame is O(rows)**: 1.75 ms → 22.50 ms → 264.86 ms, slightly worse
  than linear (a 10× row count costs ~12×, cache effects included). At 100 K rows
  it is 3.8 frames per second *before* touching a GPU, and the clip does not
  save it: a scissored command is still laid out and still submitted.

A frame that scrolls the virtual list but is otherwise idle costs 5.8 µs
(`pool_size` unchanged, no rows re-bound) — the ~71 µs is the price of re-binding
all 35 rows on a 2.5-row step. Re-keying the pool so that only the rows entering
and leaving are re-bound is the next ~12× if it ever matters;
`docs/plan.md` tracks it.


---

## 3. GPU benchmark (`draw_backend_wgpu`)

`crates/draw_backend_wgpu/benches/wgpu.rs` measures the offscreen path end to
end: CPU command execution, GPU submission, and a blocking `read_pixels` sync.
The readback makes each frame deterministic but dominates the number, so read it
as "cost of a readback-synchronized frame", not raw raster time.

```bash
cargo bench -p draw_backend_wgpu --bench wgpu
```

If no adapter is available (GPU-less CI, headless box) the benchmarks are skipped
and the process exits successfully.

---

## 4. What this does *not* do

- It does not replace the profiler: it reports a scalar per scenario, not a
  breakdown.
- It does not screenshot or measure visual output; correctness is still covered
  by the pixel/DrawList tests in `docs/testing.md`.
- It is not a micro-architecture profiler: for cache/branch effects use a
  sampling profiler on a benchmark binary.
