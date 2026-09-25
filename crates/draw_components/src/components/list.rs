//! Virtualized lists: mount what the viewport shows, recycle the rest.

use std::cell::{Cell, RefCell};
use std::ops::Range;
use std::rc::Rc;

use draw_core::{Color, Edges, NodeId, Rect, Size, Vec2};
use draw_render::PaintContext;
use draw_scene::SceneTree;
use draw_theme::{default_theme, Mode, Space, TextSize, Theme, Tone};
use draw_ui::{
    dynamic_surface_decor, Align, Control, MouseFilter, SizeBasis, SurfaceStyle, Widget,
};

use crate::base::{
    apply_spec, set_on_click, set_on_scroll, set_on_secondary, set_text, update_control, Component,
    Spec,
};
use crate::{CheckState, Checkbox, Flex, NodeRef, Row, Text};

/// Supplies the cell text of one row on demand.
///
/// The list pulls only the rows it is about to show, so the data never has to
/// exist as a materialized list of widgets — a directory with 10 000 entries
/// and one with 10 cost the same frame.
pub type RowSource = Rc<dyn Fn(usize) -> Vec<String>>;

/// A slot that has not been bound to a data row yet.
const UNBOUND: usize = usize::MAX;

/// How one column of a row is sized and toned.
///
/// Named `ListColumn` so it does not collide with the flex [`Column`](crate::Column)
/// container.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ListColumn {
    /// Fixed width in logical pixels; `None` takes the leftover space.
    pub width: Option<f32>,
    /// Tone of the cell's text.
    pub tone: Tone,
}

impl ListColumn {
    /// A column that absorbs the space the fixed columns leave over.
    pub fn flexible() -> Self {
        Self {
            width: None,
            tone: Tone::Default,
        }
    }

    /// A fixed-width column, muted by default (sizes, timestamps).
    pub fn fixed(width: f32) -> Self {
        Self {
            width: Some(width),
            tone: Tone::Muted,
        }
    }

    pub fn tone(mut self, tone: Tone) -> Self {
        self.tone = tone;
        self
    }
}

/// Draws a [`ListLead::Icon`] into its cell for a data row (the index is the
/// absolute row index).
pub type LeadDraw = Rc<dyn Fn(&mut PaintContext, Rect, usize)>;

/// A per-row leading cell, mounted before the text columns.
///
/// Leads are laid out left to right in the order they are added, so a tree puts
/// an indentation spacer, a checkbox and an icon before its name column. Every
/// lead is bound to the slot's current data row on each (re)bind, so a recycled
/// row always shows the right state.
#[derive(Clone)]
pub enum ListLead {
    /// A checkbox; `state(index)` drives it and `on_toggle(index)` fires on
    /// click. Clicking the box does not activate the row (the nearest callback
    /// wins), so checking and selecting stay separate.
    Checkbox {
        state: Rc<dyn Fn(usize) -> CheckState>,
        on_toggle: Rc<dyn Fn(usize)>,
    },
    /// A spacer whose width depends on the row (tree indentation).
    Spacer { width: Rc<dyn Fn(usize) -> f32> },
    /// A caller-drawn icon of `width` (e.g. an SVG document).
    Icon { width: f32, draw: LeadDraw },
}

/// The mounted node(s) backing one [`ListLead`] in a row slot.
enum LeadSlot {
    Checkbox(Rc<std::cell::Cell<crate::CheckState>>),
    Spacer(NodeRef),
    Icon,
}

/// A lead that strokes a caller-provided draw closure into its cell. It is
/// `MouseFilter::Ignore`, so a click lands on the row behind it.
struct LeadIcon {
    spec: Spec,
    width: f32,
    first: Rc<std::cell::Cell<usize>>,
    slot: usize,
    draw: LeadDraw,
}

impl LeadIcon {
    fn new(width: f32, first: Rc<std::cell::Cell<usize>>, slot: usize, draw: LeadDraw) -> Self {
        Self {
            spec: Spec::leaf(),
            width,
            first,
            slot,
            draw,
        }
    }
}

impl Component for LeadIcon {
    fn spec(&mut self) -> &mut Spec {
        &mut self.spec
    }

    fn name(&self) -> &'static str {
        "LeadIcon"
    }

    fn widget(&self) -> Widget {
        Widget::Panel {
            color: Color::TRANSPARENT,
            border: None,
        }
    }

    fn prepare(&mut self) {
        // A non-zero height avoids a degenerate background fill; the icon is
        // centred in the cell regardless.
        self.spec.data.min_size = Size::new(self.width, self.width);
        self.spec.data.mouse_filter = MouseFilter::Ignore;
        let first = self.first.clone();
        let slot = self.slot;
        let draw = self.draw.clone();
        self.spec.foreground = Some(Box::new(move |ctx, rect, _| {
            draw(ctx, rect, first.get() + slot);
        }));
    }
}

/// The mounted rows of a list, shared with the application.
///
/// Cloning is cheap and shares one state, so a view keeps a handle and drives
/// the list while the [`List`] component itself is mounted into the tree.
#[derive(Clone)]
pub struct ListState {
    inner: Rc<RefCell<ListInner>>,
}

struct ListInner {
    container: Option<NodeId>,
    theme: &'static dyn Theme,
    columns: Vec<ListColumn>,
    leads: Vec<ListLead>,
    source: RowSource,
    on_activate: Option<Rc<dyn Fn(usize)>>,
    /// Called with the data index and the pointer position on a right click.
    on_context: Option<Rc<dyn Fn(usize, Vec2)>>,
    count: Rc<Cell<usize>>,
    /// Recycled row slots, in viewport order.
    slots: Vec<Slot>,
    /// Data index of slot 0 — shared with the row backgrounds, which read it
    /// while painting instead of reaching through the `RefCell`.
    first: Rc<Cell<usize>>,
    selected: Rc<Cell<Option<usize>>>,
    row_height: f32,
    padding: f32,
    gap: f32,
    offset: f32,
    max_offset: f32,
    viewport_height: f32,
    /// Slots carrying a row inside the viewport, as of the last sync. The pool
    /// is usually one larger: the buffer that absorbs scrolling.
    shown: usize,
    /// Row count seen by the last [`ListState::sync`].
    bound_count: usize,
}

struct Slot {
    root: NodeId,
    leads: Vec<LeadSlot>,
    cells: Vec<Vec<NodeId>>,
    /// Data index currently bound to this slot ([`UNBOUND`] = needs a bind).
    bound: usize,
}

impl ListState {
    pub fn new() -> Self {
        let theme = default_theme(Mode::Dark);
        Self {
            inner: Rc::new(RefCell::new(ListInner {
                container: None,
                theme,
                columns: Vec::new(),
                leads: Vec::new(),
                source: Rc::new(|_| Vec::new()),
                on_activate: None,
                on_context: None,
                count: Rc::new(Cell::new(0)),
                slots: Vec::new(),
                first: Rc::new(Cell::new(0)),
                selected: Rc::new(Cell::new(None)),
                row_height: 0.0,
                padding: theme.spacing(Space::SM),
                gap: theme.spacing(Space::MD),
                offset: 0.0,
                max_offset: 0.0,
                viewport_height: 0.0,
                shown: 0,
                bound_count: 0,
            })),
        }
    }

    /// Reconciles the row pool with the viewport and the data, reporting
    /// whether the tree changed and therefore wants another
    /// [`layout`](draw_ui::layout) before the frame is painted.
    ///
    /// Call it *after* layout — the viewport is the container's resolved height
    /// — and lay out again when it returns `true`. When nothing moved, which is
    /// every frame the user does not scroll, it walks the pool and touches
    /// nothing.
    pub fn sync(&mut self, tree: &mut SceneTree) -> bool {
        self.inner.borrow_mut().sync(tree)
    }

    /// The row nodes currently mounted, in viewport order.
    pub fn rows(&self) -> Vec<NodeId> {
        self.inner
            .borrow()
            .slots
            .iter()
            .map(|slot| slot.root)
            .collect()
    }

    /// The data rows the viewport covers, including the partial rows at the
    /// edges (they are mounted, just clipped).
    pub fn visible_range(&self) -> Range<usize> {
        let inner = self.inner.borrow();
        let start = inner.first.get();
        let end = (start + inner.shown).min(inner.count.get());
        start..end.max(start)
    }

    /// Number of row nodes the list keeps alive.
    pub fn pool_size(&self) -> usize {
        self.inner.borrow().slots.len()
    }

    /// The list container node, once mounted.
    pub fn container(&self) -> Option<NodeId> {
        self.inner.borrow().container
    }

    /// Scroll offset in logical pixels.
    pub fn offset(&self) -> f32 {
        self.inner.borrow().offset
    }

    /// Shared selection (`None` = nothing selected).
    pub fn selected(&self) -> Rc<Cell<Option<usize>>> {
        self.inner.borrow().selected.clone()
    }

    /// Scrolls by `delta` logical pixels (positive scrolls down).
    ///
    /// Pure state — the tree is not touched, so this is safe from the wheel
    /// callback, and [`ListState::sync`] applies it to the pool.
    pub fn scroll_by(&self, delta: f32) {
        let mut inner = self.inner.borrow_mut();
        inner.offset = (inner.offset + delta).clamp(0.0, inner.max_offset);
    }

    /// Scrolls the smallest amount that puts `index` inside the viewport.
    pub fn scroll_to(&mut self, index: usize) {
        let mut inner = self.inner.borrow_mut();
        let row = inner.row_height;
        if row <= 0.0 {
            return;
        }
        let top = index as f32 * row;
        let bottom = top + row;
        if top < inner.offset {
            inner.offset = top;
        } else if bottom > inner.offset + inner.viewport_height {
            inner.offset = (bottom - inner.viewport_height).clamp(0.0, inner.max_offset);
        }
    }

    /// Marks every slot as needing to re-read its row on the next sync.
    ///
    /// The row count alone cannot notice that the *contents* changed (a
    /// re-scan of a directory that happens to have the same number of files),
    /// so an application that swaps its data calls this.
    pub fn invalidate(&mut self) {
        for slot in self.inner.borrow_mut().slots.iter_mut() {
            slot.bound = UNBOUND;
        }
    }
}

impl Default for ListState {
    fn default() -> Self {
        Self::new()
    }
}

impl ListInner {
    /// Slots the viewport can show, plus one for the partial row at the bottom
    /// edge. This is the pool the list grows to, not the number of rows on
    /// screen — that is counted per sync, once the offset is known.
    fn needed_slots(&self) -> usize {
        if self.row_height <= 0.0 || self.viewport_height <= 0.0 {
            return 0;
        }
        (self.viewport_height / self.row_height).ceil() as usize + 1
    }

    fn sync(&mut self, tree: &mut SceneTree) -> bool {
        let Some(container) = self.container else {
            return false;
        };
        let Some(rect) = draw_ui::control(tree, container).map(|data| data.rect) else {
            return false;
        };
        if rect.size.height <= 0.0 {
            return false;
        }
        self.viewport_height = rect.size.height;

        let count = self.count.get();
        let data_changed = count != self.bound_count;
        let mut changed = false;

        // Never mount a slot without a row to put in it, and never unmount one:
        // the pool only grows, and a surplus is hidden (see `visible_slots`).
        let target = self.needed_slots().min(count);
        if self.slots.len() < target {
            for _ in self.slots.len()..target {
                let slot = self.mount_slot(tree, container);
                self.slots.push(slot);
            }
            // A node added after the last layout is only reached if its parent
            // is arranged again; mounting is what makes that necessary.
            draw_ui::mark_dirty(tree, container);
            changed = true;
        }

        let content = count as f32 * self.row_height;
        self.max_offset = (content - self.viewport_height).max(0.0);
        self.offset = self.offset.clamp(0.0, self.max_offset);
        let first = (self.offset / self.row_height).floor() as usize;
        self.first.set(first);

        let visible_slots = self.slots.len();
        let mut shown = 0usize;
        for slot_index in 0..visible_slots {
            let index = first + slot_index;
            // The row stays where it is — clipped by the container — until it
            // is fully scrolled past, so `y` runs from -row_height down to the
            // viewport's bottom edge. The slot one past the edge is the buffer
            // that makes the next scroll step a pure offset change.
            let y = index as f32 * self.row_height - self.offset;
            let show = index < count && y < self.viewport_height;
            let bound = self.slots[slot_index].bound;
            let root = self.slots[slot_index].root;

            if tree.is_visible(root) != Some(show) {
                tree.set_visible(root, show);
                draw_ui::mark_dirty(tree, root);
                self.slots[slot_index].bound = UNBOUND;
                changed = true;
            }
            if !show {
                continue;
            }
            shown += 1;

            let want = Edges::new(0.0, y, 0.0, y + self.row_height);
            if draw_ui::control(tree, root).is_some_and(|data| data.offsets != want) {
                update_control(tree, root, |data| data.offsets = want);
                changed = true;
            }
            if bound != index || data_changed {
                self.bind_slot(tree, slot_index, index);
                changed = true;
            }
        }
        self.shown = shown;
        self.bound_count = count;
        changed
    }

    /// Mounts one recycled row: a flex row of leading cells plus text cells, a
    /// state-driven background and a click that selects.
    fn mount_slot(&self, tree: &mut SceneTree, container: NodeId) -> Slot {
        let theme = self.theme;
        let first = self.first.clone();
        let selected = self.selected.clone();
        let activate = self.on_activate.clone();
        let context = self.on_context.clone();
        let slot_index = self.slots.len();
        // Both callbacks read the slot's current data row through the shared
        // `first`, so a recycled row always answers for what it is showing.
        let paint_first = first.clone();
        let paint_selected = selected.clone();
        let click_first = first.clone();
        let click_selected = selected;

        let mut row = Row::new()
            .gap(self.gap)
            .padding(Edges::new(self.padding, 0.0, self.padding, 0.0))
            // Cells take their natural height and sit on the row's centre line,
            // instead of stretching to the full row (which top-aligns the text).
            .align(Align::Center)
            // Rows are placed by the list, not by their parent: left/right span
            // the container, top/bottom come from the scroll offset.
            .anchors(Edges::new(0.0, 0.0, 1.0, 0.0));

        // Leading cells first, so they sit left of the text columns.
        let mut leads = Vec::new();
        for lead in &self.leads {
            match lead {
                ListLead::Checkbox { state, on_toggle } => {
                    let state_cell = Rc::new(std::cell::Cell::new(state(first.get() + slot_index)));
                    let toggle = on_toggle.clone();
                    let slot_first = first.clone();
                    row = row.child(
                        Checkbox::new("", theme)
                            .state(state_cell.clone())
                            .min_size(0.0, self.row_height)
                            .on_change(move |_| toggle(slot_first.get() + slot_index)),
                    );
                    leads.push(LeadSlot::Checkbox(state_cell));
                }
                ListLead::Spacer { .. } => {
                    let node = NodeRef::new();
                    row = row.child(
                        Flex::new()
                            .padding(Edges::ZERO)
                            .mouse_filter(MouseFilter::Ignore)
                            .min_size(0.0, 0.0)
                            .ref_(&node),
                    );
                    leads.push(LeadSlot::Spacer(node));
                }
                ListLead::Icon { width, draw } => {
                    row = row.child(LeadIcon::new(
                        *width,
                        first.clone(),
                        slot_index,
                        draw.clone(),
                    ));
                    leads.push(LeadSlot::Icon);
                }
            }
        }

        // Then the text columns. The `NodeRef`s capture them in order, so the
        // leads can be prepended without re-deriving the child list.
        let mut columns = Vec::new();
        for column in &self.columns {
            let cell = Text::new("", theme)
                .size(TextSize::Small)
                .tone(column.tone)
                .max_lines(1)
                .ellipsis(true)
                .shrink(0.0);
            let node = NodeRef::new();
            row = row.child(match column.width {
                Some(width) => cell.basis(SizeBasis::Px(width)).ref_(&node),
                None => cell.grow(1.0).ref_(&node),
            });
            columns.push(node);
        }
        let root = tree.add_child(container, row);
        let cells: Vec<Vec<NodeId>> = columns
            .iter()
            .filter_map(|node| node.get())
            .map(|id| vec![id])
            .collect();

        draw_ui::add_decor(
            tree,
            root,
            dynamic_surface_decor(move |state| {
                let index = paint_first.get() + slot_index;
                if paint_selected.get() == Some(index) {
                    SurfaceStyle::new(theme.palette().selection)
                } else if state.hovered {
                    SurfaceStyle::new(theme.palette().surface_hover)
                } else {
                    SurfaceStyle::new(Color::TRANSPARENT)
                }
            }),
        );
        set_on_click(tree, root, move || {
            let index = click_first.get() + slot_index;
            click_selected.set(Some(index));
            if let Some(callback) = &activate {
                callback(index);
            }
        });
        if let Some(context) = context {
            let context_first = first.clone();
            set_on_secondary(tree, root, move |position| {
                context(context_first.get() + slot_index, position);
            });
        }

        Slot {
            root,
            leads,
            cells,
            bound: UNBOUND,
        }
    }

    fn bind_slot(&mut self, tree: &mut SceneTree, slot_index: usize, index: usize) {
        let cells = (self.source)(index);
        let leads = self.leads.clone();
        let Some(slot) = self.slots.get_mut(slot_index) else {
            return;
        };
        for (lead, mounted) in leads.iter().zip(slot.leads.iter()) {
            match (lead, mounted) {
                (ListLead::Checkbox { state, .. }, LeadSlot::Checkbox(cell)) => {
                    cell.set(state(index));
                }
                (ListLead::Spacer { width }, LeadSlot::Spacer(node)) => {
                    let width = width(index);
                    if let Some(node) = node.get() {
                        if update_control(tree, node, |data| data.min_size.width = width) {
                            draw_ui::mark_dirty(tree, node);
                        }
                    }
                }
                _ => {}
            }
        }
        for (column, nodes) in slot.cells.iter().enumerate() {
            let text = cells.get(column).cloned().unwrap_or_default();
            for node in nodes {
                set_text(tree, *node, text.clone());
            }
        }
        slot.bound = index;
    }
}

/// A list that mounts only the rows its viewport can show.
///
/// The pool is sized from the container's resolved height (the rows that fit,
/// plus the partially visible one at the bottom edge) and scrolling moves and
/// re-binds those nodes instead of building new ones — so the node count, the
/// layout work and the emitted commands depend on the viewport, not on how many
/// rows the data has. The container clips (`draw_ui::set_clip`), so the partial
/// rows at the edges are cut off instead of bleeding over the pane.
///
/// ```ignore
/// let count = Rc::new(Cell::new(entries.len()));
/// let list = List::new(theme, 28.0, {
///     let entries = entries.clone();
///     move |index| vec![entries[index].name.clone(), entries[index].size.clone()]
/// })
/// .columns(vec![ListColumn::flexible(), ListColumn::fixed(96.0)])
/// .count(count.clone());
/// let state = list.state();
/// tree.add_child(pane, list.grow(1.0));
///
/// draw_ui::layout(&mut tree, viewport);
/// if state.sync(&mut tree) {
///     draw_ui::layout(&mut tree, viewport);
/// }
/// ```
pub struct List {
    spec: Spec,
    state: ListState,
    theme: &'static dyn Theme,
    row_height: f32,
    padding: f32,
    gap: f32,
    source: RowSource,
    columns: Vec<ListColumn>,
    leads: Vec<ListLead>,
    count: Rc<Cell<usize>>,
    selected: Rc<Cell<Option<usize>>>,
    on_activate: Option<Rc<dyn Fn(usize)>>,
    on_context: Option<Rc<dyn Fn(usize, Vec2)>>,
}

impl List {
    /// A list of `row_height`-tall rows pulling their cells from `source`.
    pub fn new(
        theme: &'static dyn Theme,
        row_height: f32,
        source: impl Fn(usize) -> Vec<String> + 'static,
    ) -> Self {
        Self {
            spec: Spec::default(),
            state: ListState::new(),
            theme,
            row_height: row_height.max(1.0),
            padding: theme.spacing(Space::SM),
            gap: theme.spacing(Space::MD),
            source: Rc::new(source),
            columns: vec![ListColumn::flexible()],
            leads: Vec::new(),
            count: Rc::new(Cell::new(0)),
            selected: Rc::new(Cell::new(None)),
            on_activate: None,
            on_context: None,
        }
    }

    /// Column layout. Defaults to a single flexible column.
    pub fn columns(mut self, columns: Vec<ListColumn>) -> Self {
        if !columns.is_empty() {
            self.columns = columns;
        }
        self
    }

    /// Adds a per-row leading cell, before the text columns. Leads stack left to
    /// right in call order (indentation, checkbox, icon, …).
    pub fn lead(mut self, lead: ListLead) -> Self {
        self.leads.push(lead);
        self
    }

    /// Adds a checkbox lead: `state` drives the box (checked / unchecked /
    /// indeterminate) and `on_toggle` fires on click. The box owns the click,
    /// so checking does not activate the row.
    pub fn checkboxes(
        mut self,
        state: impl Fn(usize) -> CheckState + 'static,
        on_toggle: impl Fn(usize) + 'static,
    ) -> Self {
        self.leads.push(ListLead::Checkbox {
            state: Rc::new(state),
            on_toggle: Rc::new(on_toggle),
        });
        self
    }

    /// Adds a per-row indentation spacer of `width(index)` logical pixels.
    pub fn spacer(mut self, width: impl Fn(usize) -> f32 + 'static) -> Self {
        self.leads.push(ListLead::Spacer {
            width: Rc::new(width),
        });
        self
    }

    /// Adds a caller-drawn icon cell of `width` (e.g. an SVG document).
    pub fn icon(
        mut self,
        width: f32,
        draw: impl Fn(&mut PaintContext, Rect, usize) + 'static,
    ) -> Self {
        self.leads.push(ListLead::Icon {
            width,
            draw: Rc::new(draw),
        });
        self
    }

    /// Shared row count, read on every sync — a re-scan just sets it.
    pub fn count(mut self, count: Rc<Cell<usize>>) -> Self {
        self.count = count;
        self
    }

    /// Shared selection, surviving a re-mount of the pool.
    pub fn selected(mut self, selected: Rc<Cell<Option<usize>>>) -> Self {
        self.selected = selected;
        self
    }

    /// Called with the data index when a row is clicked.
    pub fn on_activate(mut self, callback: impl Fn(usize) + 'static) -> Self {
        self.on_activate = Some(Rc::new(callback));
        self
    }

    /// Called with the data index and the pointer position on a secondary
    /// (right) click — the caller opens a context menu at the position.
    pub fn on_context(mut self, callback: impl Fn(usize, Vec2) + 'static) -> Self {
        self.on_context = Some(Rc::new(callback));
        self
    }

    /// Inset of a row's first and last cell, in logical pixels.
    pub fn padding(mut self, padding: f32) -> Self {
        self.padding = padding;
        self
    }

    /// Space between two cells of a row.
    pub fn gap(mut self, gap: f32) -> Self {
        self.gap = gap;
        self
    }

    /// The shared state handle the application drives (`sync`, `scroll_to`, …).
    ///
    /// Take it before mounting — the component consumes itself in `build`.
    pub fn state(&self) -> ListState {
        self.state.clone()
    }
}

impl Component for List {
    fn spec(&mut self) -> &mut Spec {
        &mut self.spec
    }

    fn name(&self) -> &'static str {
        "List"
    }

    fn widget(&self) -> Widget {
        Widget::Panel {
            color: self.theme.palette().surface,
            border: None,
        }
    }

    fn prepare(&mut self) {
        // Its rectangle is what the rows are clipped to, and it is what the
        // wheel is routed to (`set_on_scroll` in `build`).
        self.spec.data.mouse_filter = MouseFilter::Stop;
        self.spec.data.clip = true;
    }

    fn build(mut self, tree: &mut SceneTree, parent: NodeId) -> NodeId {
        self.prepare();
        let spec = std::mem::take(self.spec());
        let id = tree.add_control(parent, self.name());
        tree.set_data(id, Control::new(spec.data, self.widget()));
        apply_spec(tree, id, spec);

        {
            let mut inner = self.state.inner.borrow_mut();
            inner.container = Some(id);
            inner.theme = self.theme;
            inner.columns = std::mem::take(&mut self.columns);
            inner.leads = std::mem::take(&mut self.leads);
            inner.source = self.source;
            inner.on_activate = self.on_activate.take();
            inner.on_context = self.on_context.take();
            inner.count = self.count;
            inner.selected = self.selected;
            inner.row_height = self.row_height;
            inner.padding = self.padding;
            inner.gap = self.gap;
        }
        let scrolling = self.state.clone();
        set_on_scroll(tree, id, move |delta: Vec2| scrolling.scroll_by(delta.y));
        id
    }
}

crate::impl_scene_child!(List);

#[cfg(test)]
mod tests {
    use super::*;
    use draw_theme::{default_theme, Mode};
    use std::cell::RefCell;

    use draw_core::{EventResult, InputEvent, Rect, Size, ViewportSize};
    use draw_render::{DrawCommand, DrawList, PaintContext};
    use draw_ui::handle_input;

    use crate::Flex;

    const WIDTH: f32 = 400.0;
    const HEIGHT: f32 = 240.0;
    const ROW: f32 = 24.0;
    /// The pool the viewport grows to: the 10 rows that fit, plus the buffer
    /// that absorbs the next scroll step.
    const POOL: usize = 11;
    /// The rows actually on screen when the offset is a whole number of rows.
    const VISIBLE: usize = 10;

    /// A mounted list with one flexible column per `cells`, backed by text the
    /// test can rewrite.
    struct Fixture {
        tree: SceneTree,
        state: ListState,
        data: Rc<RefCell<Vec<Vec<String>>>>,
        count: Rc<Cell<usize>>,
        selected: Rc<Cell<Option<usize>>>,
        list: NodeId,
    }

    impl Fixture {
        fn new(count: usize, cells: usize) -> Self {
            let mut tree = SceneTree::new();
            let root = tree.add_child(
                tree.root(),
                Flex::column()
                    .gap(0.0)
                    .padding(Edges::ZERO)
                    .mouse_filter(MouseFilter::Ignore),
            );
            let data: Rc<RefCell<Vec<Vec<String>>>> = Rc::new(RefCell::new(
                (0..count)
                    .map(|index| (0..cells).map(|c| format!("r{index}c{c}")).collect())
                    .collect(),
            ));
            let count_cell = Rc::new(Cell::new(count));
            let selected = Rc::new(Cell::new(None));
            let source = data.clone();
            let list = List::new(default_theme(Mode::Light), ROW, move |index| {
                source.borrow().get(index).cloned().unwrap_or_default()
            })
            .count(count_cell.clone())
            .selected(selected.clone())
            .columns((0..cells).map(|_| ListColumn::flexible()).collect());
            let state = list.state();
            let list = tree.add_child(root, list.grow(1.0));

            let mut fixture = Self {
                tree,
                state,
                data,
                count: count_cell,
                selected,
                list,
            };
            fixture.frame();
            fixture
        }

        /// The application's per-frame order: lay out, sync, and lay out again
        /// only when syncing changed the tree.
        fn frame(&mut self) {
            let viewport = ViewportSize::new(Size::new(WIDTH, HEIGHT));
            draw_ui::layout(&mut self.tree, viewport);
            if self.state.sync(&mut self.tree) {
                draw_ui::layout(&mut self.tree, viewport);
            }
            self.tree.update();
        }

        fn rect(&self) -> Rect {
            draw_ui::control(&self.tree, self.list).unwrap().rect
        }

        fn controls(&self) -> usize {
            draw_ui::control_count(&self.tree)
        }

        fn cell(&self, slot: usize, column: usize) -> String {
            let row = self.state.rows()[slot];
            let node = self.tree.children(row).unwrap()[column];
            match draw_ui::widget(&self.tree, node) {
                Some(Widget::Label { text, .. }) => text.clone(),
                other => panic!("row cell is not a label: {other:?}"),
            }
        }

        fn paint(&self) -> DrawList {
            let mut ctx = PaintContext::new();
            draw_ui::paint(&self.tree, &mut ctx);
            ctx.into_draw_list()
        }

        fn click(&mut self, position: Vec2) {
            for event in [
                InputEvent::PointerDown {
                    position,
                    button: draw_core::PointerButton::Left,
                },
                InputEvent::PointerUp {
                    position,
                    button: draw_core::PointerButton::Left,
                },
            ] {
                handle_input(&mut self.tree, &event);
            }
        }

        fn wheel(&mut self, position: Vec2, delta: Vec2) -> EventResult {
            let result = handle_input(&mut self.tree, &InputEvent::Wheel { position, delta });
            self.frame();
            result
        }
    }

    /// The claim the component exists for: the pool is sized by the viewport,
    /// so a directory with 10 000 entries mounts exactly as much as one with 8.
    #[test]
    fn the_pool_is_sized_by_the_viewport_not_by_the_data() {
        let many = Fixture::new(10_000, 1);
        assert_eq!(many.state.pool_size(), POOL);
        assert_eq!(many.state.visible_range(), 0..VISIBLE);

        // Fewer rows than slots: the pool stops at the data, so an empty list
        // mounts nothing at all.
        let few = Fixture::new(8, 1);
        assert_eq!(few.state.pool_size(), 8);
        assert_eq!(few.state.visible_range(), 0..8);
    }

    /// Node count and emitted commands are what a frame costs; both have to be
    /// flat in the row count for the virtualization to mean anything.
    #[test]
    fn a_frame_does_not_grow_with_the_row_count() {
        let small = Fixture::new(12, 1);
        let large = Fixture::new(10_000, 1);
        assert_eq!(small.state.visible_range(), large.state.visible_range());

        assert_eq!(small.controls(), large.controls());
        assert_eq!(small.paint().len(), large.paint().len());
    }

    #[test]
    fn scrolling_moves_and_rebinds_the_same_row_nodes() {
        let mut fixture = Fixture::new(1_000, 2);
        let before = fixture.state.rows();
        assert_eq!(fixture.cell(0, 0), "r0c0");
        assert_eq!(fixture.cell(0, 1), "r0c1");

        fixture.state.scroll_by(5.0 * ROW);
        fixture.frame();

        assert_eq!(fixture.state.rows(), before, "the pool is recycled");
        assert_eq!(fixture.state.visible_range().start, 5);
        assert_eq!(fixture.cell(0, 0), "r5c0");
        assert_eq!(fixture.cell(0, 1), "r5c1");
        assert_eq!(fixture.cell(2, 0), "r7c0");
    }

    /// Cells keep their natural height and sit on the row's centre line, so list
    /// text is vertically centered instead of stretched to the top.
    #[test]
    fn a_row_centers_its_cells_vertically() {
        let fixture = Fixture::new(3, 2);
        let row = fixture.state.rows()[0];
        let row_rect = draw_ui::control(&fixture.tree, row).unwrap().rect;
        let cells: Vec<NodeId> = fixture.tree.children(row).unwrap().to_vec();
        assert_eq!(cells.len(), 2);
        for cell in cells {
            let cell_rect = draw_ui::control(&fixture.tree, cell).unwrap().rect;
            assert!(
                (row_rect.center().y - cell_rect.center().y).abs() < 0.5,
                "cell {cell_rect:?} is not on the row centre line"
            );
            assert!(cell_rect.size.height < row_rect.size.height);
        }
    }

    #[test]
    fn a_wheel_event_over_the_list_scrolls_it() {
        let mut fixture = Fixture::new(1_000, 1);
        let inside = fixture.rect().center();

        assert_eq!(
            fixture.wheel(inside, Vec2::new(0.0, 3.0 * ROW)),
            EventResult::Handled
        );
        assert_eq!(fixture.state.offset(), 3.0 * ROW);
        assert_eq!(fixture.state.visible_range().start, 3);

        // A wheel outside the list is nobody's business, so it stays unhandled
        // and can fall through to `_unhandled_input`.
        assert_eq!(
            fixture.wheel(Vec2::new(WIDTH + 50.0, HEIGHT / 2.0), Vec2::new(0.0, ROW)),
            EventResult::Ignored
        );
        assert_eq!(fixture.state.offset(), 3.0 * ROW);
    }

    #[test]
    fn scrolling_stops_at_both_ends() {
        let mut fixture = Fixture::new(1_000, 1);
        fixture.state.scroll_by(-500.0);
        fixture.frame();
        assert_eq!(fixture.state.offset(), 0.0);

        fixture.state.scroll_by(1_000_000.0);
        fixture.frame();
        let max = 1_000.0 * ROW - HEIGHT;
        assert_eq!(fixture.state.offset(), max);
        assert_eq!(fixture.state.visible_range().end, 1_000);
    }

    /// A row that is only half inside the viewport must be cut off, not drawn
    /// over the pane above it — that is the whole reason the container clips.
    #[test]
    fn the_container_clips_the_partial_rows_at_its_edges() {
        let mut fixture = Fixture::new(1_000, 1);
        fixture.state.scroll_by(5.5 * ROW);
        fixture.frame();

        let container = fixture.rect();
        let top_row = fixture.state.rows()[0];
        let row_rect = draw_ui::control(&fixture.tree, top_row).unwrap().rect;
        assert!(
            row_rect.top() < container.top(),
            "the first mounted row overhangs the viewport: {row_rect:?}"
        );
        assert_eq!(
            draw_ui::control(&fixture.tree, top_row).unwrap().clip_rect,
            Some(container),
            "and it is clipped to the container, not to itself"
        );

        let list = fixture.paint();
        let clips: Vec<Rect> = list
            .commands()
            .iter()
            .filter_map(|command| match command {
                DrawCommand::ClipRect(rect) => Some(*rect),
                _ => None,
            })
            .collect();
        assert_eq!(clips, vec![container], "one clip for the whole pool");
        assert_eq!(
            list.commands()
                .iter()
                .filter(|command| matches!(command, DrawCommand::Save))
                .count(),
            list.commands()
                .iter()
                .filter(|command| matches!(command, DrawCommand::Restore))
                .count(),
            "the clip is popped again"
        );
    }

    #[test]
    fn a_row_clipped_away_takes_no_clicks() {
        let mut fixture = Fixture::new(1_000, 1);
        fixture.state.scroll_by(5.5 * ROW);
        fixture.frame();

        let top_row = fixture.state.rows()[0];
        let row_rect = draw_ui::control(&fixture.tree, top_row).unwrap().rect;
        // Inside the row's own rectangle, above the viewport: without the clip
        // hit-testing would hand this point to the overhanging row.
        let outside = Vec2::new(row_rect.center().x, row_rect.top() + 2.0);
        assert!(row_rect.contains(outside));
        assert_eq!(draw_ui::hit_test(&fixture.tree, outside), None);
    }

    #[test]
    fn clicking_a_row_selects_it() {
        let mut fixture = Fixture::new(1_000, 1);
        let second = fixture.state.rows()[1];
        let center = draw_ui::control(&fixture.tree, second)
            .unwrap()
            .rect
            .center();

        fixture.click(center);
        assert_eq!(fixture.selected.get(), Some(1));

        // The selection follows the *data* row, not the slot: scroll down and
        // the selected index stays where the user put it.
        fixture.state.scroll_by(4.0 * ROW);
        fixture.frame();
        assert_eq!(fixture.selected.get(), Some(1));
    }

    #[test]
    fn a_shrinking_directory_hides_the_rows_that_are_gone() {
        let mut fixture = Fixture::new(1_000, 1);
        fixture.count.set(3);
        fixture.frame();

        assert_eq!(fixture.state.visible_range(), 0..3);
        assert_eq!(
            fixture
                .state
                .rows()
                .into_iter()
                .filter(|row| fixture.tree.is_visible(*row) == Some(true))
                .count(),
            3,
            "the rest of the pool is hidden, not unmounted"
        );
        assert_eq!(fixture.state.offset(), 0.0, "no content left to scroll");
    }

    #[test]
    fn an_empty_list_paints_no_rows() {
        let fixture = Fixture::new(0, 1);
        assert_eq!(fixture.state.pool_size(), 0);
        assert_eq!(fixture.state.visible_range(), 0..0);

        let list = fixture.paint();
        assert!(
            !list
                .commands()
                .iter()
                .any(|command| matches!(command, DrawCommand::DrawText { .. })),
            "an empty directory draws no rows"
        );
    }

    #[test]
    fn invalidate_rereads_the_rows_from_the_source() {
        let mut fixture = Fixture::new(20, 1);
        assert_eq!(fixture.cell(0, 0), "r0c0");

        fixture.data.borrow_mut()[0] = vec!["renamed".to_string()];
        fixture.state.invalidate();
        fixture.frame();
        assert_eq!(fixture.cell(0, 0), "renamed");
    }

    #[test]
    fn a_growing_viewport_grows_the_pool() {
        let mut fixture = Fixture::new(1_000, 1);
        assert_eq!(fixture.state.pool_size(), POOL);

        let taller = 2.0 * HEIGHT;
        let viewport = ViewportSize::new(Size::new(WIDTH, taller));
        draw_ui::layout(&mut fixture.tree, viewport);
        if fixture.state.sync(&mut fixture.tree) {
            draw_ui::layout(&mut fixture.tree, viewport);
        }
        let pool = (taller / ROW).ceil() as usize + 1;
        assert_eq!(fixture.state.pool_size(), pool);
        assert_eq!(
            fixture.state.visible_range(),
            0..(taller / ROW) as usize,
            "the buffer slot sits below the edge until the offset is fractional"
        );
    }

    /// A list with a leading spacer and checkbox, to drive the lead-binding
    /// path (the tree view's indentation + selection).
    struct LeadFixture {
        tree: SceneTree,
        state: ListState,
        selected: Rc<Cell<Option<usize>>>,
        toggled: Rc<RefCell<Vec<usize>>>,
        activated: Rc<RefCell<Vec<usize>>>,
        contexted: Rc<RefCell<Vec<(usize, Vec2)>>>,
    }

    impl LeadFixture {
        fn new(count: usize) -> Self {
            let mut tree = SceneTree::new();
            let root = tree.add_child(
                tree.root(),
                Flex::column()
                    .gap(0.0)
                    .padding(Edges::ZERO)
                    .mouse_filter(MouseFilter::Ignore),
            );
            let count_cell = Rc::new(Cell::new(count));
            let selected = Rc::new(Cell::new(None));
            let toggled: Rc<RefCell<Vec<usize>>> = Rc::new(RefCell::new(Vec::new()));
            let activated: Rc<RefCell<Vec<usize>>> = Rc::new(RefCell::new(Vec::new()));
            let contexted: Rc<RefCell<Vec<(usize, Vec2)>>> = Rc::new(RefCell::new(Vec::new()));
            let width_for: Rc<RefCell<Vec<f32>>> =
                Rc::new(RefCell::new((0..count).map(|i| i as f32 * 8.0).collect()));

            let toggle_for = toggled.clone();
            let activate_for = activated.clone();
            let context_for = contexted.clone();
            let list = List::new(default_theme(Mode::Light), ROW, |_| vec!["row".to_string()])
                .count(count_cell)
                .selected(selected.clone())
                .columns(vec![ListColumn::flexible()])
                .spacer(move |index| width_for.borrow().get(index).copied().unwrap_or(0.0))
                .checkboxes(
                    |_| CheckState::Unchecked,
                    move |index| toggle_for.borrow_mut().push(index),
                )
                .on_activate(move |index| activate_for.borrow_mut().push(index))
                .on_context(move |index, position| {
                    context_for.borrow_mut().push((index, position))
                });
            let state = list.state();
            tree.add_child(root, list.grow(1.0));
            let mut fixture = Self {
                tree,
                state,
                selected,
                toggled,
                activated,
                contexted,
            };
            fixture.frame();
            fixture
        }

        fn frame(&mut self) {
            let viewport = ViewportSize::new(Size::new(WIDTH, HEIGHT));
            draw_ui::layout(&mut self.tree, viewport);
            if self.state.sync(&mut self.tree) {
                draw_ui::layout(&mut self.tree, viewport);
            }
            self.tree.update();
        }

        fn lead(&self, slot: usize, index: usize) -> NodeId {
            self.tree.children(self.state.rows()[slot]).unwrap()[index]
        }

        fn click(&mut self, position: Vec2) {
            for event in [
                InputEvent::PointerDown {
                    position,
                    button: draw_core::PointerButton::Left,
                },
                InputEvent::PointerUp {
                    position,
                    button: draw_core::PointerButton::Left,
                },
            ] {
                handle_input(&mut self.tree, &event);
            }
        }

        fn right_click(&mut self, position: Vec2) {
            for event in [
                InputEvent::PointerDown {
                    position,
                    button: draw_core::PointerButton::Right,
                },
                InputEvent::PointerUp {
                    position,
                    button: draw_core::PointerButton::Right,
                },
            ] {
                handle_input(&mut self.tree, &event);
            }
        }
    }

    /// A right click reports the row's data index and the pointer position, so
    /// the caller can open a context menu at the cursor.
    #[test]
    fn a_right_click_reports_the_row_and_position() {
        let mut fixture = LeadFixture::new(100);
        let row = fixture.state.rows()[1];
        let rect = draw_ui::control(&fixture.tree, row).unwrap().rect;
        let point = Vec2::new(rect.center().x, rect.top() + 3.0);
        fixture.right_click(point);
        assert_eq!(*fixture.contexted.borrow(), vec![(1, point)]);
    }

    /// A checkbox lead owns its click: it toggles and does **not** activate the
    /// row, so checking and selecting stay separate gestures.
    #[test]
    fn a_checkbox_lead_toggles_without_activating_the_row() {
        let mut fixture = LeadFixture::new(100);
        let checkbox = fixture.lead(0, 1);
        let center = draw_ui::control(&fixture.tree, checkbox)
            .unwrap()
            .rect
            .center();

        fixture.click(center);
        assert_eq!(*fixture.toggled.borrow(), vec![0]);
        assert!(fixture.activated.borrow().is_empty(), "no row activation");
        assert_eq!(fixture.selected.get(), None, "no row selection");
    }

    /// A spacer lead is bound per row, so scrolling rebinds the indentation.
    #[test]
    fn a_spacer_lead_binds_the_row_width() {
        let mut fixture = LeadFixture::new(100);
        let width = |fixture: &LeadFixture, slot: usize| {
            draw_ui::control(&fixture.tree, fixture.lead(slot, 0))
                .unwrap()
                .min_size
                .width
        };
        assert_eq!(width(&fixture, 0), 0.0);

        fixture.state.scroll_by(3.0 * ROW);
        fixture.frame();
        assert_eq!(width(&fixture, 0), 3.0 * 8.0);
        assert_eq!(width(&fixture, 1), 4.0 * 8.0);
    }
}
