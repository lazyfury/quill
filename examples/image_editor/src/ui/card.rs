//! 项目内的简单卡片：和左侧调色盘面板同一套外观。
//!
//! 一个竖直 `Flex`：`surface(SurfaceLevel::Surface)` 底 + 内边距 + 行间距，
//! 不带边框 / 圆角。右栏的「文件 / 图层 / 属性 / 历史」和左侧调色盘都用它，
//! 于是侧栏各块看起来是同一系列的卡片（而不是核心 `draw_components::Card`
//! 的 raised 圆角风格）。
//!
//! 只是外观容器：标题、分隔线、内容由调用方自己 `.child(..)`。

use draw_components::{Component, Spec};
use draw_core::Edges;
use draw_theme::{space, SurfaceLevel, Theme};
use draw_ui::{FlexStyle, MouseFilter, SurfaceStyle, Widget};

/// 一块侧栏卡片：surface 底 + 内边距 + 纵向间距。
pub struct Card {
    spec: Spec,
    theme: Theme,
    gap: f32,
    padding: Edges,
}

impl Card {
    /// 默认：`space::SM` 的内边距和行间距（和调色盘面板一致）。
    pub fn new(theme: Theme) -> Self {
        Self {
            spec: Spec::default(),
            theme,
            gap: space::SM,
            padding: Edges::all(space::SM),
        }
    }

    /// 行间距（覆盖默认的 `space::SM`）。
    pub fn gap(mut self, gap: f32) -> Self {
        self.gap = gap;
        self
    }
}

impl Component for Card {
    fn spec(&mut self) -> &mut Spec {
        &mut self.spec
    }

    fn name(&self) -> &'static str {
        "Card"
    }

    fn widget(&self) -> Widget {
        Widget::Flex(FlexStyle::column().gap(self.gap).padding(self.padding))
    }

    fn prepare(&mut self) {
        let theme = self.theme;
        // 容器本身不吃指针，卡片里的按钮 / 列表照常命中。
        self.spec.data.mouse_filter = MouseFilter::Ignore;
        self.spec.background = Some(Box::new(move |_| {
            SurfaceStyle::new(theme.surface(SurfaceLevel::Raised))
        }));
    }
}
