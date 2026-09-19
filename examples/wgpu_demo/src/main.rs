//! quill native `wgpu` demo.
//!
//! Opens a window and renders the shared, backend-neutral three-column
//! `demo_app::DemoApp` UI through [`draw_backend_wgpu::WgpuBackend`], presenting
//! it to a wgpu surface:
//!
//! ```text
//! winit events -> InputEvent -> Demo (Scene/UI) -> DrawList -> WgpuBackend -> surface
//! ```
//!
//! Run with:
//!
//! ```bash
//! cargo run -p wgpu_demo --release
//! ```
//!
//! Flags: `--debug-ui` and `--performance` show the component-bounds and
//! performance overlays (both off by default); `--profiler` controls stats
//! collection (default on); `--transparent-titlebar` (default on macOS; keeps
//! the traffic lights with a sidebar-only safe area), `--native-titlebar` and
//! `--hidden-titlebar` choose the window frame; `--help` lists everything.
//!
//! This is the only place that owns a window/event loop; the backend itself
//! stays window-agnostic and is also exercised headlessly in its own tests. The
//! demo is native-only; on `wasm32` the binary is intentionally empty.

#[cfg(not(target_arch = "wasm32"))]
mod app;
#[cfg(not(target_arch = "wasm32"))]
mod cli;
#[cfg(not(target_arch = "wasm32"))]
mod demo;

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match cli::parse(args) {
        Ok(cli::Command::Run(options)) => app::run(options),
        Ok(cli::Command::Help) => print!("{}", cli::HELP),
        Ok(cli::Command::Version) => println!("wgpu_demo {}", env!("CARGO_PKG_VERSION")),
        Err(message) => {
            eprintln!("error: {message}\n\n{}", cli::HELP);
            std::process::exit(2);
        }
    }
}

#[cfg(target_arch = "wasm32")]
fn main() {}
