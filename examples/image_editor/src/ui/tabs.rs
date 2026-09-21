//! 侧栏标签页（项目内组件）：把「文件 / 历史 / 属性」三块内容收进一个卡片 ——
//! 上面一排标签按钮，下面只露出当前标签的内容。
//!
//! 只负责外观与点击：内容组件由调用方通过 [`TabsView::tab`] 传入，激活态是
//! 调用方持有的共享 `Rc<Cell<..>>`；标签按钮点击直接翻转它。真正的显示 / 隐藏
//! 由 [`EditorView`](crate::ui::EditorView) 在激活页变化时调 [`show_active`]
//! 同步 —— 隐藏的内容不参与布局与绘制，所以没打开的 `List` 一个行池都不占。

use std::cell::Cell;
use std::rc::Rc;

use draw_components::{Component, Flex, NodeRef, Spec, Text};
use draw_core::{Color, Edges, NodeId, Vec2};
use draw_scene::SceneTree;
use draw_theme::{space, SurfaceLevel, Theme};
use draw_ui::{FlexStyle, MouseFilter, SurfaceStyle, Widget};

/// 侧栏标签页。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SidebarTab {
    #[default]
    File,
    History,
    Properties,
}

impl SidebarTab {
    /// 标签上的文字。
    pub const fn label(self) -> &'static str {
        match self {
            Self::File => "文件",
            Self::History => "历史",
            Self::Properties => "属性",
        }
    }
}

/// 标签页容器：卡片表面 + 一排标签 + 当前内容。
pub struct TabsView {
    spec: Spec,
    theme: Theme,
    active: Rc<Cell<SidebarTab>>,
    /// 标签按钮的横排；`.tab()` 逐个追加，`prepare` 时挂到卡片顶部。
    tabs: Flex,
}

impl TabsView {
    pub fn new(theme: Theme, active: Rc<Cell<SidebarTab>>) -> Self {
        Self {
            spec: Spec::default(),
            theme,
            active,
            tabs: Flex::row()
                .gap(space::XXS)
                .padding(Edges::ZERO)
                .mouse_filter(MouseFilter::Ignore),
        }
    }

    /// 加一个标签页：`button` / `content` 分别收下按钮与内容容器的节点，
    /// `view` 是这个标签的内容；调用顺序就是标签从左到右的顺序。
    pub fn tab<C: Component + 'static>(
        mut self,
        tab: SidebarTab,
        button: &NodeRef,
        content: &NodeRef,
        view: C,
    ) -> Self {
        self.tabs = std::mem::take(&mut self.tabs).child(tab_button(
            self.theme,
            tab,
            self.active.clone(),
            button,
        ));
        // 每个内容单独包一层容器，视图靠它的可见性二选一。
        let slot = content.clone();
        self.spec.children.push(Box::new(move |tree, parent| {
            Flex::column()
                .gap(0.0)
                .padding(Edges::ZERO)
                .grow(1.0)
                .child(view)
                .ref_(&slot)
                .build(tree, parent);
        }));
        self
    }
}

impl Component for TabsView {
    fn spec(&mut self) -> &mut Spec {
        &mut self.spec
    }

    fn name(&self) -> &'static str {
        "TabsView"
    }

    fn widget(&self) -> Widget {
        Widget::Flex(
            FlexStyle::column()
                .gap(space::SM)
                .padding(Edges::all(space::SM)),
        )
    }

    fn prepare(&mut self) {
        let theme = self.theme;
        // 卡片本身不吃指针，标签按钮 / 内容里的控件照常命中。
        self.spec.data.mouse_filter = MouseFilter::Ignore;
        self.spec.background = Some(Box::new(move |_| {
            SurfaceStyle::new(theme.surface(SurfaceLevel::Raised))
        }));
        // 标签排在最前，内容依次在后。
        let tabs = std::mem::take(&mut self.tabs);
        self.spec.children.insert(
            0,
            Box::new(move |tree, parent| {
                tabs.build(tree, parent);
            }),
        );
    }
}

/// 同步标签页可见性：只让 `active` 对应的内容可见。
///
/// 可见性变化本身**不会**让布局失效，所以这里显式把面板标脏重排 —— 否则刚露出
/// 来的内容会停在隐藏时的零矩形（画在 `(0, 0)`）。编辑器只在激活页变化时调用，
/// 不会每帧重排。
pub fn show_active(
    tree: &mut SceneTree,
    panel: NodeId,
    active: SidebarTab,
    contents: &[(SidebarTab, NodeId)],
) {
    for (tab, node) in contents {
        tree.set_visible(*node, *tab == active);
    }
    draw_ui::mark_dirty(tree, panel);
}

/// 一个标签按钮：点击选中；选中的有底色 + 底部一条 accent 下划线。
fn tab_button(
    theme: Theme,
    tab: SidebarTab,
    active: Rc<Cell<SidebarTab>>,
    slot: &NodeRef,
) -> impl Component {
    let fill = active.clone();
    let clicked = active.clone();
    let underline = active.clone();
    Flex::row()
        .gap(0.0)
        .padding(Edges::new(space::SM, space::XS, space::SM, space::XS))
        .on_click(move || clicked.set(tab))
        .dynamic_background(move |state| {
            if fill.get() == tab {
                SurfaceStyle::new(theme.palette.selection)
            } else if state.hovered {
                SurfaceStyle::new(theme.palette.surface_hover)
            } else {
                SurfaceStyle::new(Color::TRANSPARENT)
            }
        })
        .foreground(move |ctx, rect, _| {
            if underline.get() == tab {
                let y = rect.max().y - 1.0;
                ctx.draw_line(
                    Vec2::new(rect.min().x, y),
                    Vec2::new(rect.max().x, y),
                    2.0,
                    theme.palette.accent,
                );
            }
        })
        .child(Text::small(tab.label(), theme))
        .ref_(slot)
}

#[cfg(test)]
mod tests {
    use super::*;
    use draw_core::{Size, ViewportSize};
    use draw_ui::SizeBasis;

    #[test]
    fn tab_contents_stack_below_the_tab_bar() {
        let theme = Theme::dark();
        let active = Rc::new(Cell::new(SidebarTab::File));
        let file = (NodeRef::new(), NodeRef::new());
        let hist = (NodeRef::new(), NodeRef::new());
        let view = TabsView::new(theme, active)
            .tab(
                SidebarTab::File,
                &file.0,
                &file.1,
                Text::small("文件内容", theme),
            )
            .tab(
                SidebarTab::History,
                &hist.0,
                &hist.1,
                Text::small("历史内容", theme),
            );
        let mut tree = SceneTree::new();
        tree.add_child(
            tree.root(),
            Flex::column().basis(SizeBasis::Px(200.0)).child(view),
        );
        tree.set_viewport_size(Size::new(320.0, 240.0));
        draw_ui::layout(&mut tree, ViewportSize::new(Size::new(320.0, 240.0)));

        let file_rect = draw_ui::control(&tree, file.1.get().unwrap()).unwrap().rect;
        let hist_rect = draw_ui::control(&tree, hist.1.get().unwrap()).unwrap().rect;
        assert!(file_rect.top() > 0.0, "内容应在标签栏下面：{file_rect:?}");
        assert!(
            hist_rect.top() >= file_rect.max().y,
            "内容应纵向堆叠：file={file_rect:?} hist={hist_rect:?}"
        );
    }

    #[test]
    fn every_tab_has_a_distinct_label() {
        let tabs = [
            SidebarTab::File,
            SidebarTab::History,
            SidebarTab::Properties,
        ];
        let mut labels: Vec<&str> = tabs.iter().map(|tab| tab.label()).collect();
        labels.sort_unstable();
        labels.dedup();
        assert_eq!(labels.len(), tabs.len());
        assert_eq!(SidebarTab::default(), SidebarTab::File);
    }

    /// 建两页标签、排好，返回 `(tree, panel, file_content, hist_content)`。
    fn mounted() -> (SceneTree, NodeId, NodeId, NodeId) {
        let theme = Theme::dark();
        let active = Rc::new(Cell::new(SidebarTab::File));
        let file = (NodeRef::new(), NodeRef::new());
        let hist = (NodeRef::new(), NodeRef::new());
        let tabs_ref = NodeRef::new();
        let view = TabsView::new(theme, active)
            .tab(
                SidebarTab::File,
                &file.0,
                &file.1,
                Text::small("文件内容", theme),
            )
            .tab(
                SidebarTab::History,
                &hist.0,
                &hist.1,
                Text::small("历史内容", theme),
            )
            .ref_(&tabs_ref);
        let mut tree = SceneTree::new();
        tree.add_child(
            tree.root(),
            Flex::column().basis(SizeBasis::Px(200.0)).child(view),
        );
        tree.set_viewport_size(Size::new(320.0, 240.0));
        (
            tree,
            tabs_ref.get().unwrap(),
            file.1.get().unwrap(),
            hist.1.get().unwrap(),
        )
    }

    fn content_rect(tree: &SceneTree, id: NodeId) -> draw_core::Rect {
        draw_ui::control(tree, id).unwrap().rect
    }

    #[test]
    fn show_active_keeps_only_one_content_visible() {
        let (mut tree, panel, file, hist) = mounted();
        let viewport = ViewportSize::new(Size::new(320.0, 240.0));

        show_active(
            &mut tree,
            panel,
            SidebarTab::File,
            &[(SidebarTab::File, file), (SidebarTab::History, hist)],
        );
        draw_ui::layout(&mut tree, viewport);
        assert!(content_rect(&tree, file).top() > 0.0);
        assert_eq!(content_rect(&tree, hist), draw_core::Rect::ZERO);
    }

    #[test]
    fn showing_a_hidden_tab_relays_it_out_from_zero() {
        let (mut tree, panel, file, hist) = mounted();
        let viewport = ViewportSize::new(Size::new(320.0, 240.0));
        let contents = [(SidebarTab::File, file), (SidebarTab::History, hist)];

        show_active(&mut tree, panel, SidebarTab::File, &contents);
        draw_ui::layout(&mut tree, viewport);
        assert_eq!(content_rect(&tree, hist), draw_core::Rect::ZERO);

        show_active(&mut tree, panel, SidebarTab::History, &contents);
        draw_ui::layout(&mut tree, viewport);
        let hist_rect = content_rect(&tree, hist);
        assert!(
            hist_rect.top() > 0.0,
            "切换后内容应重新排布，而不是停在 (0, 0)：{hist_rect:?}"
        );
        assert_eq!(content_rect(&tree, file), draw_core::Rect::ZERO);
    }

    #[test]
    fn a_tab_mounts_a_button_and_a_content_node() {
        let mut tree = SceneTree::new();
        let active = Rc::new(Cell::new(SidebarTab::File));
        let button = NodeRef::new();
        let content = NodeRef::new();
        let view = TabsView::new(Theme::dark(), active).tab(
            SidebarTab::File,
            &button,
            &content,
            Text::small("内容", Theme::dark()),
        );
        // `TabsView` 是项目内类型，没有 `SceneChild`;用一层 `Flex` 挂载即可。
        tree.add_child(tree.root(), Flex::column().child(view));
        assert!(button.is_set(), "标签按钮应已挂载");
        assert!(content.is_set(), "内容容器应已挂载");
    }
}
