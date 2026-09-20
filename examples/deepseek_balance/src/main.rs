//! 查询 DeepSeek 账户余额的小工具。
//!
//! 三个前端，同一份数据层（[`api`]）：
//!
//! - 默认（macOS）：**菜单栏应用** —— 状态栏显示余额，左键弹出面板，右键菜单。
//!   用本仓库自己的 UI 栈（`draw_theme` / `draw_components` / `draw_ui`
//!   + `draw_backend_wgpu`）渲染面板。
//! - `--window`：普通窗口，同样的 UI，方便调样式。
//! - `--cli`：只在终端打印文本，方便脚本调用。
//!
//! 用法：
//!
//! ```bash
//! cargo run --manifest-path examples/deepseek_balance/Cargo.toml            # 菜单栏
//! cargo run --manifest-path examples/deepseek_balance/Cargo.toml -- --window # 窗口
//! cargo run --manifest-path examples/deepseek_balance/Cargo.toml -- --cli   # 文本
//! ```
//!
//! 环境变量可覆盖默认值：`DEEPSEEK_API_KEY`、`DEEPSEEK_BALANCE_URL`。

mod api;
mod host;
#[cfg(target_os = "macos")]
mod menubar;
mod ui;

use host::Options;

/// `--help` 输出。
const HELP: &str = "\
deepseek_balance — 查询 DeepSeek 账户余额

用法:
  deepseek_balance [选项]

选项:
      --window         用普通窗口而不是菜单栏（非 macOS 上只能这样）
      --cli            只在终端打印结果，不打开窗口
      --light          使用浅色主题（默认深色）
      --pixel-font     使用内置点阵字体（默认系统字体；点阵字体不含中文）
      --every <秒>     自动刷新间隔，0 表示不自动刷新（菜单栏默认 60 秒）
      --frames <n>     渲染 n 帧后退出（用于自检真实渲染管线）
      --until-result   拿到第一次余额并写入界面后立即退出（自检端到端链路）
  -h, --help           显示本帮助

环境变量:
  DEEPSEEK_API_KEY       覆盖默认的临时 key
  DEEPSEEK_BALANCE_URL   覆盖默认的 https://api.deepseek.com/user/balance

用法（macOS 菜单栏）:
  左键状态栏项            打开 / 收起余额面板
  右键状态栏项            刷新余额 / 打开面板 / 退出
  面板内 R / F5           刷新余额
  面板内 Esc              收起面板

用法（--window）:
  R / F5                 刷新余额
  关闭按钮                退出
";

/// 解析后的命令行。
enum Command {
    /// 打开窗口。
    Run(Options),
    /// 打印文本结果。
    Cli,
    /// 打印帮助。
    Help,
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match parse(&args) {
        Ok(Command::Help) => print!("{HELP}"),
        Ok(Command::Cli) => run_cli(),
        Ok(Command::Run(options)) => host::run(options),
        Err(message) => {
            eprintln!("error: {message}\n\n{HELP}");
            std::process::exit(2);
        }
    }
}

/// Parses the arguments; unknown flags are an error rather than being ignored.
fn parse(args: &[String]) -> Result<Command, String> {
    let mut options = Options {
        light: false,
        pixel_font: false,
        frames: None,
        until_result: false,
        window: false,
        every: None,
    };
    let mut cli = false;

    let mut index = 0;
    while index < args.len() {
        let arg = args[index].as_str();
        index += 1;
        match arg {
            "--cli" => cli = true,
            "--window" => options.window = true,
            "--light" => options.light = true,
            "--pixel-font" => options.pixel_font = true,
            "--until-result" => options.until_result = true,
            "--every" => {
                let value = args
                    .get(index)
                    .ok_or_else(|| "--every 需要一个秒数".to_string())?;
                index += 1;
                let seconds: u64 = value
                    .parse()
                    .map_err(|_| format!("--every 需要非负整数，收到 `{value}`"))?;
                options.every = Some(seconds);
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
        Command::Cli
    } else {
        Command::Run(options)
    })
}

/// The terminal front end: one query, printed as plain text.
fn run_cli() {
    match api::fetch(&api::endpoint(), &api::api_key()) {
        Ok(balance) => {
            println!("DeepSeek 余额查询结果");
            println!("is_available: {}", balance.is_available);
            for info in &balance.balance_infos {
                println!("currency: {}", info.currency);
                println!("  total_balance:     {}", info.total_balance);
                println!("  granted_balance:   {}", info.granted_balance);
                println!("  topped_up_balance: {}", info.topped_up_balance);
            }
        }
        Err(message) => {
            eprintln!("{message}");
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|arg| arg.to_string()).collect()
    }

    #[test]
    fn defaults_to_the_menu_bar() {
        match parse(&args(&[])) {
            Ok(Command::Run(options)) => {
                assert!(!options.light);
                assert!(!options.pixel_font);
                assert_eq!(options.frames, None);
                assert!(!options.until_result);
                assert!(!options.window, "the menu bar is the default on macOS");
                assert_eq!(options.every, None);
            }
            _ => panic!("expected a window run"),
        }
    }

    #[test]
    fn cli_flag_selects_the_text_front_end() {
        assert!(matches!(parse(&args(&["--cli"])), Ok(Command::Cli)));
    }

    #[test]
    fn flags_are_collected() {
        match parse(&args(&[
            "--light",
            "--pixel-font",
            "--frames",
            "3",
            "--until-result",
            "--window",
            "--every",
            "30",
        ])) {
            Ok(Command::Run(options)) => {
                assert!(options.light);
                assert!(options.pixel_font);
                assert_eq!(options.frames, Some(3));
                assert!(options.until_result);
                assert!(options.window);
                assert_eq!(options.every, Some(30));
            }
            _ => panic!("expected a window run"),
        }
    }

    #[test]
    fn help_wins() {
        assert!(matches!(parse(&args(&["--help"])), Ok(Command::Help)));
    }

    #[test]
    fn bad_arguments_are_reported() {
        assert!(parse(&args(&["--nope"])).is_err());
        assert!(parse(&args(&["--frames"])).is_err());
        assert!(parse(&args(&["--frames", "zero"])).is_err());
        assert!(parse(&args(&["--frames", "0"])).is_err());
        assert!(parse(&args(&["--every"])).is_err());
        assert!(parse(&args(&["--every", "-1"])).is_err());
    }

    #[test]
    fn every_zero_is_accepted_as_off() {
        match parse(&args(&["--every", "0"])) {
            Ok(Command::Run(options)) => assert_eq!(options.every, Some(0)),
            _ => panic!("expected a window run"),
        }
    }
}
