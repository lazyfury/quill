//! 菜单栏：一排菜单标题 + 真实的下拉菜单。
//!
//! 标题点击只写一个共享请求格（[`Rc<Cell<Option<usize>>>`]），真正的下拉由
//! [`EditorView`](crate::ui::EditorView) 在 `update` 里用
//! [`Overlays::menu`](draw_components::Overlays::menu) 打开 —— 回调拿不到
//! `&mut self`，所以走请求格。菜单项再写 [`MenuAction`] 请求格，由 `update`
//! 统一执行（这也是关闭菜单的时机）。
//!
//! 已实现的动作接真实命令（撤销 / 重做 / 导入导出 / 缩放 / 取消选区 / 关于），
//! 其余保留带标注的占位项，遵守仓库的 placeholder policy。

use std::cell::Cell;
use std::rc::Rc;

use draw_components::{Button, Component, Flex, Menu, MenuItem, NodeRef};
use draw_core::Edges;
use draw_scene::SceneTree;
use draw_theme::{space, SurfaceLevel, TextSize, Theme};
use draw_ui::MouseFilter;

/// 菜单栏标题，从左到右。
pub const MENUS: [&str; 8] = [
    "文件", "编辑", "图像", "图层", "选择", "滤镜", "视图", "帮助",
];

/// 下拉菜单里一个可执行的动作。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuAction {
    Undo,
    Redo,
    Import,
    Export,
    ZoomIn,
    ZoomOut,
    ZoomReset,
    ZoomFit,
    ClearSelection,
    /// 把当前图层裁到文档大小（丢掉画布外像素）。
    CropLayerToDocument,
    /// 切换透明棋盘格背景。
    ToggleCheckerboard,
    About,
    /// 尚未实现的菜单项：只把提示写进状态栏。
    Placeholder(&'static str),
}

/// 一条带占位提示的菜单栏。`request` 收到被点标题的下标。
pub fn menu_bar(
    theme: Theme,
    request: Rc<Cell<Option<usize>>>,
    refs: &mut Vec<NodeRef>,
) -> impl Component {
    let mut bar = Flex::row()
        .gap(space::XXS)
        .padding(Edges::symmetric(space::SM, space::XXS))
        .background(theme.surface(SurfaceLevel::Surface))
        .mouse_filter(MouseFilter::Ignore);
    for (index, name) in MENUS.iter().enumerate() {
        let slot = NodeRef::new();
        refs.push(slot.clone());
        let request = request.clone();
        bar = bar.child(
            Button::ghost(*name, theme)
                .font_size(TextSize::Small.px())
                .on_click(move || request.set(Some(index)))
                .ref_(&slot),
        );
    }
    bar
}

/// 构建第 `index` 个菜单的内容，挂到 `root`（覆盖层给的定位节点）下。
pub fn menu_content(
    tree: &mut SceneTree,
    root: draw_core::NodeId,
    theme: Theme,
    index: usize,
    action: Rc<Cell<Option<MenuAction>>>,
    can_undo: bool,
    can_redo: bool,
    has_selection: bool,
) {
    let mut menu = Menu::new(theme);
    match index {
        0 => {
            menu = menu
                .item(placeholder("新建…", theme, &action, "新建：后续 Phase"))
                .item(placeholder(
                    "打开…",
                    theme,
                    &action,
                    "打开：Phase 10 文件浏览器",
                ))
                .separator()
                .item(menu_item(
                    "导入 PNG…",
                    None,
                    theme,
                    &action,
                    MenuAction::Import,
                ))
                .item(menu_item(
                    "导出 PNG…",
                    None,
                    theme,
                    &action,
                    MenuAction::Export,
                ));
        }
        1 => {
            menu = menu
                .item(
                    menu_item("撤销", Some("Ctrl+Z"), theme, &action, MenuAction::Undo)
                        .disabled(!can_undo),
                )
                .item(
                    menu_item(
                        "重做",
                        Some("Shift+Ctrl+Z"),
                        theme,
                        &action,
                        MenuAction::Redo,
                    )
                    .disabled(!can_redo),
                )
                .separator()
                .item(placeholder("剪切", theme, &action, "剪切：后续 Phase"))
                .item(placeholder("复制", theme, &action, "复制：后续 Phase"))
                .item(placeholder("粘贴", theme, &action, "粘贴：后续 Phase"));
        }
        2 => {
            menu = menu
                .item(placeholder(
                    "图像大小…",
                    theme,
                    &action,
                    "图像大小：后续 Phase",
                ))
                .item(placeholder("裁剪", theme, &action, "裁剪：后续 Phase"))
                .separator()
                .item(placeholder(
                    "水平翻转",
                    theme,
                    &action,
                    "水平翻转：后续 Phase",
                ))
                .item(placeholder(
                    "垂直翻转",
                    theme,
                    &action,
                    "垂直翻转：后续 Phase",
                ))
                .item(placeholder("旋转 90°", theme, &action, "旋转：后续 Phase"));
        }
        3 => {
            menu = menu
                .item(placeholder(
                    "新建图层",
                    theme,
                    &action,
                    "新建图层：用右侧图层面板",
                ))
                .item(placeholder(
                    "复制图层",
                    theme,
                    &action,
                    "复制图层：后续 Phase",
                ))
                .separator()
                .item(placeholder(
                    "上移一层",
                    theme,
                    &action,
                    "上移一层：用右侧图层面板",
                ))
                .item(placeholder(
                    "下移一层",
                    theme,
                    &action,
                    "下移一层：用右侧图层面板",
                ))
                .separator()
                .item(menu_item(
                    "裁到文档",
                    None,
                    theme,
                    &action,
                    MenuAction::CropLayerToDocument,
                ));
        }
        4 => {
            menu = menu
                .item(placeholder("全选", theme, &action, "全选：后续 Phase"))
                .item(placeholder("反选", theme, &action, "反选：后续 Phase"))
                .item(
                    menu_item(
                        "取消选择",
                        Some("Esc"),
                        theme,
                        &action,
                        MenuAction::ClearSelection,
                    )
                    .disabled(!has_selection),
                );
        }
        5 => {
            menu = menu
                .item(placeholder("灰度", theme, &action, "灰度：后续 Phase"))
                .item(placeholder("反相", theme, &action, "反相：后续 Phase"))
                .item(placeholder("高斯模糊", theme, &action, "模糊：后续 Phase"));
        }
        6 => {
            menu = menu
                .item(menu_item(
                    "放大",
                    Some("+"),
                    theme,
                    &action,
                    MenuAction::ZoomIn,
                ))
                .item(menu_item(
                    "缩小",
                    Some("-"),
                    theme,
                    &action,
                    MenuAction::ZoomOut,
                ))
                .item(menu_item(
                    "100%",
                    Some("0"),
                    theme,
                    &action,
                    MenuAction::ZoomReset,
                ))
                .item(menu_item(
                    "适配窗口",
                    Some("F"),
                    theme,
                    &action,
                    MenuAction::ZoomFit,
                ))
                .separator()
                .item(menu_item(
                    "显示棋盘格",
                    None,
                    theme,
                    &action,
                    MenuAction::ToggleCheckerboard,
                ));
        }
        _ => {
            menu = menu
                .item(menu_item(
                    "关于 image_editor",
                    None,
                    theme,
                    &action,
                    MenuAction::About,
                ))
                .separator()
                .item(placeholder(
                    "快捷键…",
                    theme,
                    &action,
                    "快捷键：见窗口标题栏 / README",
                ));
        }
    }
    menu.build(tree, root);
}

fn menu_item(
    label: &str,
    shortcut: Option<&str>,
    theme: Theme,
    action: &Rc<Cell<Option<MenuAction>>>,
    value: MenuAction,
) -> MenuItem {
    let mut item = MenuItem::new(label, theme);
    if let Some(shortcut) = shortcut {
        item = item.shortcut(shortcut);
    }
    let action = action.clone();
    item.on_click(move || action.set(Some(value)))
}

fn placeholder(
    label: &str,
    theme: Theme,
    action: &Rc<Cell<Option<MenuAction>>>,
    note: &'static str,
) -> MenuItem {
    menu_item(label, None, theme, action, MenuAction::Placeholder(note))
}

/// 关于对话框 / 状态栏用的版本串。
pub fn about_text() -> String {
    format!("image_editor · 用 quill UI 栈绘制 · {} 个菜单", MENUS.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_menu_bar_has_eight_distinct_titles() {
        assert_eq!(MENUS.len(), 8);
        let mut titles = MENUS.to_vec();
        titles.sort_unstable();
        titles.dedup();
        assert_eq!(titles.len(), MENUS.len());
    }

    #[test]
    fn about_text_mentions_the_menu_count() {
        assert!(about_text().contains("8"));
    }
}
