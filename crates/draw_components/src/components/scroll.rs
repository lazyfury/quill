//! A generic scrollable viewport: clip + offset + a draggable scrollbar.
//!
//! The core primitive is the same one [`List`](crate::List) uses — a control
//! that clips its subtree ([`ControlData::clip`](draw_ui::ControlData)) — but
//! where `List` pools row nodes, `ScrollView` puts an arbitrary child in a
//! translated, clipped viewport and adds a scrollbar. The host drives the
//! offset with [`ScrollViewState::sync`] after layout, exactly like
//! [`ListState::sync`](crate::ListState::sync):
//!
//! ```ignore
//! let view = ScrollView::new(theme).child(long_column);
//! let state = view.state();
//! tree.add_child(pane, view);
//!
//! draw_ui::layout(&mut tree, viewport);
//! if state.sync(&mut tree) {
//!     draw_ui::layout(&mut tree, viewport);   // the offset moved the content
//! }
//! ```
//!
//! The viewport clips, so only what fits is painted, and the wheel is routed to
//! it (`set_on_scroll`); the right-hand thumb can be dragged. A content shorter
//! than the viewport does not scroll (and the scrollbar is hidden), so a
//! `ScrollView` costs nothing until it overflows.

use std::cell::RefCell;
use std::rc::Rc;

use draw_core::{Color, Cursor, Edges, NodeId, Vec2};
use draw_scene::SceneTree;
use draw_theme::{radius, Theme};
use draw_ui::{Control, DragPhase, MouseFilter, SurfaceStyle, Widget};

use crate::base::{apply_spec, set_on_drag, set_on_scroll, update_control, Component, Spec};
use crate::Panel;

/// Width of the scrollbar gutter in logical pixels.
pub const SCROLLBAR_WIDTH: f32 = 8.0;
/// Inset of the scrollbar from the viewport edges.
pub const SCROLLBAR_MARGIN: f32 = 2.0;
/// Smallest thumb height, so the thumb stays grabbable for very long content.
pub const MIN_THUMB: f32 = 24.0;

/// The mounted state of a [`ScrollView`], shared with the application.
///
/// Cloning is cheap and shares one state, so a view keeps a handle and drives
/// the viewport while the [`ScrollView`] component itself is mounted.
#[derive(Clone)]
pub struct ScrollViewState {
    inner: Rc<RefCell<ScrollInner>>,
}

struct ScrollInner {
    container: Option<NodeId>,
    content: Option<NodeId>,
    track: Option<NodeId>,
    thumb: Option<NodeId>,
    scrollbar: bool,
    offset: f32,
    max_offset: f32,
    viewport_height: f32,
    /// Frozen preferred height of the content (see [`ScrollViewState::invalidate`]).
    content_height: f32,
    /// Pointer pixels → scroll pixels while dragging the thumb.
    drag_scale: f32,
    /// When set, the viewport height shrinks to `min(content, max_height)`.
    max_height: Option<f32>,
}

impl ScrollViewState {
    pub fn new() -> Self {
        Self {
            inner: Rc::new(RefCell::new(ScrollInner {
                container: None,
                content: None,
                track: None,
                thumb: None,
                scrollbar: true,
                offset: 0.0,
                max_offset: 0.0,
                viewport_height: 0.0,
                content_height: 0.0,
                drag_scale: 0.0,
                max_height: None,
            })),
        }
    }

    /// Reconciles the offset and the scrollbar with the resolved viewport and
    /// content, reporting whether the tree changed (so the host lays out again
    /// before painting).
    ///
    /// Call it *after* layout — the viewport and content rectangles must be
    /// resolved. A hidden viewport (zero height) returns `false` without
    /// touching the tree.
    pub fn sync(&mut self, tree: &mut SceneTree) -> bool {
        let mut inner = self.inner.borrow_mut();
        let Some(container) = inner.container else {
            return false;
        };
        let Some(viewport) = draw_ui::control(tree, container).map(|data| data.rect) else {
            return false;
        };
        if viewport.size.height <= 0.0 {
            return false;
        }
        inner.viewport_height = viewport.size.height;

        let Some(content) = inner.content else {
            return false;
        };
        // On the first sync (and after `invalidate`) the content rect carries
        // its preferred height; once known we pin it, so the offset translation
        // cannot feed back into the measured size.
        if inner.content_height <= 0.0 {
            let Some(rect) = draw_ui::control(tree, content).map(|data| data.rect) else {
                return false;
            };
            inner.content_height = rect.size.height;
        }
        let content_height = inner.content_height;
        inner.max_offset = (content_height - inner.viewport_height).max(0.0);
        inner.offset = inner.offset.clamp(0.0, inner.max_offset);

        let mut changed = false;

        // With a max height, the viewport shrinks to the content (never taller
        // than the cap), so a short menu has no empty area and a tall one
        // scrolls.
        if let Some(max) = inner.max_height {
            let target = content_height.min(max);
            let current = draw_ui::control(tree, container)
                .map(|data| data.min_size.height)
                .unwrap_or(0.0);
            if (current - target).abs() > 0.5 {
                update_control(tree, container, |data| data.min_size.height = target);
                changed = true;
            }
        }

        let track_h = (inner.viewport_height - 2.0 * SCROLLBAR_MARGIN).max(0.0);
        let show_bar = inner.scrollbar && inner.max_offset > 0.0 && track_h > 0.0;
        let reserve = if show_bar { SCROLLBAR_WIDTH } else { 0.0 };

        let want = Edges::new(0.0, -inner.offset, -reserve, content_height - inner.offset);
        if draw_ui::control(tree, content).is_some_and(|data| data.offsets != want) {
            update_control(tree, content, |data| data.offsets = want);
            changed = true;
        }

        for node in [inner.track, inner.thumb].into_iter().flatten() {
            if tree.is_visible(node) != Some(show_bar) {
                tree.set_visible(node, show_bar);
                draw_ui::mark_dirty(tree, node);
                changed = true;
            }
        }
        if !show_bar {
            inner.drag_scale = 0.0;
            return changed;
        }

        let thumb_h = (inner.viewport_height * inner.viewport_height / content_height)
            .max(MIN_THUMB)
            .min(track_h);
        let travel = (track_h - thumb_h).max(0.0);
        inner.drag_scale = if travel > 0.0 {
            inner.max_offset / travel
        } else {
            0.0
        };
        let thumb_y = if inner.max_offset > 0.0 {
            inner.offset / inner.max_offset * travel
        } else {
            0.0
        };
        if let Some(thumb) = inner.thumb {
            let want = Edges::new(0.0, thumb_y, 0.0, thumb_y + thumb_h);
            if draw_ui::control(tree, thumb).is_some_and(|data| data.offsets != want) {
                update_control(tree, thumb, |data| data.offsets = want);
                changed = true;
            }
        }
        changed
    }

    /// Scrolls by `delta` logical pixels (positive scrolls down). Pure state;
    /// [`sync`](ScrollViewState::sync) applies it.
    pub fn scroll_by(&self, delta: f32) {
        self.scroll_to(self.offset() + delta);
    }

    /// Scrolls to an absolute offset, clamped to `0..=max_offset`.
    pub fn scroll_to(&self, offset: f32) {
        let mut inner = self.inner.borrow_mut();
        inner.offset = offset.clamp(0.0, inner.max_offset);
    }

    pub fn scroll_to_top(&self) {
        self.scroll_to(0.0);
    }

    /// Applies a thumb drag of `delta` pointer pixels (scaled to the scroll
    /// range).
    pub fn drag_by(&self, delta: f32) {
        let scale = self.inner.borrow().drag_scale;
        self.scroll_by(delta * scale);
    }

    /// Re-measures the content on the next frame (call after its height can
    /// change, e.g. text reflow or a different active view).
    pub fn invalidate(&mut self, tree: &mut SceneTree) {
        let mut inner = self.inner.borrow_mut();
        inner.content_height = 0.0;
        inner.offset = 0.0;
        if let Some(content) = inner.content {
            update_control(tree, content, |data| data.offsets = Edges::ZERO);
        }
    }

    /// Current scroll offset in logical pixels.
    pub fn offset(&self) -> f32 {
        self.inner.borrow().offset
    }

    /// Largest offset (`content_height - viewport_height`, never negative).
    pub fn max_offset(&self) -> f32 {
        self.inner.borrow().max_offset
    }

    /// Resolved viewport height, or `0` before the first layout.
    pub fn viewport_height(&self) -> f32 {
        self.inner.borrow().viewport_height
    }

    /// Measured content height, or `0` before the first sync.
    pub fn content_height(&self) -> f32 {
        self.inner.borrow().content_height
    }

    /// Scroll progress in `0.0..=1.0` (0 when there is nothing to scroll).
    pub fn fraction(&self) -> f32 {
        let inner = self.inner.borrow();
        if inner.max_offset <= 0.0 {
            0.0
        } else {
            inner.offset / inner.max_offset
        }
    }
}

/// A clipped viewport with an offset child and a draggable scrollbar.
///
/// Build it like any component (`.child(content)`); the content keeps its
/// natural height and is translated by the scroll offset. The scrollbar is on
/// by default — [`ScrollView::scrollbar`] turns it off for a pure clip.
pub struct ScrollView {
    spec: Spec,
    theme: &'static dyn Theme,
    state: ScrollViewState,
    scrollbar: bool,
    max_height: Option<f32>,
}

impl ScrollView {
    pub fn new(theme: &'static dyn Theme) -> Self {
        Self::with_state(theme, ScrollViewState::new())
    }

    /// Builds a viewport that drives a pre-existing state (so a rebuild reuses
    /// the same offset; used by the overlay layer's menus).
    pub fn with_state(theme: &'static dyn Theme, state: ScrollViewState) -> Self {
        Self {
            spec: Spec::default(),
            theme,
            state,
            scrollbar: true,
            max_height: None,
        }
    }

    /// Shows or hides the draggable scrollbar (content still clips and the
    /// wheel still scrolls).
    pub fn scrollbar(mut self, scrollbar: bool) -> Self {
        self.scrollbar = scrollbar;
        self
    }

    /// Caps the viewport height: it shrinks to the content when shorter, and
    /// scrolls when taller. Without this the viewport takes its parent's height.
    pub fn max_height(mut self, max_height: f32) -> Self {
        self.max_height = Some(max_height);
        self
    }

    /// The shared state handle the application drives (`sync`, `scroll_to`, …).
    ///
    /// Take it before mounting — the component consumes itself in `build`.
    pub fn state(&self) -> ScrollViewState {
        self.state.clone()
    }
}

impl Component for ScrollView {
    fn spec(&mut self) -> &mut Spec {
        &mut self.spec
    }

    fn name(&self) -> &'static str {
        "ScrollView"
    }

    fn widget(&self) -> Widget {
        Widget::Panel {
            color: Color::TRANSPARENT,
            border: None,
        }
    }

    fn prepare(&mut self) {
        // Its rectangle is the clip the content is drawn inside, and what the
        // wheel is routed to (`set_on_scroll` in `build`).
        self.spec.data.clip = true;
        self.spec.data.mouse_filter = MouseFilter::Stop;
        // Start at the cap so the first layout has a viewport; `sync` then
        // shrinks it to the content when shorter. Anchors are cleared so the
        // parent sizes to the capped viewport instead of the viewport filling.
        if let Some(max) = self.max_height {
            self.spec.data.anchors = Edges::ZERO;
            self.spec.data.min_size.height = self.spec.data.min_size.height.max(max);
        }
    }

    fn build(mut self, tree: &mut SceneTree, parent: NodeId) -> NodeId {
        self.prepare();
        let spec = std::mem::take(self.spec());
        let root = tree.add_control(parent, self.name());
        tree.set_data(root, Control::new(spec.data, self.widget()));
        apply_spec(tree, root, spec);

        // The content is the first (only) child; it keeps its preferred height
        // and spans the viewport width, and `sync` translates it.
        let content = tree
            .children(root)
            .and_then(|children| children.first().copied());
        if let Some(content) = content {
            update_control(tree, content, |data| {
                data.anchors = Edges::new(0.0, 0.0, 1.0, 0.0);
                data.offsets = Edges::ZERO;
            });
        }

        let theme = self.theme;
        let track = tree.add_child(
            root,
            Panel::new()
                .color(Color::TRANSPARENT)
                .flat()
                .surface(SurfaceStyle::new(theme.palette().border).radius(radius::FULL))
                .anchors(Edges::new(1.0, 0.0, 1.0, 1.0))
                .offsets(Edges::new(
                    -SCROLLBAR_WIDTH,
                    SCROLLBAR_MARGIN,
                    -SCROLLBAR_MARGIN,
                    -SCROLLBAR_MARGIN,
                )),
        );
        let thumb = tree.add_child(
            track,
            Panel::new()
                .color(Color::TRANSPARENT)
                .flat()
                .surface(SurfaceStyle::new(theme.palette().muted).radius(radius::FULL))
                .anchors(Edges::new(0.0, 0.0, 1.0, 0.0))
                .cursor(Cursor::RowResize),
        );
        tree.set_visible(track, false);
        tree.set_visible(thumb, false);

        {
            let mut inner = self.state.inner.borrow_mut();
            inner.container = Some(root);
            inner.content = content;
            inner.track = Some(track);
            inner.thumb = Some(thumb);
            inner.scrollbar = self.scrollbar;
            inner.max_height = self.max_height;
        }

        let scrolling = self.state.clone();
        set_on_scroll(tree, root, move |delta: Vec2| scrolling.scroll_by(delta.y));
        let dragging = self.state.clone();
        set_on_drag(tree, thumb, move |_tree, phase, delta| {
            if phase == DragPhase::Move {
                dragging.drag_by(delta.y);
            }
        });
        root
    }
}

crate::impl_scene_child!(ScrollView);

#[cfg(test)]
mod tests {
    use super::*;
    use draw_core::{InputEvent, PointerButton, Size, ViewportSize};
    use draw_theme::{default_theme, Mode};
    use draw_ui::{handle_input, layout};

    use crate::{Column, Panel};

    const WIDTH: f32 = 300.0;
    const HEIGHT: f32 = 200.0;

    fn viewport() -> ViewportSize {
        ViewportSize::new(Size::new(WIDTH, HEIGHT))
    }

    struct Fixture {
        tree: SceneTree,
        state: ScrollViewState,
        content: NodeId,
        thumb: NodeId,
    }

    /// A mounted viewport whose content is `content_height` tall.
    fn fixture(content_height: f32) -> Fixture {
        let theme = default_theme(Mode::Dark);
        let view = ScrollView::new(theme).child(
            Column::new().gap(0.0).child(
                Panel::new()
                    .color(Color::BLACK)
                    .flat()
                    .min_size(0.0, content_height),
            ),
        );
        let mut state = view.state();
        let mut tree = SceneTree::new();
        let view_node = tree.add_child(tree.root(), view);
        let children = tree.children(view_node).unwrap();
        let content = children[0];
        // Track + thumb are the last two: root children are [content, track].
        let track = children[1];
        let thumb = tree.children(track).unwrap()[0];

        layout(&mut tree, viewport());
        if state.sync(&mut tree) {
            layout(&mut tree, viewport());
        }
        Fixture {
            tree,
            state,
            content,
            thumb,
        }
    }

    fn relayout(fixture: &mut Fixture) {
        if fixture.state.sync(&mut fixture.tree) {
            layout(&mut fixture.tree, viewport());
        }
    }

    fn wheel(fixture: &mut Fixture, delta_y: f32) {
        handle_input(
            &mut fixture.tree,
            &InputEvent::Wheel {
                position: Vec2::new(WIDTH / 2.0, HEIGHT / 2.0),
                delta: Vec2::new(0.0, delta_y),
            },
        );
        relayout(fixture);
    }

    fn rect(fixture: &Fixture, id: NodeId) -> draw_core::Rect {
        draw_ui::control(&fixture.tree, id).unwrap().rect
    }

    #[test]
    fn content_that_fits_does_not_scroll() {
        let mut fixture = fixture(50.0);
        assert_eq!(fixture.state.max_offset(), 0.0);
        assert_eq!(fixture.tree.is_visible(fixture.thumb), Some(false));

        wheel(&mut fixture, 80.0);
        assert_eq!(fixture.state.offset(), 0.0);
        assert_eq!(rect(&fixture, fixture.content).top(), 0.0);
    }

    #[test]
    fn the_wheel_scrolls_and_clamps() {
        let mut fixture = fixture(1000.0);
        assert_eq!(fixture.state.content_height(), 1000.0);
        assert_eq!(fixture.state.max_offset(), 800.0);

        wheel(&mut fixture, 100.0);
        assert!((fixture.state.offset() - 100.0).abs() < 1e-3);

        wheel(&mut fixture, 10_000.0);
        assert_eq!(fixture.state.offset(), 800.0);
        assert_eq!(fixture.state.fraction(), 1.0);

        wheel(&mut fixture, -10_000.0);
        assert_eq!(fixture.state.offset(), 0.0);
    }

    #[test]
    fn scrolling_translates_the_content() {
        let mut fixture = fixture(1000.0);
        fixture.state.scroll_to(120.0);
        relayout(&mut fixture);

        assert!((fixture.state.offset() - 120.0).abs() < 1e-3);
        assert!((rect(&fixture, fixture.content).top() - (-120.0)).abs() < 1e-3);
        // The content keeps its natural height (not the viewport's).
        assert_eq!(rect(&fixture, fixture.content).size.height, 1000.0);
    }

    #[test]
    fn the_thumb_tracks_the_offset() {
        let mut fixture = fixture(1000.0);
        let start = rect(&fixture, fixture.thumb);
        assert_eq!(fixture.tree.is_visible(fixture.thumb), Some(true));
        // viewport^2 / content = 200*200/1000 = 40.
        assert_eq!(start.size.height, 40.0);

        fixture.state.scroll_to(400.0);
        relayout(&mut fixture);

        let moved = rect(&fixture, fixture.thumb);
        assert!(moved.top() > start.top(), "the thumb moved down");
        assert_eq!(moved.size.height, 40.0);
    }

    #[test]
    fn dragging_the_thumb_scrolls() {
        let mut fixture = fixture(1000.0);
        let thumb = rect(&fixture, fixture.thumb);
        let center = thumb.center();

        handle_input(
            &mut fixture.tree,
            &InputEvent::PointerDown {
                position: center,
                button: PointerButton::Left,
            },
        );
        handle_input(
            &mut fixture.tree,
            &InputEvent::PointerMove {
                position: Vec2::new(center.x, center.y + 20.0),
            },
        );
        handle_input(
            &mut fixture.tree,
            &InputEvent::PointerUp {
                position: Vec2::new(center.x, center.y + 20.0),
                button: PointerButton::Left,
            },
        );
        relayout(&mut fixture);

        // track = 200 - 2*2 = 196; thumb = 200*200/1000 = 40; travel = 156;
        // scale = 800/156; 20px -> ~102.6 scroll pixels.
        let expected = 800.0 / 156.0 * 20.0;
        assert!(
            (fixture.state.offset() - expected).abs() < 1e-2,
            "offset = {}",
            fixture.state.offset()
        );
    }

    #[test]
    fn invalidate_re_measures_and_resets() {
        let mut fixture = fixture(1000.0);
        fixture.state.scroll_to(300.0);
        relayout(&mut fixture);
        assert!(fixture.state.offset() > 0.0);

        fixture.state.invalidate(&mut fixture.tree);
        layout(&mut fixture.tree, viewport());
        fixture.state.sync(&mut fixture.tree);
        assert_eq!(fixture.state.offset(), 0.0);
        assert_eq!(fixture.state.content_height(), 1000.0);
        assert_eq!(fixture.state.max_offset(), 800.0);
    }

    /// Mount a `max_height` viewport inside a column and return its node.
    fn capped(content_height: f32, max_height: f32) -> (SceneTree, ScrollViewState, NodeId) {
        let theme = default_theme(Mode::Dark);
        let view = ScrollView::new(theme).max_height(max_height).child(
            Column::new().gap(0.0).child(
                Panel::new()
                    .color(Color::BLACK)
                    .flat()
                    .min_size(0.0, content_height),
            ),
        );
        let state = view.state();
        let mut tree = SceneTree::new();
        let column = tree.add_child(tree.root(), Column::new().gap(0.0).child(view));
        let node = tree.children(column).unwrap()[0];
        layout(&mut tree, viewport());
        for _ in 0..4 {
            if state.clone().sync(&mut tree) {
                layout(&mut tree, viewport());
            }
        }
        (tree, state, node)
    }

    #[test]
    fn max_height_caps_tall_content_and_scrolls() {
        let (tree, state, node) = capped(1000.0, 80.0);
        let h = draw_ui::control(&tree, node).unwrap().rect.size.height;
        assert!((h - 80.0).abs() < 1.5, "capped at 80, got {h}");
        assert!(state.max_offset() > 0.0, "tall content scrolls");
    }

    #[test]
    fn max_height_shrinks_to_short_content() {
        let (tree, state, node) = capped(30.0, 80.0);
        let h = draw_ui::control(&tree, node).unwrap().rect.size.height;
        assert!((h - 30.0).abs() < 1.5, "shrinks to 30, got {h}");
        assert_eq!(state.max_offset(), 0.0);
    }
}
