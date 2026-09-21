//! Lucide 图标：加载 SVG 图标包，并把图标作为 [`Icon`] **组件**描边到界面里。
//!
//! 渲染走核心的 `draw_svg`（backend-neutral：把 SVG flatten 成 IR 的 `Line` /
//! `FillCircle`），所以图标**不需要**栅格化成纹理，也不引额外依赖。
//!
//! [`Icon`] 是一个普通 `Component`，所以在构建按钮时就能 `.child(Icon::new(..))`
//! 一起搭进去（见 `ui/toolbar.rs`），不用等树建好再回头挂装饰器。
//!
//! 默认加载仓库里 vendor 的图标（`assets/icons/`，含 Lucide 的 ISC
//! `LICENSE`）——就是工具栏的工具与撤销 / 重做要用的那几个，测试与
//! `--selfcheck` 因此可复现；把 `IMAGE_EDITOR_ICON_DIR` 指向完整图标包
//! （例如下载的 lucide-static `icons/`，2112 个）即可换成整包。

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use draw_components::{Component, Spec};
use draw_core::{Color, Rect, Size};
use draw_render::PaintContext;
use draw_svg::{IconPack, SvgDocument};
use draw_ui::{InteractState, MouseFilter, Widget};

/// 仓库里 vendor 的图标目录（测试 / 自检用它，保证可复现）。
const VENDORED_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/icons");
/// 可选：指向一个完整的 Lucide `icons/` 目录。
const ENV_DIR: &str = "IMAGE_EDITOR_ICON_DIR";
/// 工具栏图标的逻辑尺寸。显式固定，不随按钮高度缩小 —— 紧凑主题把按钮变矮
/// 时，图标不会跟着缩水。
pub const TOOLBAR_ICON: f32 = 14.0;

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
}

/// 一个图标组件：占 `size × size`，把对应 SVG 居中描边。
///
/// 尺寸显式给出，不从父节点矩形推算 —— 紧凑主题把按钮变矮时图标不会缩水。
/// `mouse_filter` 是 `Ignore`，点击会落到外层按钮上。
pub struct Icon {
    spec: Spec,
    icons: Rc<IconSet>,
    name: String,
    color: Color,
    size: f32,
}

impl Icon {
    /// 在 `icons` 里按 `name` 取图标，用 `color` 画成 `size` 大小。
    pub fn new(icons: Rc<IconSet>, name: impl Into<String>, color: Color, size: f32) -> Self {
        Self {
            spec: Spec::leaf(),
            icons,
            name: name.into(),
            color,
            size,
        }
    }
}

impl Component for Icon {
    fn spec(&mut self) -> &mut Spec {
        &mut self.spec
    }

    fn name(&self) -> &'static str {
        "Icon"
    }

    fn widget(&self) -> Widget {
        Widget::Panel {
            color: Color::TRANSPARENT,
            border: None,
        }
    }

    fn prepare(&mut self) {
        self.spec.data.min_size = Size::splat(self.size);
        self.spec.data.mouse_filter = MouseFilter::Ignore;

        let Some(document) = self.icons.document(&self.name) else {
            return;
        };
        let color = self.color;
        let size = self.size;
        self.spec.foreground = Some(Box::new(
            move |ctx: &mut PaintContext, rect: Rect, _state: InteractState| {
                let icon = Rect::from_center_size(rect.center(), Size::splat(size));
                document.draw(ctx, icon, color);
            },
        ));
    }
}

draw_components::impl_scene_child!(Icon);

#[cfg(test)]
mod tests {
    use super::*;
    use draw_components::Flex;
    use draw_core::{Edges, Vec2, ViewportSize};
    use draw_render::DrawCommand;
    use draw_scene::SceneTree;

    #[test]
    fn the_vendored_pack_indexes_every_ui_icon() {
        let icons = IconSet::load();
        let names: Vec<&str> = crate::app::state::ActiveTool::ALL
            .iter()
            .map(|tool| tool.icon())
            .chain(
                crate::app::state::HistoryAction::ALL
                    .iter()
                    .map(|a| a.icon()),
            )
            .collect();
        assert!(icons.len() >= names.len(), "indexed {}", icons.len());
        for name in names {
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
            Rect::from_min_size(draw_core::Vec2::ZERO, Size::splat(24.0)),
            Color::BLACK,
        );
        assert!(!ctx.into_draw_list().is_empty(), "icon drew no commands");
    }

    #[test]
    fn an_unknown_icon_is_none() {
        let icons = IconSet::load();
        assert!(icons.document("definitely-not-an-icon").is_none());
    }

    /// Extent of the icon's stroked lines inside a `container_height`-tall row.
    fn icon_extent(container_height: f32) -> Vec2 {
        let icons = Rc::new(IconSet::load());
        let mut tree = SceneTree::new();
        let root = tree.root();
        let page = tree.add_child(
            root,
            Flex::column()
                .gap(0.0)
                .padding(Edges::ZERO)
                .mouse_filter(MouseFilter::Ignore)
                .min_size(0.0, container_height)
                .child(Icon::new(icons, "brush", Color::BLACK, TOOLBAR_ICON)),
        );
        let _ = page;
        draw_ui::layout(&mut tree, ViewportSize::new(Size::new(200.0, 200.0)));

        let mut ctx = PaintContext::new();
        draw_ui::paint(&tree, &mut ctx);
        let mut min = Vec2::splat(f32::MAX);
        let mut max = Vec2::splat(f32::MIN);
        for command in ctx.into_draw_list().commands() {
            if let DrawCommand::Line { from, to, .. } = command {
                min = min.min(*from).min(*to);
                max = max.max(*from).max(*to);
            }
        }
        max - min
    }

    #[test]
    fn an_icon_keeps_its_size_when_the_container_gets_taller() {
        // The icon is sized by the component, not by the parent rect, so a
        // compact button does not shrink it.
        let short = icon_extent(24.0);
        let tall = icon_extent(120.0);
        assert!((short.x - tall.x).abs() < 1e-3, "{short:?} vs {tall:?}");
        assert!((short.y - tall.y).abs() < 1e-3, "{short:?} vs {tall:?}");
        assert!(short.x > 0.0 && short.x <= TOOLBAR_ICON + 1e-3);
    }
}
