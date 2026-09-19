use draw_core::{Color, NodeId, Size, Transform2D};

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
}

/// The role a node plays in the scene tree.
///
/// `Node` is a pure grouping node with no visual state. `Node2D` is a canvas
/// item: it owns a local [`Transform2D`], a z-index and visibility.
/// `Control` is added in Stage 6.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NodeKind {
    Node,
    Node2D,
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
#[derive(Debug, Clone)]
pub struct Node {
    pub(crate) id: NodeId,
    pub(crate) name: String,
    pub(crate) kind: NodeKind,
    pub(crate) parent: Option<NodeId>,
    pub(crate) children: Vec<NodeId>,
    /// Monotonic creation sequence, used as a stable tie-breaker for z-ordering.
    pub(crate) order: u64,
    pub(crate) canvas: Option<CanvasItem>,
}

impl Node {
    pub(crate) fn new(id: NodeId, name: impl Into<String>, kind: NodeKind, order: u64) -> Self {
        let canvas = matches!(kind, NodeKind::Node2D).then(CanvasItem::new);
        Self {
            id,
            name: name.into(),
            kind,
            parent: None,
            children: Vec::new(),
            order,
            canvas,
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
}
