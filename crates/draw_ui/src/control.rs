use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use draw_core::{Cursor, Edges, NodeId, Rect, Size, Vec2, ViewportSize};
use draw_scene::SceneTree;

use crate::decor::DecorRef;
use crate::layout::{ApproxTextMeasurer, ContentSize, LayoutStyle, TextMeasurer, TextOptions};
use crate::widget::Widget;

/// How a control reacts to pointer events during hit testing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum MouseFilter {
    /// Consume the event and stop the search (topmost control wins).
    #[default]
    Stop,
    /// Report the hit but let a control underneath also be considered.
    Pass,
    /// Never hit-testable (transparent to the pointer).
    Ignore,
}

/// Layout data for a control.
///
/// The rectangle is resolved from the parent rectangle using the Godot-style
/// anchor/offset model:
///
/// ```text
/// left  = parent.left + parent.width  * anchor.left  + offset.left
/// right = parent.left + parent.width  * anchor.right + offset.right
/// ```
///
/// `anchor` components are normally `0.0` or `1.0`; `offset` is in pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ControlData {
    pub anchors: Edges,
    pub offsets: Edges,
    pub min_size: Size,
    /// Absolute rectangle in logical viewport coordinates, valid after layout.
    pub rect: Rect,
    pub mouse_filter: MouseFilter,
    /// Cursor the host should show while the pointer is over this control.
    pub cursor: Cursor,
    /// Dimmed and inert: hover is ignored and a click never fires. Components
    /// that paint disabled state read it through
    /// [`InteractState::disabled`](crate::InteractState::disabled).
    pub disabled: bool,
    /// How this control participates in its parent container's layout.
    pub layout: LayoutStyle,
    /// Whether this control clips its subtree to its own rectangle.
    ///
    /// Opt-in, and the only reason `draw_ui` ever emits
    /// [`DrawCommand::ClipRect`](draw_render::DrawCommand::ClipRect): a
    /// scrolling list needs its rows cut at the viewport edge, and a label
    /// whose text overflows its control should stop at the control's bounds.
    pub clip: bool,
    /// The clip this control is actually drawn under, inherited from the
    /// nearest clipping ancestors and intersected with their rectangles.
    ///
    /// Resolved by [`layout`](crate::layout) — it is a function of the final
    /// rectangles, never set by hand. `None` means "nothing clips this
    /// control"; `Some(rect)` with [`Rect::is_empty`] means the control is
    /// clipped away entirely and is skipped when painting and hit-testing.
    pub clip_rect: Option<Rect>,
}

impl Default for ControlData {
    fn default() -> Self {
        Self {
            anchors: Edges::ZERO,
            offsets: Edges::ZERO,
            min_size: Size::ZERO,
            rect: Rect::ZERO,
            mouse_filter: MouseFilter::Stop,
            cursor: Cursor::Default,
            disabled: false,
            layout: LayoutStyle::default(),
            clip: false,
            clip_rect: None,
        }
    }
}

impl ControlData {
    pub fn new() -> Self {
        Self::default()
    }

    /// Fills the parent (anchors span `0..1`, zero offsets).
    pub fn fill_parent() -> Self {
        Self {
            anchors: Edges::new(0.0, 0.0, 1.0, 1.0),
            ..Self::default()
        }
    }

    /// Resolves this control's rectangle against `parent`.
    ///
    /// The size is clamped from the top-left so it never shrinks below
    /// `min_size`.
    pub fn resolve_rect(&self, parent: Rect) -> Rect {
        let left = parent.left() + parent.size.width * self.anchors.left + self.offsets.left;
        let top = parent.top() + parent.size.height * self.anchors.top + self.offsets.top;
        let right = parent.left() + parent.size.width * self.anchors.right + self.offsets.right;
        let bottom = parent.top() + parent.size.height * self.anchors.bottom + self.offsets.bottom;

        let width = (right - left).max(self.min_size.width);
        let height = (bottom - top).max(self.min_size.height);
        Rect::from_min_size(Vec2::new(left, top), Size::new(width, height))
    }
}

/// A callback invoked when a control is activated (clicked / Enter).
pub type ClickCallback = Rc<RefCell<dyn FnMut()>>;

/// Phase of a pointer drag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DragPhase {
    /// The pointer went down on the control; `delta` is zero.
    Start,
    /// Pointer moved; `delta` is the change since the previous event.
    Move,
    /// The pointer was released; `delta` is zero.
    End,
}

/// A callback invoked while a control owns a pointer drag.
///
/// It receives the owning tree, the drag [`DragPhase`] and the **delta** since
/// the previous pointer event (logical pixels), so a component can accumulate
/// the drag and react to start/end without tracking the pointer itself.
pub type DragCallback = Rc<RefCell<dyn FnMut(&mut SceneTree, DragPhase, Vec2)>>;

/// A callback invoked while the pointer is **pressed** on a control, with the
/// control's current rect and the pointer position (same coordinate space).
///
/// Unlike [`DragCallback`] (which only reports a delta), this gives an absolute
/// position, so a component can implement a slider / colour picker that maps the
/// pointer onto its own rectangle. Fires on press and on every move while held.
pub type PointerCallback = Rc<RefCell<dyn FnMut(Rect, Vec2)>>;

/// A closure returning a control's cursor, evaluated by the framework while the
/// control is hovered. Lets a component derive its cursor from its own state
/// instead of a fixed value.
pub type CursorProvider = Rc<dyn Fn() -> Cursor>;

/// A callback invoked when a wheel event lands on a control or one of its
/// descendants, with the scroll delta in logical pixels (`y > 0` scrolls down).
///
/// The nearest ancestor carrying one owns the event: a list scrolls its own
/// rows and stops there, and a wheel over anything else stays `Ignored`.
pub type ScrollCallback = Rc<RefCell<dyn FnMut(Vec2)>>;

/// Resolves the clip rectangle a control draws its subtree under.
///
/// A non-clipping control simply passes `inherited` through; a clipping one
/// intersects the inherited clip with its own rectangle. A disjoint
/// intersection collapses to [`Rect::ZERO`] rather than `None`, so "clipped
/// away entirely" stays distinguishable from "nothing clips me".
pub(crate) fn resolve_clip(rect: Rect, clip: bool, inherited: Option<Rect>) -> Option<Rect> {
    if !clip {
        return inherited;
    }
    Some(match inherited {
        Some(outer) => outer.intersection(rect).unwrap_or(Rect::ZERO),
        None => rect,
    })
}

/// Per-node UI runtime stored in a `SceneTree` node's extension slot.
///
/// This is the control-side counterpart of [`ControlData`]: everything a
/// `Control` owns at runtime (its visual [`Widget`], click callback, themed
/// decorations and layout-dirty flag) lives here, on the node, not in the UI namespace.
/// `Ui` only keeps the environment (theme, text measurement, layout caches).
pub struct Control {
    pub data: ControlData,
    pub widget: Widget,
    pub callback: Option<ClickCallback>,
    /// Pointer-drag callback (pointer capture while held).
    pub drag_callback: Option<DragCallback>,
    /// Absolute-position pointer callback (press + move while held).
    pub pointer_callback: Option<PointerCallback>,
    /// Wheel callback: this control (or its subtree) owns mouse-wheel scrolling.
    pub scroll_callback: Option<ScrollCallback>,
    /// Dynamic cursor, resolved each frame while hovered; overrides
    /// [`ControlData::cursor`] when it returns a non-default value.
    pub cursor_provider: Option<CursorProvider>,
    /// Themed chrome attached by components (surfaces, foregrounds).
    pub decorations: Vec<DecorRef>,
    /// Set when this control's layout inputs changed; cleared as it is arranged.
    pub layout_dirty: bool,
}

impl Control {
    pub fn new(data: ControlData, widget: Widget) -> Self {
        Self {
            data,
            widget,
            callback: None,
            drag_callback: None,
            pointer_callback: None,
            scroll_callback: None,
            cursor_provider: None,
            decorations: Vec::new(),
            layout_dirty: true,
        }
    }
}

/// Viewport-level GUI interaction state (Godot `Viewport`'s `gui.*` state).
///
/// Stored in the root node's extension slot, so pointer hover / press / focus
/// ownership belongs to the scene tree rather than to the UI namespace.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct GuiState {
    pub hovered: Option<draw_core::NodeId>,
    pub pressed: Option<draw_core::NodeId>,
    pub focused: Option<draw_core::NodeId>,
    /// Node that owns the current pointer drag (pointer capture).
    pub dragging: Option<draw_core::NodeId>,
    /// Last pointer position observed while dragging (logical pixels).
    pub drag_last: Vec2,
}

/// Reads the UI runtime bundle from a node's extension slot.
pub(crate) fn control_of(tree: &SceneTree, id: draw_core::NodeId) -> Option<&Control> {
    tree.get(id).and_then(|node| node.data::<Control>())
}

/// Mutably borrows the UI runtime bundle from a node's extension slot.
pub(crate) fn control_mut(tree: &mut SceneTree, id: draw_core::NodeId) -> Option<&mut Control> {
    tree.get_mut(id).and_then(|node| node.data_mut::<Control>())
}

/// Effective visibility of `id`, walking the local `visible` flags up to the
/// root.
///
/// Unlike [`SceneTree::is_visible_in_tree`] this does not depend on a prior
/// [`SceneTree::update`], so it is safe to consult during layout, paint and
/// input (a node hidden by a router switches immediately).
pub(crate) fn control_visible(tree: &SceneTree, id: draw_core::NodeId) -> bool {
    let mut current = Some(id);
    while let Some(node) = current {
        if tree.is_visible(node) == Some(false) {
            return false;
        }
        current = tree.parent(node);
    }
    true
}

/// Cached laid-out text for one control (paint-side).
pub(crate) struct CachedText {
    pub(crate) text: String,
    pub(crate) font_size_bits: u32,
    pub(crate) width_bits: u32,
    pub(crate) options: TextOptions,
    pub(crate) lines: Rc<[String]>,
}

/// The layout engine's per-root memoization and pass counters.
///
/// Stored in the root node (behind a `RefCell`), so a `Ui` environment carries
/// no per-tree state and can drive more than one tree. Fill/read it with
/// [`root_state`] / [`root_state_mut`].
#[derive(Default)]
pub(crate) struct LayoutCache {
    pub(crate) valid: bool,
    pub(crate) viewport: ViewportSize,
    pub(crate) count: u64,
    pub(crate) last_arranged: usize,
    /// Cached child ordering per container (`LayoutStyle::order`).
    pub(crate) order: HashMap<NodeId, Vec<NodeId>>,
    /// Paint-side cache of wrapped/clipped lines per control.
    pub(crate) text: HashMap<NodeId, CachedText>,
    /// Per-pass memoization of `measure_node` results.
    pub(crate) measure: HashMap<(NodeId, u32, u32), ContentSize>,
}

/// Everything the UI layer stores on the root node: the text measurer,
/// viewport GUI interaction state and the layout cache.
///
/// `Ui` is a zero-sized environment on top of this; the tree (root node) is the
/// single owner of UI state. The theme is not stored here: it is a plain value
/// passed to component builders by the application.
pub(crate) struct UiRootState {
    pub(crate) text_measurer: Rc<dyn TextMeasurer>,
    pub(crate) gui: GuiState,
    pub(crate) layout: RefCell<LayoutCache>,
}

impl Default for UiRootState {
    fn default() -> Self {
        Self {
            text_measurer: Rc::new(ApproxTextMeasurer),
            gui: GuiState::default(),
            layout: RefCell::new(LayoutCache::default()),
        }
    }
}

/// Fallback measurer used before a root state (and thus a real measurer) is
/// installed.
pub(crate) static DEFAULT_MEASURER: ApproxTextMeasurer = ApproxTextMeasurer;

/// Reads the root node's UI state, if initialized.
pub(crate) fn root_state(tree: &SceneTree) -> Option<&UiRootState> {
    tree.node(tree.root()).data::<UiRootState>()
}

/// Mutably borrows the root node's UI state, creating it on first use.
pub(crate) fn root_state_mut(tree: &mut SceneTree) -> &mut UiRootState {
    let root = tree.root();
    if tree.node(root).data::<UiRootState>().is_none() {
        tree.node_mut(root).set_data(UiRootState::default());
    }
    tree.node_mut(root)
        .data_mut::<UiRootState>()
        .expect("root UI state initialized")
}

/// Reads the viewport GUI state from the root node, if initialized.
pub fn gui_state(tree: &SceneTree) -> Option<&GuiState> {
    root_state(tree).map(|state| &state.gui)
}

/// Mutably borrows the viewport GUI state, creating it on first use.
pub fn gui_state_mut(tree: &mut SceneTree) -> &mut GuiState {
    &mut root_state_mut(tree).gui
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fill_parent_resolves_to_parent() {
        let parent = Rect::from_min_size(Vec2::new(10.0, 20.0), Size::new(200.0, 100.0));
        let control = ControlData::fill_parent();
        assert_eq!(control.resolve_rect(parent), parent);
    }

    #[test]
    fn top_left_anchor_with_offsets() {
        let parent = Rect::from_min_size(Vec2::ZERO, Size::new(200.0, 100.0));
        let control = ControlData {
            anchors: Edges::new(0.0, 0.0, 0.0, 0.0),
            offsets: Edges::new(10.0, 20.0, 60.0, 50.0),
            ..ControlData::default()
        };
        assert_eq!(
            control.resolve_rect(parent),
            Rect::from_min_size(Vec2::new(10.0, 20.0), Size::new(50.0, 30.0))
        );
    }

    #[test]
    fn min_size_is_enforced() {
        let parent = Rect::from_min_size(Vec2::ZERO, Size::new(10.0, 10.0));
        let control = ControlData {
            anchors: Edges::new(0.0, 0.0, 0.0, 0.0),
            offsets: Edges::ZERO,
            min_size: Size::new(80.0, 40.0),
            ..ControlData::default()
        };
        assert_eq!(
            control.resolve_rect(parent),
            Rect::from_min_size(Vec2::ZERO, Size::new(80.0, 40.0))
        );
    }

    #[test]
    fn right_anchor_grows_with_parent() {
        let parent = Rect::from_min_size(Vec2::ZERO, Size::new(200.0, 100.0));
        let control = ControlData {
            anchors: Edges::new(0.0, 0.0, 1.0, 0.0),
            offsets: Edges::new(0.0, 0.0, -20.0, 30.0),
            ..ControlData::default()
        };
        let rect = control.resolve_rect(parent);
        assert_eq!(rect.left(), 0.0);
        assert_eq!(rect.right(), 180.0);
        assert_eq!(rect.size.height, 30.0);
    }
}
