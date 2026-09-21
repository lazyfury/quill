//! Resolving control rectangles: anchors/offsets, flex and grid arrangement.
//!
//! Layout is a two-pass traversal:
//!
//! - [`Ui::measure_node`] computes intrinsic sizes bottom-up (`min`/`preferred`).
//! - [`Ui::arrange_node`] assigns absolute rectangles top-down.
//!
//! Non-container controls keep the anchor/offset model; containers own their
//! children's rectangles.

use std::collections::{HashMap, HashSet};

use super::*;
use crate::control::{
    control_mut, control_of, control_visible, resolve_clip, root_state, root_state_mut, LayoutCache,
};
use crate::layout::{
    Align, AlignContent, ContentSize, FlexStyle, GridPlacement, GridStyle, Justify, LayoutStyle,
    SizeBasis, Track,
};
use draw_core::{Rect, Size, Vec2, ViewportSize};
use draw_scene::SceneTree;

impl Ui {
    /// Resolves every control's absolute rectangle against the viewport.
    ///
    /// The supplied tree is used to locate the UI roots (controls whose parent
    /// is not itself a control, e.g. under a `CanvasLayer`). Call
    /// [`SceneTree::update`] from the host to refresh visibility/transforms.
    pub fn layout(&self, tree: &mut SceneTree, viewport: ViewportSize) {
        let (valid, last_viewport) = {
            let state = root_state_mut(tree);
            let cache = state.layout.borrow();
            (cache.valid, cache.viewport)
        };
        if valid && last_viewport == viewport {
            return;
        }
        if last_viewport != viewport {
            self.mark_all_dirty(tree);
        }

        let viewport_rect = viewport.logical_rect();
        let mut rects: HashMap<NodeId, Rect> = HashMap::new();
        {
            let state = root_state(tree).expect("root UI state");
            let cache: &mut LayoutCache = &mut state.layout.borrow_mut();
            cache.measure.clear();
            cache.last_arranged = 0;
            let dirty: HashSet<NodeId> = tree
                .iter()
                .filter(|id| control_of(tree, *id).is_some_and(|control| control.layout_dirty))
                .collect();
            let roots: Vec<NodeId> = tree
                .iter()
                .filter(|id| {
                    control_of(tree, *id).is_some()
                        && control_visible(tree, *id)
                        && tree
                            .parent(*id)
                            .map_or(true, |parent| control_of(tree, parent).is_none())
                })
                .collect();
            for root in roots {
                rects.insert(root, viewport_rect);
                for child in self.children_vec(tree, root) {
                    let child_rect = self.resolve_child_rect(tree, cache, child, viewport_rect);
                    self.arrange_node(tree, cache, child, child_rect, &mut rects, &dirty);
                }
            }
        }

        // Write the resolved rectangles back to the node slots and clear dirty
        // flags. Controls hidden at runtime are reset to a zero rect so stale
        // geometry cannot leak (they are skipped for measure/arrange/paint).
        let hidden: HashSet<NodeId> = tree
            .iter()
            .filter(|id| control_of(tree, *id).is_some() && !control_visible(tree, *id))
            .collect();

        // Clipping is opt-in and rare, so it costs nothing until something asks
        // for it: one scan to find out whether any control clips, and only then
        // the per-node clip inherited from the nearest clipping ancestor.
        //
        // `tree.iter()` is pre-order, so by the time a control is reached its
        // ancestors already carry this pass's final rectangle *and* clip — which
        // makes the inherited clip a single upward hop rather than a chain walk,
        // and keeps `clip_rect` a pure function of the resolved rectangles
        // instead of another thing `mark_dirty` has to propagate.
        let clipping = tree
            .iter()
            .any(|id| control_of(tree, id).is_some_and(|control| control.data.clip));
        for id in tree.iter().collect::<Vec<_>>() {
            let inherited = if clipping {
                inherited_clip(tree, id)
            } else {
                None
            };
            let Some(control) = control_mut(tree, id) else {
                continue;
            };
            if hidden.contains(&id) {
                control.data.rect = Rect::ZERO;
                control.data.clip_rect = None;
            } else {
                if let Some(rect) = rects.get(&id) {
                    control.data.rect = *rect;
                }
                let rect = control.data.rect;
                control.data.clip_rect = resolve_clip(rect, control.data.clip, inherited);
            }
            control.layout_dirty = false;
        }

        let mut cache = root_state_mut(tree).layout.borrow_mut();
        cache.valid = true;
        cache.viewport = viewport;
        cache.count += 1;
    }

    /// Places `id` at `rect` and arranges its subtree.
    ///
    /// When both `id` and its subtree are clean and the resolved rect is
    /// unchanged, the subtree is left untouched (partial relayout).
    fn arrange_node(
        &self,
        tree: &SceneTree,
        cache: &mut LayoutCache,
        id: NodeId,
        rect: Rect,
        out: &mut HashMap<NodeId, Rect>,
        dirty: &HashSet<NodeId>,
    ) {
        let unchanged = !dirty.contains(&id)
            && control_of(tree, id).is_some_and(|control| control.data.rect == rect);
        out.insert(id, rect);
        if unchanged {
            return;
        }
        cache.last_arranged += 1;

        let children = self.children_vec(tree, id);
        match control_of(tree, id).map(|control| &control.widget) {
            Some(Widget::Flex(style)) => {
                self.arrange_flex(tree, cache, id, rect, style, &children, out, dirty)
            }
            Some(Widget::Grid(style)) => {
                self.arrange_grid(tree, cache, id, rect, style, &children, out, dirty)
            }
            _ => {
                for child in children {
                    let child_rect = self.resolve_child_rect(tree, cache, child, rect);
                    self.arrange_node(tree, cache, child, child_rect, out, dirty);
                }
            }
        }
    }

    /// Rectangle for a child of a non-container parent, honoring anchors/offsets
    /// and falling back to intrinsic size in degenerate dimensions.
    fn resolve_child_rect(
        &self,
        tree: &SceneTree,
        cache: &mut LayoutCache,
        id: NodeId,
        parent: Rect,
    ) -> Rect {
        let Some(control) = control_of(tree, id) else {
            return parent;
        };
        let measured = self.measure_node(tree, cache, id, parent.size);
        let mut rect = control.data.resolve_rect(parent);
        if rect.size.width <= 0.0 {
            rect.size.width = measured.preferred.width.max(measured.min.width);
        }
        if rect.size.height <= 0.0 {
            rect.size.height = measured.preferred.height.max(measured.min.height);
        }
        rect.size = rect.size.max(measured.min);
        rect
    }

    // -- measure -----------------------------------------------------------

    /// The size the UI's content wants, given the space a parent can offer.
    ///
    /// [`Ui::layout`] pins every root to the viewport — a view always fills the
    /// surface it was handed — so the resolved rectangles never say how much
    /// room the content *wanted*. This runs the measure pass over the roots and
    /// reports it, which is what a host needs before it can size a window to
    /// its content (`Window::request_inner_size`).
    ///
    /// The result includes each root's own padding, and it measures against the
    /// tree's current [`TextMeasurer`], i.e. the same one that will paint the
    /// frame — a window sized from a different font than it draws with would
    /// clip. Measurement caches by `(node, available)`, and [`Ui::layout`]
    /// clears that cache, so asking here cannot disturb a later frame.
    pub fn content_size(&self, tree: &SceneTree, available: Size) -> ContentSize {
        let Some(state) = root_state(tree) else {
            return ContentSize::ZERO;
        };
        let roots: Vec<NodeId> = tree
            .iter()
            .filter(|id| {
                control_of(tree, *id).is_some()
                    && control_visible(tree, *id)
                    && tree
                        .parent(*id)
                        .map_or(true, |parent| control_of(tree, parent).is_none())
            })
            .collect();
        let mut cache = state.layout.borrow_mut();
        let mut result = ContentSize::ZERO;
        for root in roots {
            let measured = self.measure_node(tree, &mut cache, root, available);
            // Roots sit side by side, so what the UI wants is the union of what
            // they want.
            result = ContentSize {
                min: result.min.max(measured.min),
                preferred: result.preferred.max(measured.preferred),
            };
        }
        result
    }

    /// Intrinsic size of a control given the space its parent can offer.
    fn measure_node(
        &self,
        tree: &SceneTree,
        cache: &mut LayoutCache,
        id: NodeId,
        available: Size,
    ) -> ContentSize {
        let key = (id, available.width.to_bits(), available.height.to_bits());
        if let Some(cached) = cache.measure.get(&key).copied() {
            return cached;
        }

        let explicit = control_of(tree, id)
            .map(|control| control.data.min_size)
            .unwrap_or(Size::ZERO);
        let measured = match control_of(tree, id).map(|control| &control.widget) {
            Some(Widget::Flex(style)) => {
                let children = self.children_vec(tree, id);
                self.measure_flex(tree, cache, id, style, &children, available)
            }
            Some(Widget::Grid(style)) => {
                let children = self.children_vec(tree, id);
                self.measure_grid(tree, cache, id, style, &children, available)
            }
            Some(widget) => {
                let measurer: &dyn TextMeasurer = root_state(tree)
                    .map(|state| state.text_measurer.as_ref())
                    .unwrap_or(&crate::control::DEFAULT_MEASURER);
                widget.measure_with(available, measurer)
            }
            None => ContentSize::ZERO,
        };
        let min = measured.min.max(explicit);
        let result = ContentSize {
            min,
            preferred: measured.preferred.max(min),
        };
        cache.measure.insert(key, result);
        result
    }

    /// Cross-axis size of a flex child once its main size is known.
    ///
    /// A child is first measured with the container's content size (to resolve
    /// its basis). It is then measured again with that **resolved main size**, so
    /// cross content that depends on the main axis — soft-wrapped text — reports
    /// the size it will actually need rather than the size for the container.
    /// Without this, a fixed-width card measures its text at the parent's width
    /// and comes out too short once it is placed at its real (narrower) width.
    fn flex_cross_size(
        &self,
        tree: &SceneTree,
        cache: &mut LayoutCache,
        child: NodeId,
        available: Size,
        horizontal: bool,
        main: f32,
    ) -> f32 {
        let sized = if horizontal {
            Size::new(main, available.height)
        } else {
            Size::new(available.width, main)
        };
        let measured = self.measure_node(tree, cache, child, sized);
        if horizontal {
            measured.preferred.height
        } else {
            measured.preferred.width
        }
    }

    fn measure_flex(
        &self,
        tree: &SceneTree,
        cache: &mut LayoutCache,
        id: NodeId,
        style: &FlexStyle,
        children: &[NodeId],
        available: Size,
    ) -> ContentSize {
        let children = self.ordered_children(tree, cache, id, children);
        let pad = style.padding;
        let inner = Size::new(
            (available.width - pad.horizontal()).max(0.0),
            (available.height - pad.vertical()).max(0.0),
        );
        let horizontal = style.direction.is_horizontal();

        let mut pref_main = 0.0f32;
        let mut min_main = 0.0f32;
        let mut min_cross = 0.0f32;
        let mut single_cross = 0.0f32;
        // Main/cross per item, kept so a wrapping container can break them into
        // lines and report the stacked cross size (see below).
        let mut items: Vec<FlexItem> = Vec::with_capacity(children.len());

        for (index, child) in children.iter().enumerate() {
            let measured = self.measure_node(tree, cache, *child, inner);
            let layout = self.layout_style(tree, *child);
            let (p_main, c_min_main, c_min_cross) = if horizontal {
                (
                    measured.preferred.width,
                    measured.min.width,
                    measured.min.height,
                )
            } else {
                (
                    measured.preferred.height,
                    measured.min.height,
                    measured.min.width,
                )
            };
            let basis =
                resolve_basis(layout.basis, p_main, inner_main(horizontal, inner)).max(c_min_main);
            let p_cross = self.flex_cross_size(tree, cache, *child, inner, horizontal, basis);
            let gap = if index == 0 { 0.0 } else { style.gap };
            pref_main += basis + gap;
            // A wrapping flex can break between items, so its minimum main size is
            // the widest item, not the sum of one line (otherwise a wrapped grid
            // would force its container as wide as a single unbounded row).
            if style.wrap {
                min_main = min_main.max(c_min_main);
            } else {
                min_main += c_min_main + gap;
            }
            single_cross = single_cross.max(p_cross);
            min_cross = min_cross.max(c_min_cross);
            items.push(FlexItem {
                id: *child,
                main: basis,
                base: basis,
                min: c_min_main,
                cross: p_cross,
                grow: layout.grow,
                shrink: layout.shrink,
                align: layout.align_self.unwrap_or(style.align),
            });
        }

        // A wrapping container's cross size is the stacked height of its lines, not
        // the tallest single line. Otherwise a row that wraps keeps one line's
        // height and overlaps the sibling below it.
        let pref_cross = if style.wrap && !items.is_empty() {
            let lines = wrap_lines(&items, inner_main(horizontal, inner), style.gap);
            let stacked: f32 = lines
                .iter()
                .map(|line| line.iter().map(|i| items[*i].cross).fold(0.0f32, f32::max))
                .sum();
            stacked + style.cross_gap * lines.len().saturating_sub(1) as f32
        } else {
            single_cross
        };

        let (width, height) = if horizontal {
            (pref_main + pad.horizontal(), pref_cross + pad.vertical())
        } else {
            (pref_cross + pad.horizontal(), pref_main + pad.vertical())
        };
        let (min_w, min_h) = if horizontal {
            (min_main + pad.horizontal(), min_cross + pad.vertical())
        } else {
            (min_cross + pad.horizontal(), min_main + pad.vertical())
        };
        ContentSize::new(Size::new(min_w, min_h), Size::new(width, height))
    }

    fn measure_grid(
        &self,
        tree: &SceneTree,
        cache: &mut LayoutCache,
        id: NodeId,
        style: &GridStyle,
        children: &[NodeId],
        available: Size,
    ) -> ContentSize {
        let children = self.ordered_children(tree, cache, id, children);
        let pad = style.padding;
        let inner = Size::new(
            (available.width - pad.horizontal()).max(0.0),
            (available.height - pad.vertical()).max(0.0),
        );
        let columns = style.columns.len().max(1);
        let rows = children.len().div_ceil(columns).max(style.rows.len());

        let mut auto_cols = vec![0.0f32; columns];
        let mut auto_rows = vec![0.0f32; rows];
        for (index, child) in children.iter().enumerate() {
            let measured = self.measure_node(tree, cache, *child, inner);
            let col = index % columns;
            let row = index / columns;
            auto_cols[col] = auto_cols[col].max(measured.preferred.width);
            auto_rows[row] = auto_rows[row].max(measured.preferred.height);
        }

        let col_widths = resolve_tracks(
            &style.columns,
            columns,
            style.column_gap,
            inner.width,
            &auto_cols,
        );
        let row_heights =
            resolve_tracks(&style.rows, rows, style.row_gap, inner.height, &auto_rows);
        let width = col_widths.iter().sum::<f32>()
            + style.column_gap * columns.saturating_sub(1) as f32
            + pad.horizontal();
        let height = row_heights.iter().sum::<f32>()
            + style.row_gap * rows.saturating_sub(1) as f32
            + pad.vertical();
        ContentSize::new(Size::new(width, height), Size::new(width, height))
    }

    fn layout_style(&self, tree: &SceneTree, id: NodeId) -> LayoutStyle {
        control_of(tree, id)
            .map(|control| control.data.layout)
            .unwrap_or_default()
    }

    /// Children in paint/placement order (`LayoutStyle::order`, stable ties).
    ///
    /// The sorted list is cached per container and invalidated by
    /// [`mark_dirty`](Ui::mark_dirty) / [`mark_all_dirty`](Ui::mark_all_dirty).
    fn ordered_children(
        &self,
        tree: &SceneTree,
        cache: &mut LayoutCache,
        id: NodeId,
        children: &[NodeId],
    ) -> Vec<NodeId> {
        if let Some(cached) = cache.order.get(&id) {
            if cached.len() == children.len() {
                return cached.clone();
            }
        }
        let mut ordered = children.to_vec();
        ordered.sort_by_key(|child| self.layout_style(tree, *child).order);
        cache.order.insert(id, ordered.clone());
        ordered
    }

    // -- flex --------------------------------------------------------------

    fn arrange_flex(
        &self,
        tree: &SceneTree,
        cache: &mut LayoutCache,
        id: NodeId,
        rect: Rect,
        style: &FlexStyle,
        children: &[NodeId],
        out: &mut HashMap<NodeId, Rect>,
        dirty: &HashSet<NodeId>,
    ) {
        let pad = style.padding;
        let content = Rect::from_min_size(
            Vec2::new(rect.left() + pad.left, rect.top() + pad.top),
            Size::new(
                (rect.size.width - pad.horizontal()).max(0.0),
                (rect.size.height - pad.vertical()).max(0.0),
            ),
        );
        let horizontal = style.direction.is_horizontal();
        let content_main = inner_main(horizontal, content.size);
        let content_cross = inner_cross(horizontal, content.size);
        let children = self.ordered_children(tree, cache, id, children);

        // Measure children and resolve their main-axis basis.
        let mut items: Vec<FlexItem> = Vec::with_capacity(children.len());
        for child in &children {
            let measured = self.measure_node(tree, cache, *child, content.size);
            let layout = self.layout_style(tree, *child);
            let (p_main, min_main) = if horizontal {
                (measured.preferred.width, measured.min.width)
            } else {
                (measured.preferred.height, measured.min.height)
            };
            let basis = resolve_basis(layout.basis, p_main, content_main).max(min_main);
            let p_cross =
                self.flex_cross_size(tree, cache, *child, content.size, horizontal, basis);
            items.push(FlexItem {
                id: *child,
                main: basis,
                base: basis,
                min: min_main,
                cross: p_cross,
                grow: layout.grow,
                shrink: layout.shrink,
                align: layout.align_self.unwrap_or(style.align),
            });
        }
        if style.direction.is_reverse() {
            items.reverse();
        }

        // Partition into wrap lines, or a single line when wrapping is off.
        let lines = if style.wrap {
            wrap_lines(&items, content_main, style.gap)
        } else {
            vec![(0..items.len()).collect()]
        };

        // Cross-axis placement of the lines themselves (align-content).
        let line_crosses: Vec<f32> = lines
            .iter()
            .map(|line| line.iter().map(|i| items[*i].cross).fold(0.0f32, f32::max))
            .collect();
        let line_sizes = layout_lines(
            style.align_content,
            content_cross,
            &line_crosses,
            style.cross_gap,
        );

        for (line_index, line) in lines.iter().enumerate() {
            let (line_offset, line_cross) = line_sizes[line_index];
            // A definite cross size caps stretch: `Stretch` items fill the
            // container instead of growing it when their min-content is
            // wider/taller (e.g. a long unbreakable word).
            let stretch_cross = line_cross.min(content_cross);
            flex_line_sizes(&mut items, line, content_main, style.gap);

            let used: f32 = line.iter().map(|i| items[*i].main).sum::<f32>()
                + style.gap * line.len().saturating_sub(1) as f32;
            let leftover = (content_main - used).max(0.0);
            let (mut cursor, extra_gap) = justify_offset(style.justify, leftover, line.len());

            for &index in line {
                let item = &items[index];
                let cross_size = if item.align == Align::Stretch {
                    stretch_cross
                } else {
                    item.cross.min(line_cross)
                };
                let cross_off = align_offset(item.align, line_cross - cross_size);
                let (x, y, w, h) = if horizontal {
                    (
                        content.left() + cursor,
                        content.top() + line_offset + cross_off,
                        item.main,
                        cross_size,
                    )
                } else {
                    (
                        content.left() + line_offset + cross_off,
                        content.top() + cursor,
                        cross_size,
                        item.main,
                    )
                };
                let child_rect = Rect::from_min_size(Vec2::new(x, y), Size::new(w, h));
                self.arrange_node(tree, cache, item.id, child_rect, out, dirty);
                cursor += item.main + style.gap + extra_gap;
            }
        }
    }

    // -- grid --------------------------------------------------------------

    fn arrange_grid(
        &self,
        tree: &SceneTree,
        cache: &mut LayoutCache,
        id: NodeId,
        rect: Rect,
        style: &GridStyle,
        children: &[NodeId],
        out: &mut HashMap<NodeId, Rect>,
        dirty: &HashSet<NodeId>,
    ) {
        let pad = style.padding;
        let content = Rect::from_min_size(
            Vec2::new(rect.left() + pad.left, rect.top() + pad.top),
            Size::new(
                (rect.size.width - pad.horizontal()).max(0.0),
                (rect.size.height - pad.vertical()).max(0.0),
            ),
        );
        let children = self.ordered_children(tree, cache, id, children);
        let columns = style.columns.len().max(1);
        let placements = resolve_placements(&children, columns, tree);

        let rows = placements
            .iter()
            .map(|p| p.row + p.row_span)
            .max()
            .unwrap_or(1)
            .max(style.rows.len());

        // Intrinsic preferred size of every child.
        let measured: Vec<Size> = children
            .iter()
            .map(|child| {
                self.measure_node(tree, cache, *child, content.size)
                    .preferred
            })
            .collect();

        // Which tracks are auto-sized (eligible for span demand / stretch).
        let col_auto: Vec<bool> = (0..columns)
            .map(|i| is_auto_track(style.columns.get(i).copied().unwrap_or(Track::Auto)))
            .collect();
        let row_auto: Vec<bool> = (0..rows)
            .map(|i| is_auto_track(style.rows.get(i).copied().unwrap_or(Track::Auto)))
            .collect();

        let mut auto_cols = vec![0.0f32; columns];
        let mut auto_rows = vec![0.0f32; rows];
        for (index, size) in measured.iter().enumerate() {
            let placement = placements[index];
            if placement.column_span == 1 && placement.column < columns {
                auto_cols[placement.column] = auto_cols[placement.column].max(size.width);
            }
            if placement.row_span == 1 && placement.row < rows {
                auto_rows[placement.row] = auto_rows[placement.row].max(size.height);
            }
        }

        let fixed_cols = fixed_tracks(&style.columns, columns, content.size.width);
        let fixed_rows = fixed_tracks(&style.rows, rows, content.size.height);
        distribute_span_demands(
            &placements,
            &measured,
            &fixed_cols,
            &mut auto_cols,
            &col_auto,
            style.column_gap,
            &fixed_rows,
            &mut auto_rows,
            &row_auto,
            style.row_gap,
        );

        let col_widths = resolve_tracks(
            &style.columns,
            columns,
            style.column_gap,
            content.size.width,
            &auto_cols,
        );
        let row_heights = resolve_tracks(
            &style.rows,
            rows,
            style.row_gap,
            content.size.height,
            &auto_rows,
        );

        let col_offset = track_offsets(&col_widths, style.column_gap, content.left());
        let row_lines = layout_tracks(
            style.align_content,
            content.size.height,
            &row_heights,
            style.row_gap,
            &row_auto,
        );

        for (index, child) in children.iter().enumerate() {
            let placement = placements[index];
            let col = placement.column.min(columns.saturating_sub(1));
            let row = placement.row.min(rows.saturating_sub(1));
            let col_end = (col + placement.column_span.max(1)).min(columns);
            let row_end = (row + placement.row_span.max(1)).min(rows);

            let cell_x = col_offset[col];
            let cell_y = content.top() + row_lines[row].0;
            let cell_w = span_size(&col_widths, col, col_end, style.column_gap);
            let cell_h = span_from_lines(&row_lines, row, row_end, style.row_gap);

            let preferred = measured[index];
            let width = if style.justify_items == Align::Stretch {
                cell_w
            } else {
                preferred.width.min(cell_w)
            };
            let height = if style.align_items == Align::Stretch {
                cell_h
            } else {
                preferred.height.min(cell_h)
            };
            let x = cell_x + align_offset(style.justify_items, cell_w - width);
            let y = cell_y + align_offset(style.align_items, cell_h - height);

            let child_rect = Rect::from_min_size(Vec2::new(x, y), Size::new(width, height));
            self.arrange_node(tree, cache, *child, child_rect, out, dirty);
        }
    }
}

/// The clip rectangle a node inherits from the nearest control above it.
///
/// Non-control nodes (the tree root, a `CanvasLayer`) carry no clip state, so
/// the walk steps past them; the first control it finds was already resolved by
/// the same pre-order pass.
fn inherited_clip(tree: &SceneTree, id: NodeId) -> Option<Rect> {
    let mut current = tree.parent(id);
    while let Some(node) = current {
        if let Some(control) = control_of(tree, node) {
            return control.data.clip_rect;
        }
        current = tree.parent(node);
    }
    None
}

/// A child during flex arrangement.
struct FlexItem {
    id: NodeId,
    main: f32,
    base: f32,
    /// Lower bound along the main axis (content minimum).
    min: f32,
    cross: f32,
    grow: f32,
    shrink: f32,
    align: Align,
}

fn inner_main(horizontal: bool, size: Size) -> f32 {
    if horizontal {
        size.width
    } else {
        size.height
    }
}

fn inner_cross(horizontal: bool, size: Size) -> f32 {
    if horizontal {
        size.height
    } else {
        size.width
    }
}

fn resolve_basis(basis: SizeBasis, auto_main: f32, content_main: f32) -> f32 {
    match basis {
        SizeBasis::Auto => auto_main,
        SizeBasis::Px(value) => value,
        SizeBasis::Percent(fraction) => content_main * fraction,
    }
}

/// Greedily partitions items into main-axis lines when wrapping is enabled.
fn wrap_lines(items: &[FlexItem], content_main: f32, gap: f32) -> Vec<Vec<usize>> {
    let mut lines: Vec<Vec<usize>> = Vec::new();
    let mut current: Vec<usize> = Vec::new();
    let mut current_main = 0.0f32;
    for (index, item) in items.iter().enumerate() {
        let add = item.main + if current.is_empty() { 0.0 } else { gap };
        if !current.is_empty() && current_main + add > content_main {
            lines.push(std::mem::take(&mut current));
            current_main = 0.0;
        }
        current_main += if current.is_empty() {
            item.main
        } else {
            item.main + gap
        };
        current.push(index);
    }
    lines.push(current);
    lines
}

/// Applies grow/shrink to the items of one line, in place.
fn flex_line_sizes(items: &mut [FlexItem], line: &[usize], content_main: f32, gap: f32) {
    let used: f32 = line.iter().map(|i| items[*i].base).sum::<f32>()
        + gap * line.len().saturating_sub(1) as f32;
    let free = content_main - used;

    if free > 0.0 {
        let total_grow: f32 = line.iter().map(|i| items[*i].grow).sum();
        if total_grow > 0.0 {
            for &i in line {
                items[i].main = items[i].base + free * items[i].grow / total_grow;
            }
        } else {
            for &i in line {
                items[i].main = items[i].base;
            }
        }
    } else if free < 0.0 {
        let total_shrink: f32 = line.iter().map(|i| items[*i].shrink * items[*i].base).sum();
        if total_shrink > 0.0 {
            let deficit = -free;
            for &i in line {
                let share = deficit * items[i].shrink * items[i].base / total_shrink;
                items[i].main = (items[i].base - share).max(items[i].min);
            }
        } else {
            for &i in line {
                items[i].main = items[i].base;
            }
        }
    } else {
        for &i in line {
            items[i].main = items[i].base;
        }
    }
}

fn justify_offset(justify: Justify, leftover: f32, count: usize) -> (f32, f32) {
    match justify {
        Justify::Start => (0.0, 0.0),
        Justify::Center => (leftover / 2.0, 0.0),
        Justify::End => (leftover, 0.0),
        Justify::SpaceBetween => {
            if count > 1 {
                (0.0, leftover / (count - 1) as f32)
            } else {
                (0.0, 0.0)
            }
        }
        Justify::SpaceAround => {
            if count > 0 {
                let gap = leftover / count as f32;
                (gap / 2.0, gap)
            } else {
                (0.0, 0.0)
            }
        }
        Justify::SpaceEvenly => {
            let gap = leftover / (count + 1) as f32;
            (gap, gap)
        }
    }
}

fn align_offset(align: Align, free: f32) -> f32 {
    match align {
        Align::Start | Align::Stretch => 0.0,
        Align::Center => free / 2.0,
        Align::End => free,
    }
}

/// Places flex wrap lines along the cross axis (`align-content`).
/// Every line may stretch (flex lines are all stretchable).
fn layout_lines(align: AlignContent, extent: f32, sizes: &[f32], gap: f32) -> Vec<(f32, f32)> {
    let mask = vec![true; sizes.len()];
    layout_tracks(align, extent, sizes, gap, &mask)
}

/// Places tracks along an axis, returning `(offset, size)` per track.
///
/// `stretchable` marks tracks that `AlignContent::Stretch` may grow
/// (auto tracks in a grid; every line in a flex container).
fn layout_tracks(
    align: AlignContent,
    extent: f32,
    sizes: &[f32],
    gap: f32,
    stretchable: &[bool],
) -> Vec<(f32, f32)> {
    let count = sizes.len();
    if count == 0 {
        return Vec::new();
    }
    let total_gap = gap * count.saturating_sub(1) as f32;
    let mut track = sizes.to_vec();

    if align == AlignContent::Stretch {
        let total: f32 = track.iter().sum::<f32>() + total_gap;
        let leftover = (extent - total).max(0.0);
        let stretchable_count = stretchable.iter().filter(|flag| **flag).count();
        if leftover > 0.0 && stretchable_count > 0 {
            let add = leftover / stretchable_count as f32;
            for (index, size) in track.iter_mut().enumerate() {
                if stretchable.get(index).copied().unwrap_or(false) {
                    *size += add;
                }
            }
        }
    }

    let total: f32 = track.iter().sum::<f32>() + total_gap;
    let leftover = (extent - total).max(0.0);
    let (offset, extra) = match align {
        AlignContent::Start | AlignContent::Stretch => (0.0, 0.0),
        AlignContent::Center => (leftover / 2.0, 0.0),
        AlignContent::End => (leftover, 0.0),
        AlignContent::SpaceBetween => {
            if count > 1 {
                (0.0, leftover / (count - 1) as f32)
            } else {
                (0.0, 0.0)
            }
        }
        AlignContent::SpaceAround => {
            let extra = leftover / count as f32;
            (extra / 2.0, extra)
        }
        AlignContent::SpaceEvenly => {
            let extra = leftover / (count + 1) as f32;
            (extra, extra)
        }
    };

    let mut cursor = offset;
    let mut out = Vec::with_capacity(count);
    for size in &track {
        out.push((cursor, *size));
        cursor += *size + gap + extra;
    }
    out
}

fn is_auto_track(track: Track) -> bool {
    matches!(track, Track::Auto)
}

/// Fixed (`Px`/`Percent`) contribution of each track. Auto/`Fr` are `0.0`.
fn fixed_tracks(tracks: &[Track], count: usize, extent: f32) -> Vec<f32> {
    (0..count)
        .map(
            |index| match tracks.get(index).copied().unwrap_or(Track::Auto) {
                Track::Px(value) => value,
                Track::Percent(fraction) => extent * fraction,
                Track::Auto | Track::Fr(_) => 0.0,
            },
        )
        .collect()
}

/// Grows auto tracks so a spanning item's preferred size fits its span.
#[allow(clippy::too_many_arguments)]
fn distribute_span_demands(
    placements: &[GridPlacement],
    measured: &[Size],
    fixed_cols: &[f32],
    auto_cols: &mut [f32],
    col_auto: &[bool],
    column_gap: f32,
    fixed_rows: &[f32],
    auto_rows: &mut [f32],
    row_auto: &[bool],
    row_gap: f32,
) {
    let columns = auto_cols.len();
    let rows = auto_rows.len();
    for (index, placement) in placements.iter().enumerate() {
        let size = measured[index];

        let col_start = placement.column.min(columns);
        let col_end = (col_start + placement.column_span.max(1)).min(columns);
        if col_end > col_start && placement.column_span > 1 {
            let fixed: f32 = fixed_cols[col_start..col_end].iter().sum();
            let auto: f32 = auto_cols[col_start..col_end].iter().sum();
            let gaps = column_gap * (col_end - col_start - 1) as f32;
            let deficit = size.width - (fixed + auto + gaps);
            let auto_count = (col_start..col_end)
                .filter(|c| col_auto.get(*c).copied().unwrap_or(false))
                .count();
            if deficit > 0.0 && auto_count > 0 {
                let add = deficit / auto_count as f32;
                for col in col_start..col_end {
                    if col_auto.get(col).copied().unwrap_or(false) {
                        auto_cols[col] += add;
                    }
                }
            }
        }

        let row_start = placement.row.min(rows);
        let row_end = (row_start + placement.row_span.max(1)).min(rows);
        if row_end > row_start && placement.row_span > 1 {
            let fixed: f32 = fixed_rows[row_start..row_end].iter().sum();
            let auto: f32 = auto_rows[row_start..row_end].iter().sum();
            let gaps = row_gap * (row_end - row_start - 1) as f32;
            let deficit = size.height - (fixed + auto + gaps);
            let auto_count = (row_start..row_end)
                .filter(|r| row_auto.get(*r).copied().unwrap_or(false))
                .count();
            if deficit > 0.0 && auto_count > 0 {
                let add = deficit / auto_count as f32;
                for row in row_start..row_end {
                    if row_auto.get(row).copied().unwrap_or(false) {
                        auto_rows[row] += add;
                    }
                }
            }
        }
    }
}

/// Size of a grid span from resolved `(offset, size)` tracks.
fn span_from_lines(lines: &[(f32, f32)], start: usize, end: usize, gap: f32) -> f32 {
    let count = end.saturating_sub(start);
    if count == 0 {
        return 0.0;
    }
    let sum: f32 = lines[start..end].iter().map(|(_, size)| *size).sum();
    sum + gap * (count - 1) as f32
}

fn resolve_placements(children: &[NodeId], columns: usize, tree: &SceneTree) -> Vec<GridPlacement> {
    let mut placements: Vec<Option<GridPlacement>> = children
        .iter()
        .map(|child| control_of(tree, *child).and_then(|control| control.data.layout.grid))
        .collect();

    let mut occupied: HashSet<(usize, usize)> = HashSet::new();
    for placement in placements.iter().flatten() {
        let end_col = placement.column + placement.column_span.max(1);
        let end_row = placement.row + placement.row_span.max(1);
        for row in placement.row..end_row {
            for col in placement.column..end_col {
                occupied.insert((row, col));
            }
        }
    }

    let mut cursor_row = 0usize;
    let mut cursor_col = 0usize;
    for slot in placements.iter_mut() {
        if slot.is_some() {
            continue;
        }
        loop {
            if cursor_col >= columns {
                cursor_col = 0;
                cursor_row += 1;
            }
            if !occupied.contains(&(cursor_row, cursor_col)) {
                break;
            }
            cursor_col += 1;
        }
        *slot = Some(GridPlacement::new(cursor_col, cursor_row));
        occupied.insert((cursor_row, cursor_col));
        cursor_col += 1;
    }

    placements.into_iter().flatten().collect()
}

/// Resolves track sizes: `Px`/`Percent`/`Auto` first, then distributes the
/// remaining space among `Fr` tracks.
fn resolve_tracks(tracks: &[Track], count: usize, gap: f32, extent: f32, auto: &[f32]) -> Vec<f32> {
    let mut sizes = vec![0.0f32; count];
    let mut used = 0.0f32;
    let mut fr_total = 0.0f32;

    for (index, size) in sizes.iter_mut().enumerate() {
        match tracks.get(index).copied().unwrap_or(Track::Auto) {
            Track::Px(value) => {
                *size = value;
                used += value;
            }
            Track::Percent(fraction) => {
                let value = extent * fraction;
                *size = value;
                used += value;
            }
            Track::Auto => {
                let value = auto.get(index).copied().unwrap_or(0.0);
                *size = value;
                used += value;
            }
            Track::Fr(fraction) => fr_total += fraction,
        }
    }

    let gaps = gap * count.saturating_sub(1) as f32;
    let remaining = (extent - used - gaps).max(0.0);
    if fr_total > 0.0 {
        for (index, size) in sizes.iter_mut().enumerate() {
            if let Track::Fr(fraction) = tracks.get(index).copied().unwrap_or(Track::Auto) {
                *size = remaining * fraction / fr_total;
            }
        }
    }
    sizes
}

fn track_offsets(sizes: &[f32], gap: f32, origin: f32) -> Vec<f32> {
    let mut offsets = Vec::with_capacity(sizes.len());
    let mut cursor = origin;
    for (index, size) in sizes.iter().enumerate() {
        if index > 0 {
            cursor += gap;
        }
        offsets.push(cursor);
        cursor += size;
    }
    offsets
}

fn span_size(sizes: &[f32], start: usize, end: usize, gap: f32) -> f32 {
    let count = end.saturating_sub(start);
    if count == 0 {
        return 0.0;
    }
    let sum: f32 = sizes[start..end].iter().sum();
    sum + gap * (count - 1) as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::control::Control;
    use crate::widget::Widget;
    use draw_core::{Color, Edges};
    use std::rc::Rc;

    fn add(tree: &mut SceneTree, parent: NodeId, data: ControlData, widget: Widget) -> NodeId {
        let id = tree.add_control(parent, "test");
        tree.set_data(id, Control::new(data, widget));
        id
    }

    fn panel(color: Color) -> Widget {
        Widget::Panel {
            color,
            border: None,
        }
    }

    /// Anchored child at an absolute-ish rectangle (anchors at the parent's
    /// origin, offsets in pixels) — the model the list positions its rows with.
    fn slab(tree: &mut SceneTree, parent: NodeId, min: Vec2, max: Vec2) -> NodeId {
        add(
            tree,
            parent,
            ControlData {
                anchors: Edges::ZERO,
                offsets: Edges::new(min.x, min.y, max.x, max.y),
                ..ControlData::default()
            },
            panel(Color::RED),
        )
    }

    #[test]
    fn a_clipping_control_hands_its_rect_to_its_subtree_only() {
        let mut tree = SceneTree::new();
        let tree_root = tree.root();
        let root = add(
            &mut tree,
            tree_root,
            ControlData::fill_parent(),
            panel(Color::TRANSPARENT),
        );
        let clipper = add(
            &mut tree,
            root,
            ControlData {
                anchors: Edges::ZERO,
                offsets: Edges::new(10.0, 10.0, 110.0, 60.0),
                clip: true,
                ..ControlData::default()
            },
            panel(Color::TRANSPARENT),
        );
        // Overflows its clipping parent on both axes.
        let inside = slab(
            &mut tree,
            clipper,
            Vec2::new(0.0, 0.0),
            Vec2::new(300.0, 300.0),
        );
        let outside = slab(
            &mut tree,
            root,
            Vec2::new(150.0, 0.0),
            Vec2::new(200.0, 40.0),
        );

        crate::layout(&mut tree, ViewportSize::new(Size::new(200.0, 100.0)));
        let clipped = Rect::from_min_size(Vec2::new(10.0, 10.0), Size::new(100.0, 50.0));

        assert_eq!(crate::control(&tree, clipper).unwrap().rect, clipped);
        assert_eq!(
            crate::control(&tree, clipper).unwrap().clip_rect,
            Some(clipped)
        );
        assert_eq!(
            crate::control(&tree, inside).unwrap().clip_rect,
            Some(clipped),
            "the subtree inherits the clip"
        );
        assert_eq!(
            crate::control(&tree, outside).unwrap().clip_rect,
            None,
            "a sibling is not clipped by its uncle"
        );
        assert_eq!(
            crate::control(&tree, root).unwrap().clip_rect,
            None,
            "and the tree above knows nothing about it"
        );
    }

    #[test]
    fn nested_clips_intersect_and_a_disjoint_one_collapses() {
        let mut tree = SceneTree::new();
        let tree_root = tree.root();
        let root = add(
            &mut tree,
            tree_root,
            ControlData::fill_parent(),
            panel(Color::TRANSPARENT),
        );
        let outer = add(
            &mut tree,
            root,
            ControlData {
                anchors: Edges::ZERO,
                offsets: Edges::new(0.0, 0.0, 100.0, 100.0),
                clip: true,
                ..ControlData::default()
            },
            panel(Color::TRANSPARENT),
        );
        let inner = add(
            &mut tree,
            outer,
            ControlData {
                anchors: Edges::ZERO,
                offsets: Edges::new(20.0, 20.0, 60.0, 60.0),
                clip: true,
                ..ControlData::default()
            },
            panel(Color::TRANSPARENT),
        );
        let deep = slab(&mut tree, inner, Vec2::new(0.0, 0.0), Vec2::new(10.0, 10.0));
        let beside = slab(
            &mut tree,
            outer,
            Vec2::new(80.0, 0.0),
            Vec2::new(90.0, 10.0),
        );
        let away = slab(
            &mut tree,
            outer,
            Vec2::new(500.0, 500.0),
            Vec2::new(600.0, 600.0),
        );
        // Nothing of this one's subtree can ever be seen.
        let far_clipper = add(
            &mut tree,
            outer,
            ControlData {
                anchors: Edges::ZERO,
                offsets: Edges::new(500.0, 500.0, 600.0, 600.0),
                clip: true,
                ..ControlData::default()
            },
            panel(Color::TRANSPARENT),
        );

        crate::layout(&mut tree, ViewportSize::new(Size::new(400.0, 400.0)));

        assert_eq!(
            crate::control(&tree, deep).unwrap().clip_rect,
            Some(Rect::from_min_size(
                Vec2::new(20.0, 20.0),
                Size::new(40.0, 40.0)
            )),
            "the inner clip wins where the two overlap"
        );
        assert_eq!(
            crate::control(&tree, beside).unwrap().clip_rect,
            Some(Rect::from_min_size(Vec2::ZERO, Size::new(100.0, 100.0))),
            "what is inside the outer clip keeps the outer clip"
        );
        assert_eq!(
            crate::control(&tree, away).unwrap().clip_rect,
            Some(Rect::from_min_size(Vec2::ZERO, Size::new(100.0, 100.0))),
            "a control sitting outside the clip is still *under* it — the backend \
             scissors it away and hit-testing rejects it, but nothing about the \
             control itself changed"
        );
        assert!(
            crate::control(&tree, far_clipper)
                .unwrap()
                .clip_rect
                .unwrap()
                .is_empty(),
            "a clipper whose own rectangle misses the inherited clip is clipped \
             away entirely, and paints nothing"
        );
    }

    #[test]
    fn turning_the_clip_off_gives_the_subtree_back_its_rectangle() {
        let mut tree = SceneTree::new();
        let tree_root = tree.root();
        let root = add(
            &mut tree,
            tree_root,
            ControlData::fill_parent(),
            panel(Color::TRANSPARENT),
        );
        let clipper = add(
            &mut tree,
            root,
            ControlData {
                anchors: Edges::ZERO,
                offsets: Edges::new(0.0, 0.0, 50.0, 50.0),
                clip: true,
                ..ControlData::default()
            },
            panel(Color::TRANSPARENT),
        );
        let child = slab(
            &mut tree,
            clipper,
            Vec2::new(0.0, 0.0),
            Vec2::new(10.0, 10.0),
        );
        let viewport = ViewportSize::new(Size::new(200.0, 200.0));
        crate::layout(&mut tree, viewport);
        assert!(crate::control(&tree, child).unwrap().clip_rect.is_some());

        crate::set_clip(&mut tree, clipper, false);
        crate::layout(&mut tree, viewport);
        assert_eq!(crate::control(&tree, child).unwrap().clip_rect, None);
        assert_eq!(crate::control(&tree, clipper).unwrap().clip_rect, None);
    }

    /// `layout` pins the root to the viewport — a view always fills the window
    /// it was handed — so nothing in the resolved rectangles says how much room
    /// the content *wanted*. `content_size` answers exactly that, out of the
    /// same measure pass, which is what lets a host size a window to its
    /// content (`Window::request_inner_size`).
    #[test]
    fn content_size_reports_what_the_content_wants_not_the_viewport() {
        let mut tree = SceneTree::new();
        let tree_root = tree.root();
        // Measuring reads the tree's UI state and its text measurer, both of
        // which a `draw_ui`-built view already has. A bare `SceneTree` has to
        // be told — exactly what a host does when it installs its font, and it
        // does so *before* the first layout.
        crate::set_text_measurer(&mut tree, Rc::new(crate::ApproxTextMeasurer::default()));
        // The shape `draw_components` views are built in: the root is arranged
        // against the viewport and the column below it is the content.
        let root = add(
            &mut tree,
            tree_root,
            ControlData::fill_parent(),
            Widget::Flex(FlexStyle::column()),
        );
        let style = FlexStyle {
            padding: Edges::new(10.0, 20.0, 10.0, 20.0),
            gap: 5.0,
            ..FlexStyle::column()
        };
        let content = add(
            &mut tree,
            root,
            ControlData::fill_parent(),
            Widget::Flex(style),
        );
        for basis in [40.0, 60.0] {
            let mut data = ControlData::default();
            data.layout.basis = SizeBasis::Px(basis);
            add(&mut tree, content, data, panel(Color::RED));
        }

        // Two blocks of 40 and 60, one gap between them, and the content
        // column's own 20+20 of padding: that column wants 145. The root adds
        // its own default 16+16 on top — padding the content cannot see and a
        // host could not recover from the resolved rectangles.
        let wanted = crate::content_size(&tree, Size::new(300.0, 1_000.0));
        assert_eq!(wanted.preferred.height, 177.0, "145 of content, 32 of root");
        assert_eq!(wanted.min.height, 77.0, "the padding and gap cannot shrink");
        assert_eq!(wanted.preferred.width, 52.0, "nothing but padding is wide");

        // The layout itself is unaffected: 145 points of content in an 800-tall
        // viewport is still an 800-tall root, which is the whole reason a host
        // cannot read the content's size off the tree.
        let viewport = ViewportSize::new(Size::new(300.0, 800.0));
        crate::layout(&mut tree, viewport);
        assert_eq!(
            crate::control(&tree, root).unwrap().rect,
            viewport.logical_rect()
        );
    }

    #[test]
    fn hidden_controls_free_their_flex_space() {
        let mut tree = SceneTree::new();
        let tree_root = tree.root();
        // A top-level panel; the flex row lives under it (the layout entry
        // arranges a root's children by anchors, so the container is a child).
        let root = add(
            &mut tree,
            tree_root,
            ControlData::fill_parent(),
            panel(Color::TRANSPARENT),
        );
        let style = FlexStyle {
            padding: Edges::ZERO,
            gap: 0.0,
            ..FlexStyle::row()
        };
        let flex = add(
            &mut tree,
            root,
            ControlData::fill_parent(),
            Widget::Flex(style),
        );
        let mut a_data = ControlData::default();
        a_data.layout.grow = 1.0;
        let a = add(&mut tree, flex, a_data, panel(Color::RED));
        let mut b_data = ControlData::default();
        b_data.layout.basis = SizeBasis::Px(100.0);
        b_data.layout.shrink = 0.0;
        let b = add(&mut tree, flex, b_data, panel(Color::BLUE));

        let viewport = ViewportSize::new(Size::new(200.0, 100.0));
        crate::layout(&mut tree, viewport);
        assert_eq!(crate::control(&tree, a).unwrap().rect.size.width, 100.0);
        assert_eq!(crate::control(&tree, b).unwrap().rect.size.width, 100.0);

        // Hiding `b` drops it from the flex line, so `a` grows into its space.
        tree.set_visible(b, false);
        crate::mark_dirty(&mut tree, b);
        crate::layout(&mut tree, viewport);
        assert_eq!(crate::control(&tree, a).unwrap().rect.size.width, 200.0);
        assert_eq!(
            crate::control(&tree, b).unwrap().rect,
            draw_core::Rect::ZERO
        );
    }

    #[test]
    fn an_invisible_root_is_not_laid_out() {
        let mut tree = SceneTree::new();
        let tree_root = tree.root();
        let root = add(
            &mut tree,
            tree_root,
            ControlData::fill_parent(),
            panel(Color::RED),
        );
        let viewport = ViewportSize::new(Size::new(100.0, 100.0));
        crate::layout(&mut tree, viewport);
        assert_eq!(crate::control(&tree, root).unwrap().rect.size.width, 100.0);

        tree.set_visible(root, false);
        crate::invalidate_layout(&mut tree);
        crate::layout(&mut tree, viewport);
        // A hidden root receives no rect (it is skipped entirely).
        assert_eq!(
            crate::control(&tree, root).unwrap().rect,
            draw_core::Rect::ZERO
        );
    }

    #[test]
    fn a_wrapping_flex_min_width_is_the_widest_item() {
        // A row of three 20px items: it may break between items, so its minimum
        // width is one item, not the single-line sum (otherwise a wrapped grid
        // would force its container as wide as an unbounded row).
        let mut tree = SceneTree::new();
        let tree_root = tree.root();
        let root = add(
            &mut tree,
            tree_root,
            ControlData::fill_parent(),
            Widget::Flex(FlexStyle::row().wrap(true)),
        );
        for _ in 0..3 {
            let mut data = ControlData::default();
            data.min_size = Size::new(20.0, 10.0);
            add(&mut tree, root, data, panel(Color::RED));
        }
        crate::set_text_measurer(&mut tree, Rc::new(crate::ApproxTextMeasurer::default()));
        let wanted = crate::content_size(&tree, Size::new(10_000.0, 1_000.0));
        assert!(
            wanted.min.width < wanted.preferred.width,
            "min {} should be smaller than the one-line preferred {}",
            wanted.min.width,
            wanted.preferred.width
        );
        // The widest item (plus the root's own padding) is the floor.
        assert!(
            wanted.min.width <= 20.0 + 32.0,
            "min = {}",
            wanted.min.width
        );
    }

    /// A flex item with a definite main size measures its cross content at that
    /// size, so a fixed-width card's wrapped text still gets its full height
    /// instead of the one-line height for the (wider) container.
    #[test]
    fn a_fixed_width_item_sizes_its_wrapped_text() {
        let mut tree = SceneTree::new();
        let tree_root = tree.root();
        let root = add(
            &mut tree,
            tree_root,
            ControlData::fill_parent(),
            panel(Color::TRANSPARENT),
        );
        let row = add(
            &mut tree,
            root,
            ControlData::fill_parent(),
            Widget::Flex(FlexStyle::row().padding(Edges::ZERO).gap(0.0)),
        );
        let card = add(
            &mut tree,
            row,
            ControlData {
                layout: LayoutStyle {
                    basis: SizeBasis::Px(100.0),
                    shrink: 0.0,
                    ..LayoutStyle::default()
                },
                ..ControlData::default()
            },
            Widget::Flex(FlexStyle::column().padding(Edges::ZERO).gap(0.0)),
        );
        add(
            &mut tree,
            card,
            ControlData::default(),
            Widget::Label {
                text: "word ".repeat(20),
                font_size: 10.0,
                color: Color::BLACK,
                options: crate::layout::TextOptions {
                    wrap: true,
                    ..Default::default()
                },
            },
        );

        crate::layout(&mut tree, ViewportSize::new(Size::new(600.0, 400.0)));
        let card = crate::control(&tree, card).unwrap().rect;
        assert!(
            (card.size.width - 100.0).abs() < 0.5,
            "width {}",
            card.size.width
        );
        let line_h = crate::layout::line_height(10.0);
        assert!(
            card.size.height >= 2.0 * line_h,
            "height {} should fit the wrapped text (one line is {line_h})",
            card.size.height
        );
    }
}
