//! Reusable component API.
//!
//! Components are small builder structs mounted into a [`SceneTree`]:
//!
//! ```ignore
//! let panel = add(tree, root, Panel::new());
//! let vbox = add(tree, panel.id(), VBox::new());
//! let label = add(tree, vbox.id(), Label::new("Hello"));
//! let button = add(tree, vbox.id(), Button::new("Click me").on_click(|| { /* ... */ }));
//! ```
//!
//! Layout is applied per-frame via [`draw_ui::layout`]; state changes only
//! touch core data and the next `paint` produces a fresh `DrawList`.

use draw_core::{Color, Edges, NodeId};
use draw_scene::SceneTree;
use draw_ui::layout::{
    Align, AlignContent, FlexDirection, FlexStyle, GridStyle, Justify, TextOptions, Track,
};
use draw_ui::{ButtonData, ControlData, Widget};

use crate::build::insert;
use crate::view::{child, BuildContext, Child, View};

/// An owned handle to a mounted control.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ControlRef {
    id: NodeId,
}

impl ControlRef {
    pub fn new(id: NodeId) -> Self {
        Self { id }
    }

    pub fn id(self) -> NodeId {
        self.id
    }
}

impl From<ControlRef> for NodeId {
    fn from(control: ControlRef) -> Self {
        control.id
    }
}

/// Something that can be mounted into a [`SceneTree`].
pub trait Component {
    fn mount(self, tree: &mut SceneTree, parent: NodeId) -> ControlRef;
}

/// A card/background control. Fills its parent by default.
pub struct Panel {
    color: Color,
    border: Option<Color>,
    children: Vec<Child>,
}

impl std::fmt::Debug for Panel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Panel")
            .field("color", &self.color)
            .field("border", &self.border)
            .field("children", &self.children.len())
            .finish()
    }
}

impl Default for Panel {
    fn default() -> Self {
        Self {
            color: Color::new(0.13, 0.15, 0.20, 1.0),
            border: Some(Color::new(0.26, 0.30, 0.40, 1.0)),
            children: Vec::new(),
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

    /// Adds one child view.
    pub fn child<V: View + 'static>(mut self, view: V) -> Self {
        self.children.push(child(view));
        self
    }

    /// Adds several child views.
    pub fn children<I, V>(mut self, views: I) -> Self
    where
        I: IntoIterator<Item = V>,
        V: View + 'static,
    {
        self.children.extend(views.into_iter().map(child));
        self
    }
}

impl Component for Panel {
    fn mount(self, tree: &mut SceneTree, parent: NodeId) -> ControlRef {
        let id = insert(
            tree,
            parent,
            "Panel",
            ControlData::fill_parent(),
            Widget::Panel {
                color: self.color,
                border: self.border,
            },
        );
        BuildContext::new(tree, id).children(self.children);
        ControlRef::new(id)
    }
}

/// A text label.
#[derive(Debug, Clone)]
pub struct Label {
    text: String,
    font_size: f32,
    color: Color,
    options: TextOptions,
}

impl Label {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            font_size: 20.0,
            color: Color::new(0.92, 0.94, 0.98, 1.0),
            options: TextOptions::default(),
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

    /// Enables/disables soft wrapping (default on).
    pub fn wrap(mut self, wrap: bool) -> Self {
        self.options.wrap = wrap;
        self
    }

    /// Caps the number of painted lines.
    pub fn max_lines(mut self, max_lines: usize) -> Self {
        self.options = self.options.max_lines(max_lines);
        self
    }

    /// Appends `…` when `max_lines` clips the text.
    pub fn ellipsis(mut self, ellipsis: bool) -> Self {
        self.options = self.options.ellipsis(ellipsis);
        self
    }

    pub fn text_options(mut self, options: TextOptions) -> Self {
        self.options = options;
        self
    }
}

impl Component for Label {
    fn mount(self, tree: &mut SceneTree, parent: NodeId) -> ControlRef {
        ControlRef::new(insert(
            tree,
            parent,
            "Label",
            ControlData::default(),
            Widget::Label {
                text: self.text,
                font_size: self.font_size,
                color: self.color,
                options: self.options,
            },
        ))
    }
}

/// A clickable button with an optional click callback.
pub struct Button {
    text: String,
    on_click: Option<Box<dyn FnMut()>>,
    options: TextOptions,
}

impl Button {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            on_click: None,
            options: TextOptions::no_wrap(),
        }
    }

    /// Registers a callback invoked when the button is clicked (or activated
    /// with Enter/Space).
    pub fn on_click(mut self, callback: impl FnMut() + 'static) -> Self {
        self.on_click = Some(Box::new(callback));
        self
    }

    /// Enables soft wrapping of the button label (default off).
    pub fn wrap(mut self, wrap: bool) -> Self {
        self.options.wrap = wrap;
        self
    }

    /// Caps the number of label lines.
    pub fn max_lines(mut self, max_lines: usize) -> Self {
        self.options = self.options.max_lines(max_lines);
        self
    }

    /// Appends `…` when `max_lines` clips the label.
    pub fn ellipsis(mut self, ellipsis: bool) -> Self {
        self.options = self.options.ellipsis(ellipsis);
        self
    }

    pub fn text_options(mut self, options: TextOptions) -> Self {
        self.options = options;
        self
    }
}

impl Component for Button {
    fn mount(self, tree: &mut SceneTree, parent: NodeId) -> ControlRef {
        let mut data = ButtonData::new(self.text);
        data.options = self.options;
        let id = insert(
            tree,
            parent,
            "Button",
            ControlData::default(),
            Widget::Button(data),
        );
        if let Some(callback) = self.on_click {
            crate::set_on_click(tree, id, callback);
        }
        ControlRef::new(id)
    }
}

/// A vertical stacking container (a column [`Flex`] with a separation).
#[derive(Debug, Clone, Copy)]
pub struct VBox {
    separation: f32,
    padding: Edges,
}

impl Default for VBox {
    fn default() -> Self {
        Self {
            separation: 8.0,
            padding: Edges::all(16.0),
        }
    }
}

impl VBox {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn separation(mut self, separation: f32) -> Self {
        self.separation = separation;
        self
    }

    pub fn padding(mut self, padding: Edges) -> Self {
        self.padding = padding;
        self
    }
}

impl Component for VBox {
    fn mount(self, tree: &mut SceneTree, parent: NodeId) -> ControlRef {
        let style = FlexStyle::column()
            .gap(self.separation)
            .padding(self.padding);
        ControlRef::new(insert(
            tree,
            parent,
            "VBox",
            ControlData::fill_parent(),
            Widget::Flex(style),
        ))
    }
}

/// A horizontal stacking container (a row [`Flex`] with a separation).
#[derive(Debug, Clone, Copy)]
pub struct HBox {
    separation: f32,
    padding: Edges,
}

impl Default for HBox {
    fn default() -> Self {
        Self {
            separation: 8.0,
            padding: Edges::all(16.0),
        }
    }
}

impl HBox {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn separation(mut self, separation: f32) -> Self {
        self.separation = separation;
        self
    }

    pub fn padding(mut self, padding: Edges) -> Self {
        self.padding = padding;
        self
    }
}

impl Component for HBox {
    fn mount(self, tree: &mut SceneTree, parent: NodeId) -> ControlRef {
        let style = FlexStyle::row().gap(self.separation).padding(self.padding);
        ControlRef::new(insert(
            tree,
            parent,
            "HBox",
            ControlData::fill_parent(),
            Widget::Flex(style),
        ))
    }
}

/// A configurable flex container.
pub struct Flex {
    style: FlexStyle,
    children: Vec<Child>,
}

impl std::fmt::Debug for Flex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Flex")
            .field("style", &self.style)
            .field("children", &self.children.len())
            .finish()
    }
}

impl Default for Flex {
    fn default() -> Self {
        Self {
            style: FlexStyle::default(),
            children: Vec::new(),
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
            children: Vec::new(),
        }
    }

    pub fn column() -> Self {
        Self {
            style: FlexStyle::column(),
            children: Vec::new(),
        }
    }

    pub fn direction(mut self, direction: FlexDirection) -> Self {
        self.style.direction = direction;
        self
    }

    pub fn justify(mut self, justify: Justify) -> Self {
        self.style.justify = justify;
        self
    }

    pub fn align(mut self, align: Align) -> Self {
        self.style.align = align;
        self
    }

    pub fn align_content(mut self, align_content: AlignContent) -> Self {
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

    /// Gap between wrapped lines along the cross axis.
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

    /// Adds one child view.
    pub fn child<V: View + 'static>(mut self, view: V) -> Self {
        self.children.push(child(view));
        self
    }

    /// Adds several child views.
    pub fn children<I, V>(mut self, views: I) -> Self
    where
        I: IntoIterator<Item = V>,
        V: View + 'static,
    {
        self.children.extend(views.into_iter().map(child));
        self
    }
}

impl Component for Flex {
    fn mount(self, tree: &mut SceneTree, parent: NodeId) -> ControlRef {
        let id = insert(
            tree,
            parent,
            "Flex",
            ControlData::fill_parent(),
            Widget::Flex(self.style),
        );
        BuildContext::new(tree, id).children(self.children);
        ControlRef::new(id)
    }
}

/// A vertical flex stack with zero default padding/gap.
///
/// ```ignore
/// ui.mount(root, Column::new().gap(12.0).padding(Edges::all(16.0))
///     .child(Label::new("Settings"))
///     .child(Button::new("Save")));
/// ```
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

    pub fn align(mut self, align: Align) -> Self {
        self.flex = self.flex.align(align);
        self
    }

    pub fn justify(mut self, justify: Justify) -> Self {
        self.flex = self.flex.justify(justify);
        self
    }

    pub fn child<V: View + 'static>(mut self, view: V) -> Self {
        self.flex = self.flex.child(view);
        self
    }

    pub fn children<I, V>(mut self, views: I) -> Self
    where
        I: IntoIterator<Item = V>,
        V: View + 'static,
    {
        self.flex = self.flex.children(views);
        self
    }
}

impl std::fmt::Debug for Column {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Column").field("flex", &self.flex).finish()
    }
}

impl View for Column {
    fn build(self, cx: &mut BuildContext) -> NodeId {
        self.flex.build(cx)
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

    pub fn align(mut self, align: Align) -> Self {
        self.flex = self.flex.align(align);
        self
    }

    pub fn justify(mut self, justify: Justify) -> Self {
        self.flex = self.flex.justify(justify);
        self
    }

    pub fn child<V: View + 'static>(mut self, view: V) -> Self {
        self.flex = self.flex.child(view);
        self
    }

    pub fn children<I, V>(mut self, views: I) -> Self
    where
        I: IntoIterator<Item = V>,
        V: View + 'static,
    {
        self.flex = self.flex.children(views);
        self
    }
}

impl std::fmt::Debug for Row {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Row").field("flex", &self.flex).finish()
    }
}

impl View for Row {
    fn build(self, cx: &mut BuildContext) -> NodeId {
        self.flex.build(cx)
    }
}

/// A grid container with fixed / `fr` / auto tracks.
#[derive(Debug, Clone)]
pub struct Grid {
    style: GridStyle,
}

impl Grid {
    pub fn new(columns: Vec<Track>) -> Self {
        Self {
            style: GridStyle::new(columns),
        }
    }

    pub fn rows(mut self, rows: Vec<Track>) -> Self {
        self.style.rows = rows;
        self
    }

    pub fn align_items(mut self, align: Align) -> Self {
        self.style.align_items = align;
        self
    }

    pub fn justify_items(mut self, align: Align) -> Self {
        self.style.justify_items = align;
        self
    }

    pub fn align_content(mut self, align: AlignContent) -> Self {
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
    fn mount(self, tree: &mut SceneTree, parent: NodeId) -> ControlRef {
        ControlRef::new(insert(
            tree,
            parent,
            "Grid",
            ControlData::fill_parent(),
            Widget::Grid(self.style),
        ))
    }
}
