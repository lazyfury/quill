//! Lucide 图标：加载 SVG 图标包，并把图标直接描边到界面节点上。
//!
//! 渲染走核心的 `draw_svg`（backend-neutral：把 SVG flatten 成 IR 的 `Line` /
//! `FillCircle`），所以图标**不需要**栅格化成纹理，也不引额外依赖。
//!
//! 默认加载仓库里 vendor 的 20 个图标（`assets/icons/`，含 Lucide 的 ISC
//! `LICENSE`），测试与 `--selfcheck` 因此可复现；把 `IMAGE_EDITOR_ICON_DIR`
//! 指向完整图标包（例如下载的 lucide-static `icons/`，2112 个）即可换成整包，
//! 而展示的仍是 [`ICON_NAMES`] 这一小组。

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use draw_core::{Color, NodeId, Rect, Size, Vec2};
use draw_render::PaintContext;
use draw_scene::SceneTree;
use draw_svg::{IconPack, SvgDocument};
use draw_ui::{add_decor, foreground_decor, inset, InteractState};

/// 仓库里 vendor 的图标目录（测试 / 自检用它，保证可复现）。
const VENDORED_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/icons");
/// 可选：指向一个完整的 Lucide `icons/` 目录。
const ENV_DIR: &str = "IMAGE_EDITOR_ICON_DIR";

/// 画廊里展示的图标（也是工具栏等界面会用到的名字）。
pub const ICON_NAMES: [&str; 20] = [
    "mouse-pointer-2",
    "brush",
    "eraser",
    "square-dashed",
    "pipette",
    "undo-2",
    "redo-2",
    "plus",
    "minus",
    "trash-2",
    "eye",
    "eye-off",
    "chevron-up",
    "chevron-down",
    "pencil",
    "save",
    "folder-open",
    "image",
    "zoom-in",
    "zoom-out",
];

/// 网格里一个图标的格子边长（逻辑像素）。
const CELL: f32 = 34.0;
/// 格子内图标四周的留白。
const CELL_INSET: f32 = 5.0;
/// 单个图标按钮里图标四周的留白。
const ICON_INSET: f32 = 5.0;

/// 已加载的图标包 + 解析缓存。
pub struct IconSet {
    pack: IconPack,
    cache: RefCell<HashMap<String, Rc<SvgDocument>>>,
}

impl IconSet {
    /// 加载图标包：`IMAGE_EDITOR_ICON_DIR` 优先，否则用 vendor 的目录。
    ///
    /// 目录打不开时回退到 vendor 目录；vendor 目录是仓库自带的，所以这里
    /// `expect` 只会在仓库损坏（资产被删）时触发。
    pub fn load() -> Self {
        let dir = std::env::var(ENV_DIR).unwrap_or_else(|_| VENDORED_DIR.to_string());
        let pack = IconPack::open(&dir)
            .or_else(|_| IconPack::open(VENDORED_DIR))
            .expect("vendored icon pack must exist");
        Self {
            pack,
            cache: RefCell::new(HashMap::new()),
        }
    }

    /// 包里索引到的图标数量（vendor 子集为 20，完整包为 2000+）。
    pub fn len(&self) -> usize {
        self.pack.len()
    }

    /// 解析（并缓存）一个图标；名字不存在或解析失败返回 `None`。
    pub fn document(&self, name: &str) -> Option<Rc<SvgDocument>> {
        if let Some(document) = self.cache.borrow().get(name) {
            return Some(document.clone());
        }
        let document = Rc::new(self.pack.load(name)?.ok()?);
        self.cache
            .borrow_mut()
            .insert(name.to_string(), document.clone());
        Some(document)
    }

    /// 在 `node` 的矩形中央画一个图标（`node` 必须已在树里）。
    pub fn attach_icon(&self, tree: &mut SceneTree, node: NodeId, name: &str, color: Color) {
        let Some(document) = self.document(name) else {
            return;
        };
        let decor = foreground_decor(
            move |ctx: &mut PaintContext, rect: Rect, _state: InteractState| {
                let size = rect.size.width.min(rect.size.height);
                let icon = Rect::from_center_size(rect.center(), Size::splat(size));
                document.draw(ctx, inset(icon, ICON_INSET), color);
            },
        );
        add_decor(tree, node, decor);
    }

    /// 在 `node` 的矩形里按网格画一批图标（`node` 必须已在树里）。
    ///
    /// 解析好的 [`SvgDocument`] 被 decorator 持有，所以每帧只是把矢量重新
    /// 描边成命令，不会重复读文件 / 解析。
    pub fn attach_grid(
        &self,
        tree: &mut SceneTree,
        node: NodeId,
        names: &[&'static str],
        color: Color,
    ) {
        let documents: Vec<Rc<SvgDocument>> = names
            .iter()
            .filter_map(|name| self.document(name))
            .collect();
        if documents.is_empty() {
            return;
        }
        let decor = foreground_decor(
            move |ctx: &mut PaintContext, rect: Rect, _state: InteractState| {
                let columns = ((rect.size.width / CELL).floor() as usize).max(1);
                for (index, document) in documents.iter().enumerate() {
                    let column = index % columns;
                    let row = index / columns;
                    let cell = Rect::from_min_size(
                        rect.origin + Vec2::new(column as f32 * CELL, row as f32 * CELL),
                        Size::splat(CELL),
                    );
                    document.draw(ctx, inset(cell, CELL_INSET), color);
                }
            },
        );
        add_decor(tree, node, decor);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use draw_render::PaintContext;

    #[test]
    fn the_vendored_pack_indexes_every_listed_icon() {
        let icons = IconSet::load();
        assert!(
            icons.len() >= ICON_NAMES.len(),
            "expected at least the vendored subset, got {}",
            icons.len()
        );
        for name in ICON_NAMES {
            assert!(icons.document(name).is_some(), "missing icon `{name}`");
        }
    }

    #[test]
    fn an_icon_draws_into_a_draw_list() {
        let icons = IconSet::load();
        let document = icons.document("brush").expect("brush icon");
        let mut ctx = PaintContext::new();
        document.draw(
            &mut ctx,
            Rect::from_min_size(Vec2::ZERO, Size::splat(24.0)),
            Color::BLACK,
        );
        assert!(!ctx.into_draw_list().is_empty(), "icon drew no commands");
    }

    #[test]
    fn an_unknown_icon_is_none() {
        let icons = IconSet::load();
        assert!(icons.document("definitely-not-an-icon").is_none());
    }
}
