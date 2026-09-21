//! 菜单栏：除“编辑”外仍是占位入口。
//!
//! 点击一个菜单就在状态栏写一句话，说明它属于后面的哪个 Phase —— 不假装
//! 功能已经能用（遵守 `AGENTS.md` 的 placeholder policy）。撤销 / 重做已经
//! 实现（Phase 6），所以“编辑”指向真实的快捷键。

use std::cell::RefCell;
use std::rc::Rc;

use draw_components::{Button, Component, Flex};
use draw_core::Edges;
use draw_theme::{space, SurfaceLevel, TextSize, Theme};
use draw_ui::MouseFilter;

/// 菜单项，从左到右。
const MENUS: [&str; 8] = [
    "文件", "编辑", "图像", "图层", "选择", "滤镜", "视图", "帮助",
];

/// 一条带占位提示的菜单栏。
pub fn menu_bar(theme: Theme, message: Rc<RefCell<Option<String>>>) -> impl Component {
    let mut bar = Flex::row()
        .gap(space::XXS)
        .padding(Edges::symmetric(space::SM, space::XXS))
        .background(theme.surface(SurfaceLevel::Surface))
        .mouse_filter(MouseFilter::Ignore);
    for name in MENUS {
        let message = message.clone();
        bar = bar.child(
            Button::ghost(name, theme)
                .font_size(TextSize::Small.px())
                .on_click(move || {
                    *message.borrow_mut() = Some(menu_message(name));
                }),
        );
    }
    bar
}

/// 菜单项点击后的状态栏提示。“编辑”里的撤销 / 重做已经实现。
fn menu_message(name: &str) -> String {
    match name {
        "文件" => "文件：用右侧「文件」面板导入 / 导出 PNG（导入会成为新图层）".to_string(),
        "编辑" => {
            "编辑：撤销 Ctrl/Cmd+Z · 重做 Shift+Ctrl/Cmd+Z（其余编辑项属后续 Phase）".to_string()
        }
        _ => format!("菜单「{name}」：Phase 1 仅占位，功能随阶段实现"),
    }
}
