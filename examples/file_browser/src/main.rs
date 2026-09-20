//! 用本仓库自己的 UI 栈写的一个文件浏览器。
//!
//! 三个前端，同一份数据层（[`scan`]）和同一个视图（[`ui::Browser`]）：
//!
//! - 默认：**窗口**。左边列表展示目录内容，右边是可拖拽的二进制预览栏 ——
//!   选中一个文件就把它的前 64 KiB 画成 hex dump。滚轮滚动，方向键选择，
//!   Enter 进目录，Backspace 回上级。读目录和读文件都在工作线程上，界面
//!   不等磁盘。
//! - `--cli`：只在终端打印文本清单，方便脚本调用。
//! - `--selfcheck` / `--dump`：无头自检 —— 同一套 UI 画进 `RecordingBackend`，
//!   用 `draw_profile` 体检 + 断言关键内容，不开窗、不截图。
//!
//! 用法：
//!
//! ```bash
//! cargo run --manifest-path examples/file_browser/Cargo.toml
//! cargo run --manifest-path examples/file_browser/Cargo.toml -- --path /usr --all
//! cargo run --manifest-path examples/file_browser/Cargo.toml -- --cli
//! cargo run --manifest-path examples/file_browser/Cargo.toml -- --selfcheck
//! cargo run --manifest-path examples/file_browser/Cargo.toml -- --frames 120
//! ```
//!
//! 这个 demo 也是 `draw_components::List` 的第一份真实用例：目录里有多少
//! 条目、选中的文件有多大，都不影响每帧成本（见 `docs/benchmarking.md`）。
//! 两个列表 —— 目录行和 hex 行 —— 都只挂视口那几行。

mod host;
mod preview;
mod scan;
mod selfcheck;
mod ui;

use std::path::PathBuf;

use host::Options;

const HELP: &str = "\
file_browser — 用 quill 自己画的目录浏览器

用法:
  file_browser [选项]

选项:
      --path <目录>    起始目录（默认当前目录）
      --all            显示点开头的文件（默认隐藏）
      --cli            只在终端打印文本清单，不开窗
      --selfcheck      无头自检：同一套 UI 画进 RecordingBackend，
                       用 draw_profile 体检 + 断言关键内容，失败退出码 1
      --dump           同 --selfcheck，并打印完整绘制命令
      --frames <n>     渲染 n 帧后退出（验证真实渲染管线）
      --light          使用浅色主题（默认深色）
      --pixel-font     使用内置点阵字体（默认系统字体；点阵字体不含中文）
  -h, --help           显示本帮助

用法（窗口）:
  ↑↓                   选择一行
  Enter                打开目录（也可以直接点目录）
  Backspace            回到上级
  R / F5               重扫当前目录
  H                    显示 / 隐藏点开头的文件
  滚轮                 滚动列表
  拖动分隔条           调整右栏（二进制预览）宽度
";

/// 解析后的命令行。
enum Command {
    Run(Options),
    /// 打印文本清单。
    Cli {
        path: PathBuf,
        hidden: bool,
    },
    /// 无头自检。`dump` 额外打印所有绘制命令。
    SelfCheck {
        dump: bool,
    },
    Help,
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match parse(&args) {
        Ok(Command::Help) => print!("{HELP}"),
        Ok(Command::Cli { path, hidden }) => run_cli(&path, hidden),
        Ok(Command::SelfCheck { dump }) => std::process::exit(selfcheck::run(dump)),
        Ok(Command::Run(options)) => host::run(options),
        Err(message) => {
            eprintln!("error: {message}\n\n{HELP}");
            std::process::exit(2);
        }
    }
}

fn parse(args: &[String]) -> Result<Command, String> {
    let cwd = std::env::current_dir().map_err(|_| "读不到当前目录".to_string())?;
    let mut options = Options {
        path: cwd.clone(),
        hidden: false,
        frames: None,
        light: false,
        pixel_font: false,
    };
    let mut cli = false;

    let mut index = 0;
    while index < args.len() {
        let arg = args[index].as_str();
        index += 1;
        match arg {
            "--cli" => cli = true,
            "--selfcheck" => return Ok(Command::SelfCheck { dump: false }),
            "--dump" => return Ok(Command::SelfCheck { dump: true }),
            "--all" => options.hidden = true,
            "--light" => options.light = true,
            "--pixel-font" => options.pixel_font = true,
            "--path" => {
                let value = args
                    .get(index)
                    .ok_or_else(|| "--path 需要一个目录".to_string())?;
                index += 1;
                let path = PathBuf::from(value);
                if !path.is_dir() {
                    return Err(format!("不是目录：`{}`", path.display()));
                }
                options.path = path;
            }
            "--frames" => {
                let value = args
                    .get(index)
                    .ok_or_else(|| "--frames 需要一个帧数".to_string())?;
                index += 1;
                let frames: u32 = value
                    .parse()
                    .map_err(|_| format!("--frames 需要数字，收到 `{value}`"))?;
                if frames == 0 {
                    return Err("--frames 至少为 1".to_string());
                }
                options.frames = Some(frames);
            }
            "-h" | "--help" => return Ok(Command::Help),
            other => return Err(format!("未知参数 `{other}`")),
        }
    }

    Ok(if cli {
        Command::Cli {
            path: options.path,
            hidden: options.hidden,
        }
    } else {
        Command::Run(options)
    })
}

/// 终端前端：扫一次目录，纯文本打出来。
fn run_cli(path: &std::path::Path, hidden: bool) {
    let listing = scan::scan(path, hidden);
    if let Some(error) = listing.error {
        eprintln!("{error}");
        std::process::exit(1);
    }
    println!(
        "{} ({} 项)",
        scan::display_path(&listing.path),
        listing.len()
    );
    for entry in &listing.entries {
        println!(
            "  {:<40} {:>9}  {}",
            scan::display_name(entry),
            scan::format_size(entry),
            scan::format_time(entry),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|arg| arg.to_string()).collect()
    }

    #[test]
    fn no_arguments_opens_a_window_on_the_current_directory() {
        let cwd = std::env::current_dir().unwrap();
        match parse(&args(&[])) {
            Ok(Command::Run(options)) => {
                assert_eq!(options.path, cwd);
                assert!(!options.hidden, "点文件默认隐藏");
                assert_eq!(options.frames, None);
                assert!(!options.light);
                assert!(!options.pixel_font);
            }
            _ => panic!("expected a window run"),
        }
    }

    #[test]
    fn cli_flag_selects_the_text_front_end() {
        match parse(&args(&["--cli"])) {
            Ok(Command::Cli { hidden, .. }) => assert!(!hidden),
            _ => panic!("expected the cli"),
        }
    }

    #[test]
    fn selfcheck_and_dump_are_headless() {
        assert!(matches!(
            parse(&args(&["--selfcheck"])),
            Ok(Command::SelfCheck { dump: false })
        ));
        assert!(matches!(
            parse(&args(&["--dump"])),
            Ok(Command::SelfCheck { dump: true })
        ));
    }

    #[test]
    fn flags_are_collected() {
        match parse(&args(&[
            "--path",
            "/tmp",
            "--all",
            "--frames",
            "3",
            "--light",
            "--pixel-font",
        ])) {
            Ok(Command::Run(options)) => {
                assert_eq!(options.path, PathBuf::from("/tmp"));
                assert!(options.hidden);
                assert_eq!(options.frames, Some(3));
                assert!(options.light);
                assert!(options.pixel_font);
            }
            _ => panic!("expected a window run"),
        }
    }

    #[test]
    fn a_path_that_is_not_a_directory_is_rejected() {
        assert!(parse(&args(&["--path", "/definitely/not/here"])).is_err());
    }

    #[test]
    fn bad_arguments_are_reported() {
        assert!(parse(&args(&["--nope"])).is_err());
        assert!(parse(&args(&["--path"])).is_err(), "--path 缺参数");
        assert!(parse(&args(&["--frames"])).is_err());
        assert!(parse(&args(&["--frames", "zero"])).is_err());
        assert!(parse(&args(&["--frames", "0"])).is_err());
    }

    #[test]
    fn help_wins() {
        assert!(matches!(parse(&args(&["--help"])), Ok(Command::Help)));
    }
}
