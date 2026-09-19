#[cfg(target_os = "macos")]
mod app;

fn main() {
    #[cfg(target_os = "macos")]
    {
        if let Err(error) = app::main() {
            eprintln!("macos_demo: {error}");
            std::process::exit(1);
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        eprintln!("macos_demo is macOS-only");
    }
}
