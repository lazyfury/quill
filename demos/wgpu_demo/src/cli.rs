//! Command-line options for the native `wgpu` demo (native only).
//!
//! Parsed by hand: the demo has no CLI dependency and only a handful of flags.
//! The result is a pure value ([`Options`]) so parsing is unit-testable without
//! opening a window.

/// Runtime options derived from the command line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    /// Component debug drawing (yellow bounds + `name#id`) at startup.
    pub debug_ui: bool,
    /// Performance panel at startup.
    pub performance: bool,
    /// Collect frame stats in the profiler.
    pub profiler: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            debug_ui: false,
            performance: false,
            profiler: true,
        }
    }
}

/// What the command line asked the demo to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    /// Run the demo with these options.
    Run(Options),
    /// Print [`HELP`] and exit.
    Help,
    /// Print the version and exit.
    Version,
}

/// Help text, printed by `--help` and after argument errors.
pub const HELP: &str = "\
wgpu_demo — quill native wgpu demo

USAGE:
    wgpu_demo [OPTIONS]

OPTIONS:
        --debug-ui        Draw component bounds (yellow name#id)
        --no-debug-ui     Start without component debug drawing (default)
        --performance     Show the performance panel
        --no-performance  Start without the performance panel (default)
        --profiler        Collect frame stats (default)
        --no-profiler     Disable the profiler (the panel shows placeholders)
    -h, --help            Print this help
    -V, --version         Print the version

SHORTCUTS:
    F3 / ` / d            Toggle component debug bounds (yellow name#id boxes)
    F4 / p                Toggle the performance panel
    F5 / o                Toggle the profiler
";

/// Parses arguments (the program name must already be stripped).
///
/// Returns [`Command::Help`] / [`Command::Version`] for those flags, or an error
/// string for unrecognized arguments.
pub fn parse<I>(args: I) -> Result<Command, String>
where
    I: IntoIterator<Item = String>,
{
    let mut options = Options::default();
    for arg in args {
        match arg.as_str() {
            "--debug-ui" | "--debug" => options.debug_ui = true,
            "--no-debug-ui" | "--no-debug" => options.debug_ui = false,
            "--performance" | "--perf" => options.performance = true,
            "--no-performance" | "--no-perf" => options.performance = false,
            "--profiler" | "--profile" => options.profiler = true,
            "--no-profiler" | "--no-profile" => options.profiler = false,
            "-h" | "--help" => return Ok(Command::Help),
            "-V" | "--version" => return Ok(Command::Version),
            other => return Err(format!("unrecognized argument '{other}'")),
        }
    }
    Ok(Command::Run(options))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_strs(args: &[&str]) -> Result<Command, String> {
        parse(args.iter().map(|arg| (*arg).to_string()))
    }

    fn options(args: &[&str]) -> Options {
        match parse_strs(args) {
            Ok(Command::Run(options)) => options,
            other => panic!("expected Run, got {other:?}"),
        }
    }

    #[test]
    fn defaults_are_all_off_except_the_profiler() {
        assert_eq!(
            options(&[]),
            Options {
                debug_ui: false,
                performance: false,
                profiler: true,
            }
        );
    }

    #[test]
    fn flags_toggle_individual_overlays() {
        assert!(!options(&["--no-debug-ui"]).debug_ui);
        assert!(options(&["--performance"]).performance);
        assert!(!options(&["--no-profiler"]).profiler);
        assert!(options(&["--no-debug-ui", "--debug-ui"]).debug_ui);
        assert!(!options(&["--performance", "--no-performance"]).performance);
    }

    #[test]
    fn last_flag_wins() {
        assert!(!options(&["--debug-ui", "--no-debug-ui"]).debug_ui);
        assert!(options(&["--no-performance", "--performance"]).performance);
        assert!(!options(&["--profiler", "--no-profiler"]).profiler);
    }

    #[test]
    fn aliases_are_accepted() {
        assert!(!options(&["--no-debug"]).debug_ui);
        assert!(options(&["--perf"]).performance);
        assert!(!options(&["--no-profile"]).profiler);
    }

    #[test]
    fn help_and_version() {
        assert_eq!(parse_strs(&["-h"]), Ok(Command::Help));
        assert_eq!(parse_strs(&["--help"]), Ok(Command::Help));
        assert_eq!(parse_strs(&["-V"]), Ok(Command::Version));
        assert_eq!(parse_strs(&["--version"]), Ok(Command::Version));
        // help/version short-circuit even with other flags present
        assert_eq!(parse_strs(&["--no-profiler", "--help"]), Ok(Command::Help));
    }

    #[test]
    fn unknown_and_positional_arguments_are_errors() {
        assert!(parse_strs(&["--wat"]).is_err());
        assert!(parse_strs(&["extra"]).is_err());
    }
}
