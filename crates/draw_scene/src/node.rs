use std::any::{Any, TypeId};

use draw_core::{Color, EventResult, InputEvent, NodeId, Rect, Size, Transform2D, Vec2};
use draw_render::TextureId;

use crate::viewport::Viewport;

/// A node's type-keyed extension store.
///
/// Each `'static` type holds one value, and different types coexist: a `Control`
/// runtime bundle and a game component can live on the same node. Backed by a
/// small `Vec` (normally 0-3 entries) with a linear type search, which avoids a
/// `HashMap` allocation per node.
#[derive(Default)]
pub(crate) struct Extensions {
    entries: Vec<(TypeId, Box<dyn Any>)>,
}

impl Extensions {
    pub(crate) fn set<T: 'static>(&mut self, value: T) {
        let id = TypeId::of::<T>();
        match self.entries.iter_mut().find(|(key, _)| *key == id) {
            Some((_, slot)) => *slot = Box::new(value),
            None => self.entries.push((id, Box::new(value))),
        }
    }

    pub(crate) fn get<T: 'static>(&self) -> Option<&T> {
        let id = TypeId::of::<T>();
        self.entries
            .iter()
            .find(|(key, _)| *key == id)
            .and_then(|(_, value)| value.downcast_ref::<T>())
    }

    pub(crate) fn get_mut<T: 'static>(&mut self) -> Option<&mut T> {
        let id = TypeId::of::<T>();
        self.entries
            .iter_mut()
            .find(|(key, _)| *key == id)
            .and_then(|(_, value)| value.downcast_mut::<T>())
    }

    pub(crate) fn has<T: 'static>(&self) -> bool {
        self.get::<T>().is_some()
    }

    pub(crate) fn take<T: 'static>(&mut self) -> Option<T> {
        let id = TypeId::of::<T>();
        let index = self.entries.iter().position(|(key, _)| *key == id)?;
        let (_, value) = self.entries.remove(index);
        value.downcast::<T>().ok().map(|boxed| *boxed)
    }

    pub(crate) fn clear(&mut self) {
        self.entries.clear();
    }

    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }
}

/// A minimal built-in visual for canvas items.
///
/// This is a temporary primitive that lets `Node2D` participate in the
/// `Scene -> DrawList` pipeline before custom drawing (`Control`, Stage 6) and
/// richer node types exist. The default is [`Visual::None`].
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Visual {
    #[default]
    None,
    Rect {
        size: Size,
        color: Color,
    },
    Circle {
        radius: f32,
        color: Color,
    },
    /// A textured quad drawn from the local origin with `size`, sampled from
    /// `texture`.
    ///
    /// The scene only keeps the backend-neutral [`TextureId`] handle; the
    /// backend maps it to its own resource (a GPU texture, an `ImageBitmap`,
    /// or a plain recording). This is what lets a `Node2D` act as an image /
    /// sprite canvas item (`Sprite2D` grows from here later).
    Image {
        texture: TextureId,
        size: Size,
    },
    /// A textured sprite drawn from the local origin with `size` (like
    /// [`Visual::Image`]), plus atlas / flip / nine-slice controls.
    ///
    /// `source` is the sub-rectangle of the texture to sample (an atlas frame;
    /// `None` = the whole texture). `flip_x` / `flip_y` mirror the sprite.
    /// `nine` is a `[left, top, right, bottom]` nine-slice inset in source
    /// pixels and requires `source`; it stretches the edges/center to `size`.
    Sprite {
        texture: TextureId,
        size: Size,
        source: Option<Rect>,
        flip_x: bool,
        flip_y: bool,
        nine: Option<[f32; 4]>,
    },
}

/// The role a node plays in the scene tree.
///
/// `Node` is a pure grouping node with no visual state. `Node2D` is a canvas
/// item: it owns a local [`Transform2D`], a z-index and visibility.
/// `CanvasLayer` opens a new canvas transform context (it is *not* a canvas
/// item); `Camera2D` is a canvas item that writes the viewport camera.
/// `Control` is a UI canvas item whose layout and painting are owned by
/// `draw_ui`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NodeKind {
    Node,
    Node2D,
    /// A UI control. Like `Node2D` it is a canvas item, but its layout and
    /// painting are owned by `draw_ui`.
    Control,
    /// A grouping node that opens a new canvas transform context. Children
    /// belonging to no nested `CanvasLayer` are painted with this layer's
    /// transform instead of the viewport camera. Not a canvas item.
    CanvasLayer,
    /// A 2D camera. A canvas item (it has a transform), but invisible; its
    /// transform drives the viewport's `canvas_transform` when current.
    Camera2D,
    /// The root render context. Owns the logical size and the world -> screen
    /// `canvas_transform`. Not a canvas item. Only the tree root uses it today
    /// (Godot `RootViewport`); `SubViewport` is out of scope.
    Viewport,
}

/// How a [`Camera2DData`] maps the camera position onto the viewport.
///
/// Mirrors Godot `Camera2D::AnchorMode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AnchorMode {
    /// The camera's position is the viewport's top-left corner.
    FixedTopLeft,
    /// The camera's position is the viewport's center (Godot default).
    #[default]
    DragCenter,
}

/// Per-node data owned by a [`NodeKind::CanvasLayer`].
///
/// Mirrors Godot's `CanvasLayer`: `layer` selects paint order (default `1`,
/// the default world canvas being `0`), `transform` is the layer's own canvas
/// transform, and `follow_viewport` composes the camera transform before it
/// (Godot `CanvasLayer::get_final_transform`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CanvasLayerData {
    pub layer: i32,
    pub transform: Transform2D,
    pub follow_viewport: bool,
}

impl Default for CanvasLayerData {
    fn default() -> Self {
        Self {
            layer: 1,
            transform: Transform2D::IDENTITY,
            follow_viewport: false,
        }
    }
}

/// Per-node data owned by a [`NodeKind::Camera2D`].
///
/// Phase 2 keeps the transform inputs Godot uses for
/// `Camera2D::get_camera_transform`: `current`, `zoom`, `offset` and
/// `anchor_mode`. Limits, drag margins and smoothing arrive in a later phase.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Camera2DData {
    pub enabled: bool,
    pub current: bool,
    pub zoom: Vec2,
    pub offset: Vec2,
    pub anchor_mode: AnchorMode,
}

impl Default for Camera2DData {
    fn default() -> Self {
        Self {
            enabled: true,
            current: false,
            zoom: Vec2::ONE,
            offset: Vec2::ZERO,
            anchor_mode: AnchorMode::DragCenter,
        }
    }
}

/// Which derived values are stale and must be recomputed on the next update.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DirtyFlags {
    pub transform: bool,
    pub visibility: bool,
}

impl DirtyFlags {
    pub const DIRTY: Self = Self {
        transform: true,
        visibility: true,
    };

    pub const fn clean() -> Self {
        Self {
            transform: false,
            visibility: false,
        }
    }

    pub const fn is_clean(self) -> bool {
        !self.transform && !self.visibility
    }
}

/// Visual state carried by canvas items (currently `Node2D`).
///
/// `transform` is local to the parent; `world_transform` and `world_visible`
/// are derived during [`crate::SceneTree::update`] and should not be set
/// directly through the public API.
#[derive(Debug, Clone, Copy)]
pub struct CanvasItem {
    pub(crate) visible: bool,
    pub(crate) z_index: i32,
    pub(crate) transform: Transform2D,
    pub(crate) world_transform: Transform2D,
    pub(crate) world_visible: bool,
    pub(crate) visual: Visual,
    pub(crate) dirty: DirtyFlags,
}

impl Default for CanvasItem {
    fn default() -> Self {
        Self::new()
    }
}

impl CanvasItem {
    pub fn new() -> Self {
        Self {
            visible: true,
            z_index: 0,
            transform: Transform2D::IDENTITY,
            world_transform: Transform2D::IDENTITY,
            world_visible: true,
            visual: Visual::None,
            dirty: DirtyFlags::DIRTY,
        }
    }

    pub fn visible(&self) -> bool {
        self.visible
    }

    pub fn z_index(&self) -> i32 {
        self.z_index
    }

    /// Local transform relative to the parent node.
    pub fn transform(&self) -> Transform2D {
        self.transform
    }

    /// Transform relative to the scene root, valid after
    /// [`crate::SceneTree::update`].
    pub fn world_transform(&self) -> Transform2D {
        self.world_transform
    }

    /// Effective visibility including all ancestors, valid after
    /// [`crate::SceneTree::update`].
    pub fn world_visible(&self) -> bool {
        self.world_visible
    }

    pub fn dirty_flags(&self) -> DirtyFlags {
        self.dirty
    }

    pub fn visual(&self) -> Visual {
        self.visual
    }
}

/// A single node in the [`crate::SceneTree`].
///
/// `Node` is not `Clone`: it holds a type-keyed extension store of arbitrary
/// `'static` data via [`Node::set_data`]. Clone the tree only if you never need
/// the store, which is not the case today.
pub struct Node {
    pub(crate) id: NodeId,
    pub(crate) name: String,
    pub(crate) kind: NodeKind,
    pub(crate) parent: Option<NodeId>,
    pub(crate) children: Vec<NodeId>,
    /// Monotonic creation sequence, used as a stable tie-breaker for z-ordering.
    pub(crate) order: u64,
    pub(crate) canvas: Option<CanvasItem>,
    pub(crate) canvas_layer: Option<CanvasLayerData>,
    pub(crate) camera_2d: Option<Camera2DData>,
    pub(crate) viewport: Option<Viewport>,
    /// Backend-neutral, type-keyed extension store for engine/UI data
    /// (`Control`, game components, …). `draw_scene` never names the types.
    pub(crate) data: Extensions,
    /// Per-frame lifecycle callback, dispatched by [`crate::SceneTree::process`].
    pub(crate) process: Option<Box<dyn FnMut(f32)>>,
    /// Capture-phase input callback (Godot `Node::_input`).
    pub(crate) input: Option<Box<dyn FnMut(&InputEvent) -> EventResult>>,
    /// World-pick input callback (Godot `Node2D`/`CanvasItem::_input_event`).
    pub(crate) input_event: Option<Box<dyn FnMut(&InputEvent) -> EventResult>>,
    /// Unhandled-input callback (Godot `Node::_unhandled_input`).
    pub(crate) unhandled_input: Option<Box<dyn FnMut(&InputEvent) -> EventResult>>,
}

impl std::fmt::Debug for Node {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Node")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("kind", &self.kind)
            .field("parent", &self.parent)
            .field("children", &self.children)
            .field("order", &self.order)
            .field("canvas", &self.canvas)
            .field("canvas_layer", &self.canvas_layer)
            .field("camera_2d", &self.camera_2d)
            .field("viewport", &self.viewport)
            .field("data_count", &self.data.len())
            .field("has_process", &self.process.is_some())
            .field("has_input", &self.input.is_some())
            .field("has_input_event", &self.input_event.is_some())
            .field("has_unhandled_input", &self.unhandled_input.is_some())
            .finish()
    }
}

impl Node {
    pub(crate) fn new(id: NodeId, name: impl Into<String>, kind: NodeKind, order: u64) -> Self {
        let canvas = matches!(
            kind,
            NodeKind::Node2D | NodeKind::Control | NodeKind::Camera2D
        )
        .then(CanvasItem::new);
        let canvas_layer = (kind == NodeKind::CanvasLayer).then(CanvasLayerData::default);
        let camera_2d = (kind == NodeKind::Camera2D).then(Camera2DData::default);
        let viewport = (kind == NodeKind::Viewport).then(Viewport::default);
        Self {
            id,
            name: name.into(),
            kind,
            parent: None,
            children: Vec::new(),
            order,
            canvas,
            canvas_layer,
            camera_2d,
            viewport,
            data: Extensions::default(),
            process: None,
            input: None,
            input_event: None,
            unhandled_input: None,
        }
    }

    pub fn id(&self) -> NodeId {
        self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn kind(&self) -> NodeKind {
        self.kind
    }

    pub fn parent(&self) -> Option<NodeId> {
        self.parent
    }

    pub fn children(&self) -> &[NodeId] {
        &self.children
    }

    pub fn order(&self) -> u64 {
        self.order
    }

    pub fn is_canvas_item(&self) -> bool {
        self.canvas.is_some()
    }

    pub fn canvas(&self) -> Option<&CanvasItem> {
        self.canvas.as_ref()
    }

    /// Local visibility. Grouping nodes without a canvas are always visible.
    pub fn is_visible(&self) -> bool {
        self.canvas.as_ref().map_or(true, |c| c.visible)
    }

    pub fn z_index(&self) -> i32 {
        self.canvas.as_ref().map_or(0, |c| c.z_index)
    }

    pub fn local_transform(&self) -> Option<Transform2D> {
        self.canvas.as_ref().map(CanvasItem::transform)
    }

    pub fn world_transform(&self) -> Option<Transform2D> {
        self.canvas.as_ref().map(CanvasItem::world_transform)
    }

    pub fn visual(&self) -> Visual {
        self.canvas
            .as_ref()
            .map_or(Visual::None, CanvasItem::visual)
    }

    /// Effective visibility including ancestors (valid after update).
    pub fn world_visible(&self) -> bool {
        self.canvas.as_ref().map_or(true, |c| c.world_visible)
    }

    /// Layer data for a [`NodeKind::CanvasLayer`] node; `None` otherwise.
    pub fn canvas_layer(&self) -> Option<&CanvasLayerData> {
        self.canvas_layer.as_ref()
    }

    /// Data for a [`NodeKind::Camera2D`] node; `None` otherwise.
    pub fn camera_2d(&self) -> Option<&Camera2DData> {
        self.camera_2d.as_ref()
    }

    /// Render context for a [`NodeKind::Viewport`] node; `None` otherwise.
    pub fn viewport(&self) -> Option<&Viewport> {
        self.viewport.as_ref()
    }

    // -- generic extension slot --------------------------------------------

    /// Stores a value in the node's type-keyed extension slot, replacing any
    /// previous value *of the same type* (other types are untouched).
    /// Downcast with [`Node::data`].
    pub fn set_data<T: 'static>(&mut self, value: T) {
        self.data.set(value);
    }

    /// Borrows the stored value if it has type `T`.
    pub fn data<T: 'static>(&self) -> Option<&T> {
        self.data.get::<T>()
    }

    /// Mutably borrows the stored value if it has type `T`.
    pub fn data_mut<T: 'static>(&mut self) -> Option<&mut T> {
        self.data.get_mut::<T>()
    }

    /// Returns `true` if a value of type `T` is stored.
    pub fn has_data<T: 'static>(&self) -> bool {
        self.data.has::<T>()
    }

    /// Removes and returns the stored value if it has type `T`.
    pub fn take_data<T: 'static>(&mut self) -> Option<T> {
        self.data.take::<T>()
    }

    /// Removes every stored value, of every type.
    pub fn clear_data(&mut self) {
        self.data.clear();
    }

    /// Whether the node has a lifecycle callback.
    pub fn has_process(&self) -> bool {
        self.process.is_some()
    }

    /// Whether the node has a capture-phase input callback.
    pub fn has_input(&self) -> bool {
        self.input.is_some()
    }

    /// Whether the node has a world-pick input callback.
    pub fn has_input_event(&self) -> bool {
        self.input_event.is_some()
    }

    /// Whether the node has an unhandled-input callback.
    pub fn has_unhandled_input(&self) -> bool {
        self.unhandled_input.is_some()
    }
}
