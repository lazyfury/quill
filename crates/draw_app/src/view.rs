//! Declarative view layer: compose UI as a tree of values, not as statements
//! against the scene tree.
//!
//! ```ignore
//! use draw_app as ui;
//!
//! ui::mount(&mut tree, root, Column::new().gap(12.0).padding(16.0)
//!     .child(Label::new("Settings").font_size(20.0))
//!     .child(Label::new("Verbose").grow(1.0).min_size(0.0, 40.0)));
//! ```
//!
//! A [`View`] builds exactly one node into a [`BuildContext`]. Containers take
//! `children`; [`ViewExt`] modifiers wrap a view and post-process its node
//! (layout, surface, clicks) without exposing [`NodeId`]s to the caller.
//!
//! Every [`Component`] is automatically a [`View`], so existing builders compose
//! without changes.

use draw_core::{Edges, NodeId, Size};
use draw_scene::SceneTree;
use draw_theme::Theme;

use draw_ui::{dynamic_surface_decor, foreground_decor, surface_decor};
use draw_ui::{ControlData, InteractState, MouseFilter, SurfaceStyle, Widget};

use crate::build::insert;
use crate::Component;

/// A boxed child builder, so heterogeneous children can be collected.
pub type Child = Box<dyn FnOnce(&mut BuildContext) -> NodeId>;

/// Turns `view` into a boxed [`Child`].
pub fn child<V: View + 'static>(view: V) -> Child {
    Box::new(move |cx| view.build(cx))
}

/// A declarative piece of UI that builds one control node.
pub trait View {
    fn build(self, cx: &mut BuildContext) -> NodeId;
}

/// Every [`Component`] is a [`View`] (its `mount` is the low-level form).
impl<T: Component> View for T {
    fn build(self, cx: &mut BuildContext) -> NodeId {
        self.mount(cx.tree, cx.parent).id()
    }
}

/// The mounting context passed to [`View::build`].
///
/// Carries the [`SceneTree`] the node is built into and the parent node.
pub struct BuildContext<'a> {
    pub(crate) tree: &'a mut SceneTree,
    pub(crate) parent: NodeId,
}

impl<'a> BuildContext<'a> {
    /// A context that builds into `parent`.
    pub fn new(tree: &'a mut SceneTree, parent: NodeId) -> Self {
        Self { tree, parent }
    }

    /// The active theme (stored on the tree root).
    pub fn theme(&self) -> Theme {
        draw_ui::theme(self.tree)
    }

    /// The node this context builds into.
    pub fn parent(&self) -> NodeId {
        self.parent
    }

    /// A context that builds into `parent` instead.
    pub fn at(&mut self, parent: NodeId) -> BuildContext<'_> {
        BuildContext {
            tree: self.tree,
            parent,
        }
    }

    /// Builds `view` as a child of this context's parent.
    pub fn child<V: View + 'static>(&mut self, view: V) -> NodeId {
        view.build(self)
    }

    /// Builds every boxed `child` into this context's parent.
    pub fn children(&mut self, children: Vec<Child>) {
        for child in children {
            child(self);
        }
    }

    /// Escape hatch: the scene tree the view is built into.
    pub fn tree(&mut self) -> &mut SceneTree {
        self.tree
    }
}

/// Builder modifiers shared by every [`View`].
///
/// Each modifier wraps the view and applies one runtime property to the node it
/// produced, so they compose in any order:
///
/// ```ignore
/// Label::new("Hi").grow(1.0).min_size(0.0, 32.0).background(style).on_click(cb)
/// ```
pub trait ViewExt: View + Sized {
    /// Flex grow factor.
    fn grow(self, grow: f32) -> Modify<Self> {
        Modify::new(self, move |tree, node| {
            crate::update_control(tree, node, |data| data.layout.grow = grow);
        })
    }

    /// Flex shrink factor.
    fn shrink(self, shrink: f32) -> Modify<Self> {
        Modify::new(self, move |tree, node| {
            crate::update_control(tree, node, |data| data.layout.shrink = shrink);
        })
    }

    /// Flex basis.
    fn basis(self, basis: draw_ui::layout::SizeBasis) -> Modify<Self> {
        Modify::new(self, move |tree, node| {
            crate::update_control(tree, node, |data| data.layout.basis = basis);
        })
    }

    /// Minimum size.
    fn min_size(self, width: f32, height: f32) -> Modify<Self> {
        Modify::new(self, move |tree, node| {
            crate::update_control(tree, node, |data| data.min_size = Size::new(width, height));
        })
    }

    /// Layout order within the parent.
    fn order(self, order: i32) -> Modify<Self> {
        Modify::new(self, move |tree, node| {
            crate::update_control(tree, node, |data| data.layout.order = order);
        })
    }

    /// Anchor edges (`0` = parent start, `1` = parent end).
    fn anchors(self, anchors: Edges) -> Modify<Self> {
        Modify::new(self, move |tree, node| {
            crate::update_control(tree, node, |data| data.anchors = anchors);
        })
    }

    /// Offset edges, in the same order as [`Edges`].
    fn offsets(self, offsets: Edges) -> Modify<Self> {
        Modify::new(self, move |tree, node| {
            crate::update_control(tree, node, |data| data.offsets = offsets);
        })
    }

    /// A themed surface painted behind the node.
    fn background(self, style: SurfaceStyle) -> Modify<Self> {
        Modify::new(self, move |tree, node| {
            draw_ui::add_decor(tree, node, surface_decor(style));
        })
    }

    /// A themed surface whose style is recomputed from the interaction state
    /// each frame (selection, hover, active).
    fn dynamic_background(
        self,
        resolve: impl Fn(&Theme, InteractState) -> SurfaceStyle + 'static,
    ) -> Modify<Self> {
        Modify::new(self, move |tree, node| {
            let theme = draw_ui::theme(tree);
            draw_ui::add_decor(tree, node, dynamic_surface_decor(theme, resolve));
        })
    }

    /// A themed foreground painted in front of the node.
    fn foreground(
        self,
        draw: impl Fn(&mut draw_render::PaintContext, draw_core::Rect, &Theme, InteractState) + 'static,
    ) -> Modify<Self> {
        Modify::new(self, move |tree, node| {
            let theme = draw_ui::theme(tree);
            draw_ui::add_decor(tree, node, foreground_decor(theme, draw));
        })
    }

    /// Click callback for the node and its descendants.
    fn on_click(self, callback: impl FnMut() + 'static) -> Modify<Self> {
        Modify::new(self, move |tree, node| {
            crate::set_on_click(tree, node, callback);
        })
    }

    /// Runs `f` with the node this view built (id capture / escape hatch).
    fn capture(self, f: impl FnOnce(NodeId) + 'static) -> Modify<Self> {
        Modify::new(self, move |_tree, node| f(node))
    }

    /// Mouse filter.
    fn mouse_filter(self, filter: MouseFilter) -> Modify<Self> {
        Modify::new(self, move |tree, node| {
            crate::update_control(tree, node, |data| data.mouse_filter = filter);
        })
    }
}

impl<V: View> ViewExt for V {}

/// A view wrapped with one or more node modifiers.
pub struct Modify<V> {
    inner: V,
    apply: Box<dyn FnOnce(&mut SceneTree, NodeId)>,
}

impl<V> Modify<V> {
    fn new(inner: V, apply: impl FnOnce(&mut SceneTree, NodeId) + 'static) -> Self {
        Self {
            inner,
            apply: Box::new(apply),
        }
    }

    /// Runs another modifier after this one, keeping a single wrapper type.
    pub fn and(self, apply: impl FnOnce(&mut SceneTree, NodeId) + 'static) -> Self {
        let Modify {
            inner,
            apply: first,
        } = self;
        Self {
            inner,
            apply: Box::new(move |tree, node| {
                first(tree, node);
                apply(tree, node);
            }),
        }
    }
}

impl<V: View> View for Modify<V> {
    fn build(self, cx: &mut BuildContext) -> NodeId {
        let Modify { inner, apply } = self;
        let node = inner.build(cx);
        apply(cx.tree, node);
        node
    }
}

/// Convenience: builds a `Widget` node directly (for custom views).
pub fn widget(cx: &mut BuildContext, name: &'static str, widget: Widget) -> NodeId {
    insert(cx.tree, cx.parent, name, ControlData::default(), widget)
}
