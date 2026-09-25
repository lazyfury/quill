//! UI `DrawList` caching for idle frames (Stage 27).
//!
//! The UI is retained, so its painted output changes only when the scene does.
//! [`UiPaintCache`] keeps the last UI [`DrawList`] and lets a host splice it into
//! a fresh frame with [`paint_cached`] instead of re-running the paint walk
//! every frame. This is what keeps a game's frame cost from paying for an
//! unchanged UI (the refresh-decoupling plan in `docs/godot-migration.md`).
//!
//! The cache is owned by the host, not stored on the tree, so one cache belongs
//! to one tree and the tree stays the single owner of UI state. It is keyed on
//! [`paint_generation`](crate::paint_generation), which is bumped by every
//! change that can alter painted output (`mark_dirty`, GUI interaction state,
//! decorators).

use draw_render::{DrawCommand, DrawList, PaintContext};
use draw_scene::SceneTree;

use crate::{paint, paint_generation};

/// Whether [`paint_cached`] rebuilt the UI or reused the cached list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaintStatus {
    /// The UI was unchanged; the cached commands were appended.
    Reused,
    /// The UI changed; its list was rebuilt and cached.
    Rebuilt,
}

impl PaintStatus {
    /// Whether the cached list was reused (no paint walk happened).
    pub fn is_reused(self) -> bool {
        self == PaintStatus::Reused
    }
}

/// Holds the UI's last [`DrawList`] across frames.
///
/// **Non-breaking addition to `draw_ui`** (Stage 27); recorded in
/// `docs/design-system.md`.
#[derive(Default)]
pub struct UiPaintCache {
    generation: u64,
    filled: bool,
    list: DrawList,
}

impl UiPaintCache {
    /// Creates an empty cache (nothing painted yet).
    pub fn new() -> Self {
        Self::default()
    }

    /// The `paint_generation` the cached list was built from.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Whether the cache has never been filled.
    pub fn is_empty(&self) -> bool {
        !self.filled
    }

    /// Whether the cached list is stale for `tree` (must be rebuilt).
    pub fn is_stale(&self, tree: &SceneTree) -> bool {
        !self.filled || self.generation != paint_generation(tree)
    }

    /// Drops the cached list; the next [`paint_cached`] rebuilds.
    pub fn clear(&mut self) {
        self.filled = false;
        self.list.clear();
    }

    /// The cached commands (empty before the first paint).
    pub fn commands(&self) -> &[DrawCommand] {
        self.list.commands()
    }
}

/// Appends the UI for `tree` into `ctx`, rebuilding only when it changed.
///
/// Returns whether the list was rebuilt or reused. With `ctx` empty, the result
/// is exactly what [`paint`](crate::paint) would have produced; with a populated
/// `ctx`, the UI is appended after the existing commands.
pub fn paint_cached(
    tree: &SceneTree,
    cache: &mut UiPaintCache,
    ctx: &mut PaintContext,
) -> PaintStatus {
    let generation = paint_generation(tree);
    if cache.filled && cache.generation == generation {
        ctx.extend(&cache.list);
        return PaintStatus::Reused;
    }

    let mut local = PaintContext::new();
    paint(tree, &mut local);
    cache.list = local.into_draw_list();
    cache.generation = generation;
    cache.filled = true;
    ctx.extend(&cache.list);
    PaintStatus::Rebuilt
}

#[cfg(test)]
mod tests {
    use draw_core::{Color, Edges, NodeId, Size, ViewportSize};
    use draw_render::DrawCommand;

    use crate::control::{control_mut, Control, ControlData};
    use crate::layout::TextOptions;
    use crate::widget::Widget;

    use super::*;

    fn viewport() -> ViewportSize {
        ViewportSize::new(Size::new(200.0, 100.0))
    }

    fn add_label(tree: &mut SceneTree, text: &str) -> NodeId {
        let data = ControlData {
            anchors: Edges::ZERO,
            offsets: Edges::new(0.0, 0.0, 80.0, 20.0),
            ..ControlData::default()
        };
        let widget = Widget::Label {
            text: text.to_string(),
            font_size: 12.0,
            color: Color::WHITE,
            options: TextOptions::default(),
        };
        let id = tree.add_control(tree.root(), "label");
        tree.set_data(id, Control::new(data, widget));
        crate::mark_dirty(tree, id);
        id
    }

    fn set_label_text(tree: &mut SceneTree, id: NodeId, text: &str) {
        if let Some(control) = control_mut(tree, id) {
            control.widget.set_text(text);
        }
        crate::mark_dirty(tree, id);
    }

    fn laid_out(text: &str) -> (SceneTree, NodeId) {
        let mut tree = SceneTree::new();
        let id = add_label(&mut tree, text);
        crate::layout(&mut tree, viewport());
        (tree, id)
    }

    fn has_text(list: &[DrawCommand], needle: &str) -> bool {
        list.iter().any(|command| match command {
            DrawCommand::DrawText { text, .. } => text == needle,
            _ => false,
        })
    }

    #[test]
    fn paint_cached_rebuilds_then_reuses_an_unchanged_ui() {
        let (tree, _) = laid_out("hello");
        let mut cache = UiPaintCache::new();
        assert!(cache.is_empty());

        let mut first = PaintContext::new();
        assert_eq!(
            paint_cached(&tree, &mut cache, &mut first),
            PaintStatus::Rebuilt
        );
        assert!(has_text(first.draw_list(), "hello"));
        assert!(!cache.is_stale(&tree));

        let mut second = PaintContext::new();
        assert_eq!(
            paint_cached(&tree, &mut cache, &mut second),
            PaintStatus::Reused
        );
        assert_eq!(first.draw_list(), second.draw_list());
    }

    #[test]
    fn a_text_change_invalidates_the_cache() {
        let (mut tree, id) = laid_out("hello");
        let mut cache = UiPaintCache::new();
        let mut ctx = PaintContext::new();
        paint_cached(&tree, &mut cache, &mut ctx);
        let before = cache.generation();

        set_label_text(&mut tree, id, "world");
        assert!(cache.is_stale(&tree));

        let mut ctx = PaintContext::new();
        assert_eq!(
            paint_cached(&tree, &mut cache, &mut ctx),
            PaintStatus::Rebuilt
        );
        assert_ne!(cache.generation(), before);
        assert!(has_text(ctx.draw_list(), "world"));
    }

    #[test]
    fn hover_state_change_invalidates_the_cache() {
        let (mut tree, id) = laid_out("hover me");
        let mut cache = UiPaintCache::new();
        let mut ctx = PaintContext::new();
        paint_cached(&tree, &mut cache, &mut ctx);
        assert!(!cache.is_stale(&tree));

        crate::gui_state_mut(&mut tree).hovered = Some(id);
        assert!(cache.is_stale(&tree), "interaction state is painted");
    }

    #[test]
    fn needs_layout_reports_pending_work() {
        let mut tree = SceneTree::new();
        let id = add_label(&mut tree, "x");
        assert!(crate::needs_layout(&tree), "fresh tree needs a layout");

        crate::layout(&mut tree, viewport());
        assert!(!crate::needs_layout(&tree));

        crate::mark_dirty(&mut tree, id);
        assert!(crate::needs_layout(&tree));
    }

    #[test]
    fn clearing_the_cache_forces_a_rebuild() {
        let (tree, _) = laid_out("again");
        let mut cache = UiPaintCache::new();
        let mut ctx = PaintContext::new();
        paint_cached(&tree, &mut cache, &mut ctx);
        cache.clear();
        assert!(cache.is_empty());
        assert!(cache.is_stale(&tree));
    }
}
