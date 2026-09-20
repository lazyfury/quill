//! Components: node-building values composed with `.child()`.
//!
//! A component builds exactly one primary control node; nesting is expressed by
//! chaining [`Component::child`] or by attaching the component to the scene
//! with [`SceneTree::add_child`](draw_scene::SceneTree::add_child):
//!
//! ```ignore
//! let root = tree.add_child(tree.root(), Flex::column()
//!     .gap(12.0)
//!     .child(Label::new("Hello"))
//!     .child(Button::new("Save").on_click(|| { /* ... */ })));
//! ```
//!
//! Every component carries a [`Spec`] with the layout inputs, background,
//! foreground, click callback and children. The theme is never stored here: a
//! component receives the concrete colors it paints.

use std::cell::RefCell;
use std::rc::Rc;

use draw_core::{Color, Cursor, Edges, NodeId, Rect, Size, Vec2};
use draw_render::PaintContext;
use draw_scene::SceneTree;
use draw_ui::layout::{FlexDirection, FlexStyle, GridStyle, SizeBasis, Track};
use draw_ui::{
    dynamic_surface_decor, foreground_decor, ButtonData, Control, ControlData, DragPhase,
    InteractState, MouseFilter, SurfaceStyle, Widget,
};

use crate::node_ref::{NodeRef, Ref};

/// A child builder stored on a [`Spec`].
pub type ChildFn = Box<dyn FnOnce(&mut SceneTree, NodeId)>;

/// The common node state every component carries.
///
/// Layout fields mirror [`ControlData`]; the rest are the decorators, click
/// callback and children applied when the component is built.
pub struct Spec {
    pub data: ControlData,
    pub background: Option<Box<dyn Fn(InteractState) -> SurfaceStyle>>,
    pub foreground: Option<Box<dyn Fn(&mut PaintContext, Rect, InteractState)>>,
    pub on_click: Option<Box<dyn FnMut()>>,
    pub on_drag: Option<Box<dyn FnMut(&mut SceneTree, DragPhase, Vec2)>>,
    pub on_scroll: Option<Box<dyn FnMut(Vec2)>>,
    pub cursor_provider: Option<Box<dyn Fn() -> Cursor>>,
    pub children: Vec<ChildFn>,
}

impl Default for Spec {
    fn default() -> Self {
        Self {
            data: ControlData::fill_parent(),
            background: None,
            foreground: None,
            on_click: None,
            on_drag: None,
            on_scroll: None,
            cursor_provider: None,
            children: Vec::new(),
        }
    }
}

impl Spec {
    /// A spec with leaf defaults (top-left anchors, so it sizes to contents).
    pub fn leaf() -> Self {
        Self {
            data: ControlData::default(),
            ..Self::default()
        }
    }

    /// Records `child` to be built under this spec's node at mount time.
    ///
    /// This is the `prepare(&mut self)`-friendly counterpart to
    /// [`Component::child`], which requires `self` by value.
    pub fn child<C: Component + 'static>(&mut self, child: C) {
        self.children.push(Box::new(move |tree, parent| {
            child.build(tree, parent);
        }));
    }

    /// Records several children (equivalent to repeated [`Spec::child`]).
    pub fn children<I, C>(&mut self, children: I)
    where
        I: IntoIterator<Item = C>,
        C: Component + 'static,
    {
        for child in children {
            self.child(child);
        }
    }
}

/// A value that builds one primary control node into a [`SceneTree`].
///
/// Implementors embed a [`Spec`], return it from [`Component::spec`], and
/// describe their visual with [`Component::widget`]. The default [`build`]
/// creates the node and applies the spec, so components compose natively with
/// `.child()`, `.background()`, `.grow()` and friends.
///
/// [`build`]: Component::build
pub trait Component: Sized {
    /// The component's common node state.
    fn spec(&mut self) -> &mut Spec;

    /// Node name shown in debug overlays.
    fn name(&self) -> &'static str {
        "Control"
    }

    /// The visual widget for the primary node.
    fn widget(&self) -> Widget;

    /// Finalizes the spec from the component's fields (surface styles, child
    /// closures) after every builder method has run.
    ///
    /// The default does nothing. Override it when a decorator or an internal
    /// child depends on more than one builder value.
    fn prepare(&mut self) {}

    /// Builds this component's node under `parent` and returns its id.
    ///
    /// The default runs [`prepare`](Component::prepare), creates a `Control`
    /// node, installs the widget and applies the spec (layout, decorators,
    /// click callback, children).
    fn build(mut self, tree: &mut SceneTree, parent: NodeId) -> NodeId {
        self.prepare();
        let spec = std::mem::take(self.spec());
        let id = tree.add_control(parent, self.name());
        tree.set_data(id, Control::new(spec.data, self.widget()));
        apply_spec(tree, id, spec);
        id
    }

    /// Adds one child component.
    fn child<C: Component + 'static>(mut self, child: C) -> Self {
        self.spec().children.push(Box::new(move |tree, parent| {
            child.build(tree, parent);
        }));
        self
    }

    /// Adds several child components.
    fn children<I, C>(mut self, children: I) -> Self
    where
        I: IntoIterator<Item = C>,
        C: Component + 'static,
    {
        for child in children {
            self = self.child(child);
        }
        self
    }

    /// Wraps `self` so its mounted `NodeId` is written to `slot`.
    ///
    /// This is the component equivalent of Godot holding a `Node*` from `new()`
    /// / React's `ref` callback: the slot exists before mount and is read after.
    /// It works identically through
    /// [`SceneTree::add_child`](draw_scene::SceneTree::add_child) and
    /// [`Component::child`], because both mount paths run [`build`]
    /// ([`Component::build`]).
    fn ref_(self, slot: &NodeRef) -> Ref<Self> {
        Ref::new(self, {
            let slot = slot.clone();
            move |id| slot.fill(id)
        })
    }

    /// Like [`ref_`](Component::ref_), but reports the mounted id to a callback.
    fn with_ref(self, on_mount: impl FnOnce(NodeId) + 'static) -> Ref<Self> {
        Ref::new(self, on_mount)
    }

    /// Paints a rounded surface behind the node.
    fn background(mut self, color: Color) -> Self {
        self = self.surface(SurfaceStyle::new(color));
        self
    }

    /// Paints an explicit surface style behind the node.
    fn surface(mut self, style: SurfaceStyle) -> Self {
        self.spec().background = Some(Box::new(move |_| style));
        self
    }

    /// A surface whose style is resolved from the interaction state each frame.
    fn dynamic_background(
        mut self,
        resolve: impl Fn(InteractState) -> SurfaceStyle + 'static,
    ) -> Self {
        self.spec().background = Some(Box::new(resolve));
        self
    }

    /// Paints arbitrary chrome in front of the node.
    fn foreground(
        mut self,
        draw: impl Fn(&mut PaintContext, Rect, InteractState) + 'static,
    ) -> Self {
        self.spec().foreground = Some(Box::new(draw));
        self
    }

    /// Runs `callback` when the node is clicked or activated.
    fn on_click(mut self, callback: impl FnMut() + 'static) -> Self {
        self.spec().on_click = Some(Box::new(callback));
        self
    }

    /// Runs `callback` on drag start/move/end while the node is held, with the
    /// delta since the previous event. Gives the node pointer capture.
    fn on_drag(mut self, callback: impl FnMut(&mut SceneTree, DragPhase, Vec2) + 'static) -> Self {
        self.spec().on_drag = Some(Box::new(callback));
        self
    }

    /// Runs `callback` when a wheel event lands on this node or one of its
    /// descendants, with the scroll delta in logical pixels.
    ///
    /// The nearest ancestor with a scroll callback owns the event, so a list
    /// can scroll itself and everything else stays unhandled.
    fn on_scroll(mut self, callback: impl FnMut(Vec2) + 'static) -> Self {
        self.spec().on_scroll = Some(Box::new(callback));
        self
    }

    /// Clips this component's subtree to its own rectangle.
    fn clip(mut self, clip: bool) -> Self {
        self.spec().data.clip = clip;
        self
    }

    /// A cursor resolved from the component's own state each frame while
    /// hovered (overrides [`Component::cursor`] when non-default).
    fn dynamic_cursor(mut self, cursor: impl Fn() -> Cursor + 'static) -> Self {
        self.spec().cursor_provider = Some(Box::new(cursor));
        self
    }

    /// Flex grow factor.
    fn grow(mut self, grow: f32) -> Self {
        self.spec().data.layout.grow = grow;
        self
    }

    /// Flex shrink factor.
    fn shrink(mut self, shrink: f32) -> Self {
        self.spec().data.layout.shrink = shrink;
        self
    }

    /// Flex basis.
    fn basis(mut self, basis: SizeBasis) -> Self {
        self.spec().data.layout.basis = basis;
        self
    }

    /// Minimum intrinsic size.
    fn min_size(mut self, width: f32, height: f32) -> Self {
        self.spec().data.min_size = Size::new(width, height);
        self
    }

    /// Layout order within the parent.
    fn order(mut self, order: i32) -> Self {
        self.spec().data.layout.order = order;
        self
    }

    /// Anchor edges (`0` = parent start, `1` = parent end).
    fn anchors(mut self, anchors: Edges) -> Self {
        self.spec().data.anchors = anchors;
        self
    }

    /// Offset edges, in the same order as [`Edges`].
    fn offsets(mut self, offsets: Edges) -> Self {
        self.spec().data.offsets = offsets;
        self
    }

    /// Pointer hit-test behaviour.
    fn mouse_filter(mut self, filter: MouseFilter) -> Self {
        self.spec().data.mouse_filter = filter;
        self
    }

    /// Cursor the host shows while the pointer is over the node.
    fn cursor(mut self, cursor: draw_core::Cursor) -> Self {
        self.spec().data.cursor = cursor;
        self
    }
}

/// Implements [`SceneChild`](draw_scene::SceneChild) for component types.
///
/// The trait lives in `draw_scene` (so `SceneTree::add_child` stays UI-neutral),
/// so each component type needs its own impl; the macro keeps that to one line
/// per type without introducing a forwarding layer.
#[macro_export]
macro_rules! impl_scene_child {
    ($($t:ty),* $(,)?) => {$(
        impl draw_scene::SceneChild for $t {
            fn attach(
                self,
                tree: &mut draw_scene::SceneTree,
                parent: draw_core::NodeId,
            ) -> draw_core::NodeId {
                <Self as $crate::Component>::build(self, tree, parent)
            }
        }
    )*};
}

impl_scene_child!(Panel, Label, Button, VBox, HBox, Flex, Column, Row, Grid);

/// Applies a spec to an already-created node (decorators, callback, children).
pub fn apply_spec(tree: &mut SceneTree, id: NodeId, spec: Spec) {
    if let Some(background) = spec.background {
        draw_ui::add_decor(tree, id, dynamic_surface_decor(background));
    }
    if let Some(foreground) = spec.foreground {
        draw_ui::add_decor(tree, id, foreground_decor(foreground));
    }
    if let Some(callback) = spec.on_click {
        set_on_click(tree, id, callback);
    }
    if let Some(callback) = spec.on_drag {
        set_on_drag(tree, id, callback);
    }
    if let Some(callback) = spec.on_scroll {
        set_on_scroll(tree, id, callback);
    }
    if let Some(provider) = spec.cursor_provider {
        set_cursor_provider(tree, id, provider);
    }
    for child in spec.children {
        child(tree, id);
    }
}

/// Borrows a control's runtime from the node's extension slot.
pub fn control_mut(tree: &mut SceneTree, id: NodeId) -> Option<&mut Control> {
    tree.data_mut::<Control>(id)
}

/// Registers a click callback on `id`.
pub fn set_on_click<F>(tree: &mut SceneTree, id: NodeId, callback: F) -> bool
where
    F: FnMut() + 'static,
{
    match tree.data_mut::<Control>(id) {
        Some(control) => {
            control.callback = Some(Rc::new(RefCell::new(callback)));
            true
        }
        None => false,
    }
}

/// Registers a pointer-drag callback on `id` (pointer capture while held).
pub fn set_on_drag<F>(tree: &mut SceneTree, id: NodeId, callback: F) -> bool
where
    F: FnMut(&mut SceneTree, DragPhase, Vec2) + 'static,
{
    match tree.data_mut::<Control>(id) {
        Some(control) => {
            control.drag_callback = Some(Rc::new(RefCell::new(callback)));
            true
        }
        None => false,
    }
}

/// Registers a wheel callback on `id`, so it (and its subtree) owns scrolling.
pub fn set_on_scroll<F>(tree: &mut SceneTree, id: NodeId, callback: F) -> bool
where
    F: FnMut(Vec2) + 'static,
{
    match tree.data_mut::<Control>(id) {
        Some(control) => {
            control.scroll_callback = Some(Rc::new(RefCell::new(callback)));
            true
        }
        None => false,
    }
}

/// Registers a dynamic cursor provider on `id`, evaluated while it is hovered.
pub fn set_cursor_provider<F>(tree: &mut SceneTree, id: NodeId, provider: F) -> bool
where
    F: Fn() -> Cursor + 'static,
{
    match tree.data_mut::<Control>(id) {
        Some(control) => {
            control.cursor_provider = Some(Rc::new(provider));
            true
        }
        None => false,
    }
}

/// Replaces a control's text, marking layout dirty only when it changed.
pub fn set_text(tree: &mut SceneTree, id: NodeId, text: impl Into<String>) -> bool {
    let changed = match tree.data_mut::<Control>(id) {
        Some(control) => control.widget.set_text(text),
        None => return false,
    };
    if changed {
        draw_ui::mark_dirty(tree, id);
    }
    true
}

/// Mutates a control's layout data and marks the tree dirty.
pub fn update_control(tree: &mut SceneTree, id: NodeId, f: impl FnOnce(&mut ControlData)) -> bool {
    let changed = match tree.data_mut::<Control>(id) {
        Some(control) => {
            f(&mut control.data);
            true
        }
        None => false,
    };
    if changed {
        draw_ui::mark_dirty(tree, id);
    }
    changed
}

/// A card/background control. Fills its parent by default.
pub struct Panel {
    spec: Spec,
    color: Color,
    border: Option<Color>,
}

impl Default for Panel {
    fn default() -> Self {
        Self {
            spec: Spec::default(),
            color: Color::new(0.13, 0.15, 0.20, 1.0),
            border: Some(Color::new(0.26, 0.30, 0.40, 1.0)),
        }
    }
}

impl Panel {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    pub fn border(mut self, border: Option<Color>) -> Self {
        self.border = border;
        self
    }

    pub fn flat(mut self) -> Self {
        self.border = None;
        self
    }
}

impl Component for Panel {
    fn spec(&mut self) -> &mut Spec {
        &mut self.spec
    }

    fn name(&self) -> &'static str {
        "Panel"
    }

    fn widget(&self) -> Widget {
        Widget::Panel {
            color: self.color,
            border: self.border,
        }
    }
}

/// A text label.
pub struct Label {
    spec: Spec,
    text: String,
    font_size: f32,
    color: Color,
    options: draw_ui::TextOptions,
}

impl Label {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            spec: Spec::leaf(),
            text: text.into(),
            font_size: 20.0,
            color: Color::new(0.92, 0.94, 0.98, 1.0),
            options: draw_ui::TextOptions::default(),
        }
    }

    pub fn font_size(mut self, font_size: f32) -> Self {
        self.font_size = font_size;
        self
    }

    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    pub fn wrap(mut self, wrap: bool) -> Self {
        self.options.wrap = wrap;
        self
    }

    pub fn max_lines(mut self, max_lines: usize) -> Self {
        self.options = self.options.max_lines(max_lines);
        self
    }

    pub fn ellipsis(mut self, ellipsis: bool) -> Self {
        self.options = self.options.ellipsis(ellipsis);
        self
    }

    pub fn text_options(mut self, options: draw_ui::TextOptions) -> Self {
        self.options = options;
        self
    }
}

impl Component for Label {
    fn spec(&mut self) -> &mut Spec {
        &mut self.spec
    }

    fn name(&self) -> &'static str {
        "Label"
    }

    fn widget(&self) -> Widget {
        Widget::Label {
            text: self.text.clone(),
            font_size: self.font_size,
            color: self.color,
            options: self.options,
        }
    }
}

/// A clickable button with an optional click callback.
pub struct Button {
    spec: Spec,
    data: ButtonData,
}

impl Button {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            spec: Spec::leaf(),
            data: ButtonData::new(text),
        }
    }

    pub fn font_size(mut self, font_size: f32) -> Self {
        self.data.font_size = font_size;
        self
    }

    pub fn fill(mut self, color: Color) -> Self {
        self.data.color = color;
        self
    }

    pub fn hover_fill(mut self, color: Color) -> Self {
        self.data.hover_color = color;
        self
    }

    pub fn pressed_fill(mut self, color: Color) -> Self {
        self.data.pressed_color = color;
        self
    }

    pub fn text_color(mut self, color: Color) -> Self {
        self.data.text_color = color;
        self
    }

    pub fn on_click(mut self, callback: impl FnMut() + 'static) -> Self {
        self = Component::on_click(self, callback);
        self
    }
}

impl Component for Button {
    fn spec(&mut self) -> &mut Spec {
        &mut self.spec
    }

    fn name(&self) -> &'static str {
        "Button"
    }

    fn widget(&self) -> Widget {
        Widget::Button(self.data.clone())
    }
}

/// A vertical stacking container (a column [`Flex`] with a separation).
pub struct VBox {
    spec: Spec,
    style: FlexStyle,
}

impl Default for VBox {
    fn default() -> Self {
        Self {
            spec: Spec::default(),
            style: FlexStyle::column().gap(8.0).padding(Edges::all(16.0)),
        }
    }
}

impl VBox {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn separation(mut self, separation: f32) -> Self {
        self.style.gap = separation;
        self.style.cross_gap = separation;
        self
    }

    pub fn padding(mut self, padding: Edges) -> Self {
        self.style.padding = padding;
        self
    }
}

impl Component for VBox {
    fn spec(&mut self) -> &mut Spec {
        &mut self.spec
    }

    fn name(&self) -> &'static str {
        "VBox"
    }

    fn widget(&self) -> Widget {
        Widget::Flex(self.style)
    }
}

/// A horizontal stacking container (a row [`Flex`] with a separation).
pub struct HBox {
    spec: Spec,
    style: FlexStyle,
}

impl Default for HBox {
    fn default() -> Self {
        Self {
            spec: Spec::default(),
            style: FlexStyle::row().gap(8.0).padding(Edges::all(16.0)),
        }
    }
}

impl HBox {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn separation(mut self, separation: f32) -> Self {
        self.style.gap = separation;
        self.style.cross_gap = separation;
        self
    }

    pub fn padding(mut self, padding: Edges) -> Self {
        self.style.padding = padding;
        self
    }
}

impl Component for HBox {
    fn spec(&mut self) -> &mut Spec {
        &mut self.spec
    }

    fn name(&self) -> &'static str {
        "HBox"
    }

    fn widget(&self) -> Widget {
        Widget::Flex(self.style)
    }
}

/// A configurable flex container.
pub struct Flex {
    spec: Spec,
    style: FlexStyle,
}

impl Default for Flex {
    fn default() -> Self {
        Self {
            spec: Spec::default(),
            style: FlexStyle::default(),
        }
    }
}

impl Flex {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn row() -> Self {
        Self {
            style: FlexStyle::row(),
            ..Self::default()
        }
    }

    pub fn column() -> Self {
        Self {
            style: FlexStyle::column(),
            ..Self::default()
        }
    }

    pub fn direction(mut self, direction: FlexDirection) -> Self {
        self.style.direction = direction;
        self
    }

    pub fn justify(mut self, justify: draw_ui::Justify) -> Self {
        self.style.justify = justify;
        self
    }

    pub fn align(mut self, align: draw_ui::Align) -> Self {
        self.style.align = align;
        self
    }

    pub fn align_content(mut self, align_content: draw_ui::AlignContent) -> Self {
        self.style.align_content = align_content;
        self
    }

    pub fn wrap(mut self, wrap: bool) -> Self {
        self.style.wrap = wrap;
        self
    }

    /// Gap between items along the main axis (also called separation).
    pub fn gap(mut self, gap: f32) -> Self {
        self.style.gap = gap;
        self.style.cross_gap = gap;
        self
    }

    pub fn cross_gap(mut self, gap: f32) -> Self {
        self.style.cross_gap = gap;
        self
    }

    pub fn separation(self, separation: f32) -> Self {
        self.gap(separation)
    }

    pub fn padding(mut self, padding: Edges) -> Self {
        self.style.padding = padding;
        self
    }
}

impl Component for Flex {
    fn spec(&mut self) -> &mut Spec {
        &mut self.spec
    }

    fn name(&self) -> &'static str {
        "Flex"
    }

    fn widget(&self) -> Widget {
        Widget::Flex(self.style)
    }
}

/// A vertical flex stack with zero default padding/gap.
pub struct Column {
    flex: Flex,
}

impl Default for Column {
    fn default() -> Self {
        Self::new()
    }
}

impl Column {
    pub fn new() -> Self {
        Self {
            flex: Flex::column().gap(0.0).padding(Edges::ZERO),
        }
    }

    pub fn gap(mut self, gap: f32) -> Self {
        self.flex = self.flex.gap(gap);
        self
    }

    pub fn padding(mut self, padding: Edges) -> Self {
        self.flex = self.flex.padding(padding);
        self
    }

    pub fn align(mut self, align: draw_ui::Align) -> Self {
        self.flex = self.flex.align(align);
        self
    }

    pub fn justify(mut self, justify: draw_ui::Justify) -> Self {
        self.flex = self.flex.justify(justify);
        self
    }
}

impl Component for Column {
    fn spec(&mut self) -> &mut Spec {
        self.flex.spec()
    }

    fn name(&self) -> &'static str {
        "Column"
    }

    fn widget(&self) -> Widget {
        self.flex.widget()
    }
}

/// A horizontal flex row with zero default padding/gap.
pub struct Row {
    flex: Flex,
}

impl Default for Row {
    fn default() -> Self {
        Self::new()
    }
}

impl Row {
    pub fn new() -> Self {
        Self {
            flex: Flex::row().gap(0.0).padding(Edges::ZERO),
        }
    }

    pub fn gap(mut self, gap: f32) -> Self {
        self.flex = self.flex.gap(gap);
        self
    }

    pub fn padding(mut self, padding: Edges) -> Self {
        self.flex = self.flex.padding(padding);
        self
    }

    pub fn align(mut self, align: draw_ui::Align) -> Self {
        self.flex = self.flex.align(align);
        self
    }

    pub fn justify(mut self, justify: draw_ui::Justify) -> Self {
        self.flex = self.flex.justify(justify);
        self
    }
}

impl Component for Row {
    fn spec(&mut self) -> &mut Spec {
        self.flex.spec()
    }

    fn name(&self) -> &'static str {
        "Row"
    }

    fn widget(&self) -> Widget {
        self.flex.widget()
    }
}

/// A grid container with fixed / `fr` / auto tracks.
pub struct Grid {
    spec: Spec,
    style: GridStyle,
}

impl Grid {
    pub fn new(columns: Vec<Track>) -> Self {
        Self {
            spec: Spec::default(),
            style: GridStyle::new(columns),
        }
    }

    pub fn rows(mut self, rows: Vec<Track>) -> Self {
        self.style.rows = rows;
        self
    }

    pub fn align_items(mut self, align: draw_ui::Align) -> Self {
        self.style.align_items = align;
        self
    }

    pub fn justify_items(mut self, align: draw_ui::Align) -> Self {
        self.style.justify_items = align;
        self
    }

    pub fn align_content(mut self, align: draw_ui::AlignContent) -> Self {
        self.style.align_content = align;
        self
    }

    pub fn gap(mut self, gap: f32) -> Self {
        self.style.column_gap = gap;
        self.style.row_gap = gap;
        self
    }

    pub fn column_gap(mut self, gap: f32) -> Self {
        self.style.column_gap = gap;
        self
    }

    pub fn row_gap(mut self, gap: f32) -> Self {
        self.style.row_gap = gap;
        self
    }

    pub fn padding(mut self, padding: Edges) -> Self {
        self.style.padding = padding;
        self
    }
}

impl Component for Grid {
    fn spec(&mut self) -> &mut Spec {
        &mut self.spec
    }

    fn name(&self) -> &'static str {
        "Grid"
    }

    fn widget(&self) -> Widget {
        Widget::Grid(self.style.clone())
    }
}
