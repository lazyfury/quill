//! quill 图像编辑器 —— 用本仓库自己的 UI 栈代替 `egui` 的桌面示例。
//!
//! 启动先看到**主页**（[`ui::HomeView`]）：「新建窗口」开一个**原生**的
//! 「新建文档」窗口（[`ui::NewDocumentView`]）选尺寸 / 背景色，「画廊」是占位；
//! 创建后编辑器仍在**主窗口**里。多窗口由 [`app::application`] 管理。
//!
//! 编辑器本身到 Phase 8（移动 / 框选 / 吸管）：菜单/工具栏 + 真实文档 /
//! 图层 + 画笔 / 橡皮 + 撤销 / 重做 + PNG 导入 / 导出 + 完整工具集。
//!
//! ```bash
//! cargo run --manifest-path examples/image_editor/Cargo.toml
//! cargo run --manifest-path examples/image_editor/Cargo.toml -- --selfcheck
//! ```

mod app;
mod canvas;
mod document;
mod icons;
mod io;
mod renderer;
mod selfcheck;
mod theme;
mod tools;
mod ui;

use app::application::{self, Options};

const HELP: &str = "\
image_editor — 用 quill 自己画的图像编辑器（Phase 8：移动 / 框选 / 吸管）

启动先显示主页；点「新建窗口」开一个**原生**的「新建文档」窗口（不弹模态框）
选尺寸 / 背景色，创建后在**主窗口**里编辑。「画廊」是占位。

用法:
  image_editor [选项]

选项:
      --selfcheck   无头自检：录 DrawList + draw_profile 体检 + 断言关键内容，
                    失败退出码 1
      --dump        同 --selfcheck，并打印完整绘制命令
      --frames <n>  渲染 n 帧后退出（验证真实渲染管线）
      --light       使用浅色主题（默认深色）
      --pixel-font  使用内置点阵字体（默认系统字体；点阵字体不含中文）
  -h, --help        显示本帮助

快捷键:
  V 移动 · B 画笔 · E 橡皮 · M 矩形选择 · I 吸管
  Ctrl/Cmd+Z 撤销 · Shift+Ctrl/Cmd+Z（或 Ctrl+Y）重做
  [ ] 笔刷大小 · , . 笔刷不透明度
  + - 缩放 · 0 100% · F 适配

导入导出（Phase 7）:
  右侧「文件」面板改路径，再点「导入 PNG」/「导出 PNG」
  （winit 无原生文件对话框，所以用「改路径」内联输入）

工具（Phase 8）:
  V 移动当前图层（画布上拖动）
  M 框选（拖动出选区，画笔只在选区内落笔；Esc 清空）
  I 吸管（点画布取色为前景色）
";

/// 解析后的命令行。
enum Command {
    Run(Options),
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
        Ok(Command::SelfCheck { dump }) => std::process::exit(selfcheck::run(dump)),
        Ok(Command::Run(options)) => application::run(options),
        Err(message) => {
            eprintln!("error: {message}\n\n{HELP}");
            std::process::exit(2);
        }
    }
}

fn parse(args: &[String]) -> Result<Command, String> {
    let mut options = Options::default();
    let mut index = 0;
    while index < args.len() {
        let arg = args[index].as_str();
        index += 1;
        match arg {
            "--selfcheck" => return Ok(Command::SelfCheck { dump: false }),
            "--dump" => return Ok(Command::SelfCheck { dump: true }),
            "--light" => options.light = true,
            "--pixel-font" => options.pixel_font = true,
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
    Ok(Command::Run(options))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|arg| arg.to_string()).collect()
    }

    #[test]
    fn no_arguments_opens_a_window_with_defaults() {
        match parse(&args(&[])) {
            Ok(Command::Run(options)) => {
                assert_eq!(options.frames, None);
                assert!(!options.light);
                assert!(!options.pixel_font);
            }
            _ => panic!("expected a window run"),
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
        match parse(&args(&["--frames", "3", "--light", "--pixel-font"])) {
            Ok(Command::Run(options)) => {
                assert_eq!(options.frames, Some(3));
                assert!(options.light);
                assert!(options.pixel_font);
            }
            _ => panic!("expected a window run"),
        }
    }

    #[test]
    fn bad_arguments_are_reported() {
        assert!(parse(&args(&["--nope"])).is_err());
        assert!(parse(&args(&["--frames"])).is_err());
        assert!(parse(&args(&["--frames", "zero"])).is_err());
        assert!(parse(&args(&["--frames", "0"])).is_err());
    }

    #[test]
    fn help_wins() {
        assert!(matches!(parse(&args(&["--help"])), Ok(Command::Help)));
    }
}
