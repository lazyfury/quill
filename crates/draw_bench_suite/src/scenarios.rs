//! Deterministic fixtures and sizes shared by the benchmark scenarios.
//!
//! Fixtures build a fixed tree/UI, call `update`/`layout` once so the first
//! measured call starts from the same state as every later one, and expose the
//! ids a routine needs to mutate. No randomness, no I/O: two calls to the same
//! builder produce identical structure.

use std::cell::Cell;
use std::rc::Rc;

use draw_components::theme::{default_theme, space, Mode, TextSize};
use draw_components::{
    update_control, Component, Flex, Label, List, ListColumn, ListState, Panel, Row, Text, Theme,
    VBox,
};
use draw_core::{Color, Edges, NodeId, Size, Vec2, ViewportSize};
use draw_render::PaintContext;
use draw_scene::{SceneTree, Visual};
use draw_ui::{MouseFilter, SizeBasis};

/// Entity counts every scenario is run at, to expose scaling curves.
pub const SIZES: [usize; 3] = [100, 1_000, 10_000];

/// Logical viewport used by UI scenarios.
pub const VIEWPORT_SIZE: Size = Size::new(1280.0, 800.0);

/// A flat scene: `n` visible `Node2D`s laid out on a grid under the root.
///
/// `update` has already run once, so `tree.update()` starts clean — that is what
/// the `update_clean` scenario measures.
pub struct SceneFixture {
    pub tree: SceneTree,
    /// Every child node, in creation order.
    pub ids: Vec<NodeId>,
}

impl SceneFixture {
    /// Builds a grid scene with `n` visible rect nodes.
    pub fn new(n: usize) -> Self {
        let mut tree = SceneTree::new();
        let root = tree.root();
        let columns = (n as f64).sqrt().ceil().max(1.0) as usize;
        let mut ids = Vec::with_capacity(n);

        for i in 0..n {
            let id = tree.add_node2d(root, format!("item_{i}"));
            let x = (i % columns) as f32 * 4.0;
            let y = (i / columns) as f32 * 4.0;
            tree.set_position(id, Vec2::new(x, y));
            tree.set_visual(
                id,
                Visual::Rect {
                    size: Size::new(2.0, 2.0),
                    color: Color::RED,
                },
            );
            ids.push(id);
        }

        tree.update();
        Self { tree, ids }
    }

    /// Draw commands emitted by [`SceneTree::paint`] for this fixture.
    pub fn expected_commands(&self) -> usize {
        // Each visible node emits Save + SetTransform + FillRect + Restore.
        self.ids.len() * 4
    }
}

/// A UI with `n` labels inside a panel/vbox, already laid out once.
pub struct UiFixture {
    pub tree: SceneTree,
    root: NodeId,
    pub viewport: ViewportSize,
    /// The label controls, in creation order.
    pub ids: Vec<NodeId>,
    /// Center of the *first* label — the worst case for reverse hit testing.
    pub first_center: Vec2,
}

impl UiFixture {
    /// Builds a panel > vbox > `n` labels UI and lays it out.
    pub fn new(n: usize) -> Self {
        let viewport = ViewportSize::new(VIEWPORT_SIZE);
        let mut tree = SceneTree::new();
        let tree_root = tree.root();
        let root = tree.add_child(
            tree_root,
            Flex::column().mouse_filter(draw_ui::MouseFilter::Ignore),
        );
        let panel = tree.add_child(root, Panel::new());
        let vbox = tree.add_child(panel, VBox::new());

        let mut ids = Vec::with_capacity(n);
        for i in 0..n {
            ids.push(tree.add_child(vbox, Label::new(format!("Item {i}"))));
        }

        draw_ui::layout(&mut tree, viewport);
        tree.update();
        let first_center = draw_ui::control(&tree, ids[0])
            .map(|control| control.rect.center())
            .unwrap_or(Vec2::ZERO);

        Self {
            tree,
            root,
            viewport,
            ids,
            first_center,
        }
    }

    pub fn root(&self) -> NodeId {
        self.root
    }

    pub fn layout(&mut self, viewport: ViewportSize) {
        draw_ui::layout(&mut self.tree, viewport);
        self.tree.update();
    }

    pub fn hit_test(&self, position: Vec2) -> Option<NodeId> {
        draw_ui::hit_test(&self.tree, position)
    }

    pub fn paint(&self, ctx: &mut draw_render::PaintContext) {
        draw_ui::paint(&self.tree, ctx);
    }
}

// -- lists ---------------------------------------------------------------------

/// Entity counts the list scenarios run at: a folder holding a thousand, ten
/// thousand and a hundred thousand entries.
///
/// The virtualized shape is meant to be flat across all three; the naive one is
/// the control that shows what "flat" is being compared against.
pub const LIST_SIZES: [usize; 3] = [1_000, 10_000, 100_000];

/// Row height both list fixtures place their rows at, so the two shapes are
/// compared over the same geometry.
pub const LIST_ROW_HEIGHT: f32 = 24.0;

/// How far one measured list frame scrolls, in logical pixels.
///
/// Two and a half rows is what a trackpad flick delivers, and it is the worst
/// case for a virtualizer: every row in the pool moves *and* re-binds, so the
/// frame pays for the whole pool rather than for the one row that entered it.
pub const LIST_SCROLL_STEP: f32 = 2.5 * LIST_ROW_HEIGHT;

/// Reflects a scrolling step at either end, so the offset keeps moving.
///
/// A one-way scroll parks at the bottom, and that would quietly ruin the
/// comparison: the harness calls the routine tens of thousands of times per
/// benchmark, so a monotonic scroll spends most of its samples on a list that is
/// standing still — and *when* it parks depends on the row count, which would
/// end up comparing a moving list against a stopped one (and an idle frame skips
/// its whole subtree through partial relayout, so it is cheap for reasons that
/// have nothing to do with the shape). Reflecting keeps every measured frame a
/// real scrolling frame at every size.
fn bounce(offset: f32, direction: f32, delta: f32, max_offset: f32) -> (f32, f32) {
    if max_offset <= 0.0 {
        return (0.0, direction);
    }
    // Turn around *before* stepping off the end, so every step is a full step in
    // the current direction instead of a shortened one scraping the edge.
    let ahead = offset + direction * delta;
    let direction = if ahead < 0.0 || ahead > max_offset {
        -direction
    } else {
        direction
    };
    (
        (offset + direction * delta).clamp(0.0, max_offset),
        direction,
    )
}

/// What one frame is billed in: nodes to walk, commands to submit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameShape {
    /// Controls in the tree ([`draw_ui::control_count`]).
    pub controls: usize,
    /// Commands the frame emits (`DrawList::len`).
    pub commands: usize,
}

/// The cell text of data row `index` — the same content in both fixtures.
fn row_cells(index: usize) -> Vec<String> {
    vec![
        format!("entry_{index:06}.txt"),
        format!("{} KB", index % 4096),
    ]
}

/// The columns a row is laid out in: one that takes the leftover width, one
/// fixed — the shape a file listing has.
fn row_columns() -> Vec<ListColumn> {
    vec![ListColumn::flexible(), ListColumn::fixed(96.0)]
}

/// One row of the list with `cells` already bound.
///
/// Both fixtures build their rows here, so the comparison is about *how many*
/// rows are mounted and nothing else. The row carries no background: the
/// virtualizer's rows add hover/selection chrome on top of this, which makes the
/// naive baseline deliberately the cheaper of the two.
fn row_component(theme: &'static dyn Theme, cells: &[String]) -> Row {
    let mut row = Row::new()
        .gap(space::MD)
        .padding(Edges::new(space::SM, 0.0, space::SM, 0.0));
    for (index, column) in row_columns().into_iter().enumerate() {
        let text = cells.get(index).cloned().unwrap_or_default();
        let cell = Text::new(text, theme)
            .size(TextSize::Small)
            .tone(column.tone)
            .max_lines(1)
            .ellipsis(true)
            .shrink(0.0);
        row = row.child(match column.width {
            Some(width) => cell.basis(SizeBasis::Px(width)),
            None => cell.grow(1.0),
        });
    }
    row
}

/// One scrolling frame's worth of commands, for sizing the paint context.
fn list_command_hint(rows: usize) -> usize {
    rows * 3 + 32
}

/// The naive list: every row of the data mounted up front, scrolled by moving
/// the container they live in.
///
/// This is the shape the virtualized list exists to beat, and it is not a
/// strawman — the rows are ordinary flex children, the container moves by
/// absolute offsets (the cheap way to scroll), and the pane clips so a row that
/// leaves the viewport is cut off rather than overdrawn. The cost that remains
/// is structural: arranging the container arranges its whole subtree, so a frame
/// is O(rows) however little of it is on screen.
pub struct ListFullFixture {
    pub tree: SceneTree,
    viewport: ViewportSize,
    /// The pane the rows are clipped to.
    pane: NodeId,
    /// The block holding every row, moved by the scroll offset.
    content: NodeId,
    content_height: f32,
    offset: f32,
    /// `1.0` scrolling down, `-1.0` back up (see [`bounce`]).
    direction: f32,
    max_offset: f32,
    rows: usize,
}

impl ListFullFixture {
    /// Builds a panel > content > `n` rows UI, lays it out and mounts every row.
    pub fn new(n: usize) -> Self {
        let theme = default_theme(Mode::Dark);
        let viewport = ViewportSize::new(VIEWPORT_SIZE);
        let mut tree = SceneTree::new();
        let root = tree.add_child(
            tree.root(),
            Flex::column()
                .gap(0.0)
                .padding(Edges::ZERO)
                .mouse_filter(MouseFilter::Ignore),
        );
        let pane = tree.add_child(root, Panel::new().grow(1.0).clip(true));

        let mut content = Flex::column()
            .gap(0.0)
            .padding(Edges::ZERO)
            // Full width of the pane; top/bottom come from the scroll offset.
            .anchors(Edges::new(0.0, 0.0, 1.0, 0.0));
        for index in 0..n {
            content = content.child(
                row_component(theme, &row_cells(index)).basis(SizeBasis::Px(LIST_ROW_HEIGHT)),
            );
        }
        let content = tree.add_child(pane, content);

        let mut fixture = Self {
            tree,
            viewport,
            pane,
            content,
            content_height: n as f32 * LIST_ROW_HEIGHT,
            offset: 0.0,
            direction: 1.0,
            max_offset: 0.0,
            rows: n,
        };
        fixture.layout();
        fixture.max_offset = (fixture.content_height - fixture.viewport_height()).max(0.0);
        fixture
    }

    /// Rows of data this shape mounted.
    pub fn row_count(&self) -> usize {
        self.rows
    }

    /// Scroll offset in logical pixels.
    pub fn offset(&self) -> f32 {
        self.offset
    }

    /// Scrolls by `delta` logical pixels, turning around at either end.
    pub fn scroll_by(&mut self, delta: f32) {
        let (offset, direction) = bounce(self.offset, self.direction, delta, self.max_offset);
        self.offset = offset;
        self.direction = direction;
    }

    /// Places the content at the current offset and arranges the tree.
    pub fn layout(&mut self) {
        let want = Edges::new(0.0, -self.offset, 0.0, -self.offset + self.content_height);
        if draw_ui::control(&self.tree, self.content).is_some_and(|data| data.offsets != want) {
            update_control(&mut self.tree, self.content, |data| data.offsets = want);
        }
        draw_ui::layout(&mut self.tree, self.viewport);
        self.tree.update();
    }

    pub fn paint(&self, ctx: &mut PaintContext) {
        draw_ui::paint(&self.tree, ctx);
    }

    pub fn command_hint(&self) -> usize {
        list_command_hint(self.rows)
    }

    /// What one frame is billed in.
    pub fn shape(&self) -> FrameShape {
        let mut ctx = PaintContext::with_capacity(self.command_hint());
        self.paint(&mut ctx);
        FrameShape {
            controls: draw_ui::control_count(&self.tree),
            commands: ctx.into_draw_list().len(),
        }
    }

    /// Height of the viewport the rows scroll through.
    pub fn viewport_height(&self) -> f32 {
        draw_ui::control(&self.tree, self.pane).map_or(0.0, |data| data.rect.size.height)
    }
}

/// The virtualized list: the pool holds what the viewport shows, and scrolling
/// moves and re-binds those nodes instead of building new ones.
pub struct ListVirtualFixture {
    pub tree: SceneTree,
    /// The list's shared state, driven exactly as an application would.
    pub state: ListState,
    viewport: ViewportSize,
    count: Rc<Cell<usize>>,
    /// Mirrors [`ListState::offset`] so the scroll direction can be tracked —
    /// the pool only needs a target offset, not a history.
    offset: f32,
    /// `1.0` scrolling down, `-1.0` back up (see [`bounce`]).
    direction: f32,
    max_offset: f32,
}

impl ListVirtualFixture {
    /// Builds a panel-sized list over `n` rows and mounts its pool.
    pub fn new(n: usize) -> Self {
        let theme = default_theme(Mode::Dark);
        let viewport = ViewportSize::new(VIEWPORT_SIZE);
        let mut tree = SceneTree::new();
        let root = tree.add_child(
            tree.root(),
            Flex::column()
                .gap(0.0)
                .padding(Edges::ZERO)
                .mouse_filter(MouseFilter::Ignore),
        );

        let count = Rc::new(Cell::new(n));
        let list = List::new(theme, LIST_ROW_HEIGHT, row_cells)
            .columns(row_columns())
            .count(count.clone());
        let state = list.state();
        tree.add_child(root, list.grow(1.0));

        let mut fixture = Self {
            tree,
            state,
            viewport,
            count,
            offset: 0.0,
            direction: 1.0,
            max_offset: 0.0,
        };
        fixture.layout();
        fixture.max_offset = (n as f32 * LIST_ROW_HEIGHT - fixture.viewport_height()).max(0.0);
        fixture
    }

    /// Rows of data behind the list — the number the frame's cost must not track.
    pub fn row_count(&self) -> usize {
        self.count.get()
    }

    /// Row nodes kept alive.
    pub fn pool_size(&self) -> usize {
        self.state.pool_size()
    }

    /// Data rows the viewport currently covers.
    pub fn visible_rows(&self) -> usize {
        self.state.visible_range().len()
    }

    /// Scroll offset in logical pixels.
    pub fn offset(&self) -> f32 {
        self.state.offset()
    }

    /// Scrolls by `delta` logical pixels, turning around at either end.
    pub fn scroll_by(&mut self, delta: f32) {
        let (offset, direction) = bounce(self.offset, self.direction, delta, self.max_offset);
        // The list's own clamp uses the same ceiling, so the difference is
        // exactly the step and the pool follows the offset the user asked for.
        self.state.scroll_by(offset - self.offset);
        self.offset = offset;
        self.direction = direction;
    }

    /// Height of the viewport the rows scroll through.
    pub fn viewport_height(&self) -> f32 {
        let Some(container) = self.state.container() else {
            return 0.0;
        };
        draw_ui::control(&self.tree, container).map_or(0.0, |data| data.rect.size.height)
    }

    /// The application's frame: lay out, reconcile the pool, and lay out again
    /// only when the reconciliation changed the tree.
    pub fn layout(&mut self) {
        draw_ui::layout(&mut self.tree, self.viewport);
        if self.state.sync(&mut self.tree) {
            draw_ui::layout(&mut self.tree, self.viewport);
        }
        self.tree.update();
    }

    pub fn paint(&self, ctx: &mut PaintContext) {
        draw_ui::paint(&self.tree, ctx);
    }

    pub fn command_hint(&self) -> usize {
        list_command_hint(self.pool_size())
    }

    /// What one frame is billed in.
    pub fn shape(&self) -> FrameShape {
        let mut ctx = PaintContext::with_capacity(self.command_hint());
        self.paint(&mut ctx);
        FrameShape {
            controls: draw_ui::control_count(&self.tree),
            commands: ctx.into_draw_list().len(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scene_fixture_is_deterministic() {
        let a = SceneFixture::new(50);
        let b = SceneFixture::new(50);
        assert_eq!(a.ids.len(), 50);
        assert_eq!(a.tree.node_count(), 51);

        let mut ctx_a = draw_render::PaintContext::new();
        let mut ctx_b = draw_render::PaintContext::new();
        a.tree.paint(&mut ctx_a);
        b.tree.paint(&mut ctx_b);
        assert_eq!(ctx_a.into_draw_list(), ctx_b.into_draw_list());
        assert_eq!(a.expected_commands(), a.ids.len() * 4);
    }

    #[test]
    fn scene_fixture_starts_clean() {
        let mut fixture = SceneFixture::new(100);
        assert_eq!(fixture.tree.update(), 0);
    }

    #[test]
    fn ui_fixture_lays_out_and_hit_tests() {
        let fixture = UiFixture::new(100);
        assert_eq!(fixture.ids.len(), 100);
        let hit = fixture.hit_test(fixture.first_center);
        assert_eq!(hit, Some(fixture.ids[0]));
    }

    #[test]
    fn ui_fixture_is_deterministic() {
        let mut a = UiFixture::new(20);
        let mut b = UiFixture::new(20);
        a.layout(a.viewport);
        b.layout(b.viewport);

        let mut ctx_a = draw_render::PaintContext::new();
        let mut ctx_b = draw_render::PaintContext::new();
        a.paint(&mut ctx_a);
        b.paint(&mut ctx_b);
        assert_eq!(ctx_a.into_draw_list(), ctx_b.into_draw_list());
    }

    /// Both shapes have to scroll over the same viewport, or the comparison
    /// measures two different UIs.
    #[test]
    fn both_list_fixtures_fill_the_viewport() {
        let full = ListFullFixture::new(100);
        assert_eq!(full.viewport_height(), VIEWPORT_SIZE.height);

        let virtual_list = ListVirtualFixture::new(100);
        let container = virtual_list.state.container().expect("container");
        let rect = draw_ui::control(&virtual_list.tree, container)
            .unwrap()
            .rect;
        assert_eq!(rect.size.height, VIEWPORT_SIZE.height);
        assert_eq!(rect.size.width, VIEWPORT_SIZE.width);
    }

    /// The naive shape mounts three controls per row (row + two cells) plus the
    /// pane and the block they live in.
    #[test]
    fn the_naive_list_mounts_every_row() {
        let fixture = ListFullFixture::new(100);
        assert_eq!(fixture.row_count(), 100);
        assert_eq!(fixture.shape().controls, 100 * 3 + 3);
    }

    /// The claim the virtualized list is built on: the frame's cost tracks the
    /// viewport, not the data. A hundredfold more rows must not move a number.
    #[test]
    fn the_virtual_list_shape_is_flat_in_the_row_count() {
        let small = ListVirtualFixture::new(1_000);
        let large = ListVirtualFixture::new(100_000);

        assert_eq!(small.shape(), large.shape());
        assert_eq!(small.pool_size(), large.pool_size());
        assert_eq!(small.visible_rows(), large.visible_rows());
        assert_eq!(
            small.pool_size(),
            (VIEWPORT_SIZE.height / LIST_ROW_HEIGHT).ceil() as usize + 1
        );
    }

    /// The control the previous test needs to mean something: the same growth in
    /// the naive shape shows up in both numbers.
    #[test]
    fn the_naive_list_shape_grows_with_the_row_count() {
        let small = ListFullFixture::new(1_000).shape();
        let large = ListFullFixture::new(10_000).shape();

        assert!(large.controls > small.controls * 9);
        assert!(large.commands > small.commands * 9);
    }

    /// Scrolling is the operation the two shapes actually differ in, so both
    /// have to move — and the naive one has to move all of its rows while the
    /// virtual one moves only its pool.
    #[test]
    fn one_scrolling_frame_moves_all_the_rows_but_mounts_none() {
        let mut full = ListFullFixture::new(10_000);
        let before = full.shape();
        full.scroll_by(LIST_SCROLL_STEP);
        full.layout();
        assert!(full.offset() > 0.0);
        assert_eq!(
            full.shape(),
            before,
            "the naive shape does not change count"
        );

        let mut virtual_list = ListVirtualFixture::new(10_000);
        let rows = virtual_list.state.rows();
        let before = virtual_list.shape();
        virtual_list.scroll_by(LIST_SCROLL_STEP);
        virtual_list.layout();

        assert!(virtual_list.offset() > 0.0);
        assert_eq!(virtual_list.state.rows(), rows, "the pool is recycled");
        assert_eq!(virtual_list.shape(), before);
    }

    /// A scroll step of 2.5 rows leaves rows half out of the viewport, which is
    /// the case the container's clip exists for: the pool is the viewport's rows
    /// plus the buffer, never more.
    #[test]
    fn the_pool_absorbs_a_fractional_scroll() {
        let mut fixture = ListVirtualFixture::new(10_000);
        for _ in 0..40 {
            fixture.scroll_by(LIST_SCROLL_STEP);
            fixture.layout();
            assert_eq!(
                fixture.state.pool_size(),
                (VIEWPORT_SIZE.height / LIST_ROW_HEIGHT).ceil() as usize + 1,
                "the pool never grows past the viewport plus its buffer"
            );
        }
        assert_eq!(fixture.offset(), 40.0 * LIST_SCROLL_STEP);
    }

    /// The scrolling pattern itself has to keep scrolling — a one-way scroll
    /// parks at the bottom, and the harness calls the routine tens of thousands
    /// of times per benchmark, so most of the samples would measure a stopped
    /// list. Both shapes also have to walk the same track, or the timings are
    /// not comparable.
    #[test]
    fn the_scroll_pattern_always_moves_and_turns_around() {
        let mut full = ListFullFixture::new(100);
        let mut virtual_list = ListVirtualFixture::new(100);
        let max = 100.0 * LIST_ROW_HEIGHT - VIEWPORT_SIZE.height;
        assert_eq!(full.offset(), virtual_list.offset());

        let mut previous = 0.0f32;
        let mut last_step = 0.0f32;
        let mut turns = 0;
        for _ in 0..2_000 {
            full.scroll_by(LIST_SCROLL_STEP);
            virtual_list.scroll_by(LIST_SCROLL_STEP);
            let offset = full.offset();
            assert_eq!(offset, virtual_list.offset(), "one track for both shapes");
            assert!((0.0..=max).contains(&offset));
            let step = offset - previous;
            assert!(
                (step.abs() - LIST_SCROLL_STEP).abs() < 1e-3,
                "every frame moves a full step, got {step}"
            );
            if step.signum() != last_step.signum() {
                turns += 1;
            }
            last_step = step;
            previous = offset;
        }
        assert!(
            turns >= 10,
            "the track turns around repeatedly, got {turns}"
        );
    }

    /// The data can shrink under the list without the pool being rebuilt.
    #[test]
    fn a_shrinking_count_keeps_the_pool() {
        let mut fixture = ListVirtualFixture::new(10_000);
        let pool = fixture.pool_size();
        fixture.count.set(7);
        fixture.layout();

        assert_eq!(fixture.pool_size(), pool, "the pool is not torn down");
        assert_eq!(fixture.visible_rows(), 7, "it just shows fewer rows");
    }
}
