//! `game_demo` entry point: a window host, or `--selfcheck` for a headless run.

mod app;

use game_demo::run_selfcheck;

fn main() {
    if std::env::args().any(|arg| arg == "--selfcheck") {
        if let Err(error) = run_selfcheck() {
            eprintln!("game_demo selfcheck failed: {error}");
            std::process::exit(1);
        }
        return;
    }
    app::run();
}
