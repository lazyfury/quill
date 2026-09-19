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
use crate::layout::{
    Align, AlignContent, ContentSize, FlexStyle, GridPlacement, GridStyle, Justify, LayoutStyle,
    SizeBasis, Track,
};
use draw_core::{Rect, Size, Vec2, Viewport};

impl Ui {
    /// Resolves every control's absolute rectangle against the viewport and
    /// refreshes scene visibility.
    pub fn layout(&mut self, viewport: Viewport) {
        if self.layout_valid && self.layout_viewport == viewport && self.dirty.is_empty() {
            return;
        }
        if self.layout_viewport != viewport {
            self.mark_all_dirty();
        }
        let dirty = std::mem::take(&mut self.dirty);
        self.measure_cache.borrow_mut().clear();
        self.last_arranged = 0;

        let viewport_rect = viewport.logical_rect();
        if let Some(root) = self.controls.get_mut(&self.root) {
            root.rect = viewport_rect;
        }

        let mut rects: HashMap<NodeId, Rect> = HashMap::new();
        let root_children = self.children_vec(self.root);
        for child in root_children {
            let child_rect = self.resolve_child_rect(child, viewport_rect);
            self.arrange_node(child, child_rect, &mut rects, &dirty);
        }
        for (id, rect) in rects {
            if let Some(control) = self.controls.get_mut(&id) {
                control.rect = rect;
            }
        }
        self.layout_valid = true;
        self.layout_viewport = viewport;
        self.layout_count += 1;
        self.tree.update();
    }

    /// Places `id` at `rect` and arranges its subtree.
    ///
    /// When both `id` and its subtree are clean and the resolved rect is
    /// unchanged, the subtree is left untouched (partial relayout).
    fn arrange_node(
        &mut self,
        id: NodeId,
        rect: Rect,
        out: &mut HashMap<NodeId, Rect>,
        dirty: &HashSet<NodeId>,
    ) {
        let unchanged =
            !dirty.contains(&id) && self.controls.get(&id).is_some_and(|c| c.rect == rect);
        out.insert(id, rect);
        if unchanged {
            return;
        }
        self.last_arranged += 1;

        let children = self.children_vec(id);
        match self.widgets.get(&id).cloned() {
            Some(Widget::Flex(style)) => self.arrange_flex(id, rect, &style, &children, out, dirty),
            Some(Widget::Grid(style)) => self.arrange_grid(id, rect, &style, &children, out, dirty),
            _ => {
                for child in children {
                    let child_rect = self.resolve_child_rect(child, rect);
                    self.arrange_node(child, child_rect, out, dirty);
                }
            }
        }
    }

    /// Rectangle for a child of a non-container parent, honoring anchors/offsets
    /// and falling back to intrinsic size in degenerate dimensions.
    fn resolve_child_rect(&self, id: NodeId, parent: Rect) -> Rect {
        let Some(control) = self.controls.get(&id).copied() else {
            return parent;
        };
        let measured = self.measure_node(id, parent.size);
        let mut rect = control.resolve_rect(parent);
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

    /// Intrinsic size of a control given the space its parent can offer.
    fn measure_node(&self, id: NodeId, available: Size) -> ContentSize {
        let key = (id, available.width.to_bits(), available.height.to_bits());
        if let Some(cached) = self.measure_cache.borrow().get(&key).copied() {
            return cached;
        }

        let explicit = self
            .controls
            .get(&id)
            .map(|control| control.min_size)
            .unwrap_or(Size::ZERO);
        let measured = match self.widgets.get(&id) {
            Some(Widget::Flex(style)) => {
                let children = self.children_vec(id);
                self.measure_flex(id, style, &children, available)
            }
            Some(Widget::Grid(style)) => {
                let children = self.children_vec(id);
                self.measure_grid(id, style, &children, available)
            }
            Some(widget) => widget.measure_with(available, self.text_measurer.as_ref()),
            None => ContentSize::ZERO,
        };
        let min = measured.min.max(explicit);
        let result = ContentSize {
            min,
            preferred: measured.preferred.max(min),
        };
        self.measure_cache.borrow_mut().insert(key, result);
        result
    }

    fn measure_flex(
        &self,
        id: NodeId,
        style: &FlexStyle,
        children: &[NodeId],
        available: Size,
    ) -> ContentSize {
        let children = self.ordered_children(id, children);
        let pad = style.padding;
        let inner = Size::new(
            (available.width - pad.horizontal()).max(0.0),
            (available.height - pad.vertical()).max(0.0),
        );
        let horizontal = style.direction.is_horizontal();

        let mut pref_main = 0.0f32;
        let mut pref_cross = 0.0f32;
        let mut min_main = 0.0f32;
        let mut min_cross = 0.0f32;

        for (index, child) in children.iter().enumerate() {
            let measured = self.measure_node(*child, inner);
            let layout = self.layout_style(*child);
            let (p_main, p_cross, c_min_main, c_min_cross) = if horizontal {
                (
                    measured.preferred.width,
                    measured.preferred.height,
                    measured.min.width,
                    measured.min.height,
                )
            } else {
                (
                    measured.preferred.height,
                    measured.preferred.width,
                    measured.min.height,
                    measured.min.width,
                )
            };
            let basis = resolve_basis(layout.basis, p_main, inner_main(horizontal, inner));
            let gap = if index == 0 { 0.0 } else { style.gap };
            pref_main += basis.max(c_min_main) + gap;
            min_main += c_min_main + gap;
            pref_cross = pref_cross.max(p_cross);
            min_cross = min_cross.max(c_min_cross);
        }

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
        id: NodeId,
        style: &GridStyle,
        children: &[NodeId],
        available: Size,
    ) -> ContentSize {
        let children = self.ordered_children(id, children);
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
            let measured = self.measure_node(*child, inner);
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

    fn layout_style(&self, id: NodeId) -> LayoutStyle {
        self.controls.get(&id).map(|c| c.layout).unwrap_or_default()
    }

    /// Children in paint/placement order (`LayoutStyle::order`, stable ties).
    ///
    /// The sorted list is cached per container and invalidated by
    /// [`mark_dirty`](Ui::mark_dirty) / [`mark_all_dirty`](Ui::mark_all_dirty).
    fn ordered_children(&self, id: NodeId, children: &[NodeId]) -> Vec<NodeId> {
        if let Some(cached) = self.order_cache.borrow().get(&id) {
            if cached.len() == children.len() {
                return cached.clone();
            }
        }
        let mut ordered = children.to_vec();
        ordered.sort_by_key(|child| self.layout_style(*child).order);
        self.order_cache.borrow_mut().insert(id, ordered.clone());
        ordered
    }

    // -- flex --------------------------------------------------------------

    fn arrange_flex(
        &mut self,
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
        let children = self.ordered_children(id, children);

        // Measure children and resolve their main-axis basis.
        let mut items: Vec<FlexItem> = Vec::with_capacity(children.len());
        for child in &children {
            let measured = self.measure_node(*child, content.size);
            let layout = self.layout_style(*child);
            let (p_main, p_cross, min_main) = if horizontal {
                (
                    measured.preferred.width,
                    measured.preferred.height,
                    measured.min.width,
                )
            } else {
                (
                    measured.preferred.height,
                    measured.preferred.width,
                    measured.min.height,
                )
            };
            let basis = resolve_basis(layout.basis, p_main, content_main).max(min_main);
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
                self.arrange_node(item.id, child_rect, out, dirty);
                cursor += item.main + style.gap + extra_gap;
            }
        }
    }

    // -- grid --------------------------------------------------------------

    fn arrange_grid(
        &mut self,
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
        let children = self.ordered_children(id, children);
        let columns = style.columns.len().max(1);
        let placements = resolve_placements(&children, columns, self);

        let rows = placements
            .iter()
            .map(|p| p.row + p.row_span)
            .max()
            .unwrap_or(1)
            .max(style.rows.len());

        // Intrinsic preferred size of every child.
        let measured: Vec<Size> = children
            .iter()
            .map(|child| self.measure_node(*child, content.size).preferred)
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
            self.arrange_node(*child, child_rect, out, dirty);
        }
    }
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

fn resolve_placements(children: &[NodeId], columns: usize, ui: &Ui) -> Vec<GridPlacement> {
    let mut placements: Vec<Option<GridPlacement>> = children
        .iter()
        .map(|child| ui.controls.get(child).and_then(|c| c.layout.grid))
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
    use crate::component::Button;
    use crate::layout::{Align, AlignContent, FlexStyle, GridPlacement, LayoutStyle, Track};
    use crate::widget::Widget;
    use draw_core::{Color, Edges, Size};
    use std::rc::Rc;

    fn viewport(w: f32, h: f32) -> Viewport {
        Viewport::new(Size::new(w, h))
    }

    fn rect(ui: &Ui, id: NodeId) -> Rect {
        ui.control(id).unwrap().rect
    }

    fn panel(ui: &mut Ui, parent: NodeId, name: &str) -> NodeId {
        ui.insert(
            parent,
            name,
            ControlData::default(),
            Widget::Panel {
                color: Color::WHITE,
                border: None,
            },
        )
    }

    fn label(ui: &mut Ui, parent: NodeId, name: &str, text: &str, font_size: f32) -> NodeId {
        ui.insert(
            parent,
            name,
            ControlData::default(),
            Widget::Label {
                text: text.into(),
                font_size,
                color: Color::WHITE,
                options: crate::layout::TextOptions::default(),
            },
        )
    }

    #[test]
    fn flex_row_grow_distributes_leftover() {
        let mut ui = Ui::new();
        let row = ui.add_flex(ui.root(), FlexStyle::row().gap(0.0).padding(Edges::ZERO));
        let a = panel(&mut ui, row, "A");
        let b = panel(&mut ui, row, "B");
        ui.set_layout_style(
            a,
            LayoutStyle::new().basis(SizeBasis::Px(100.0)).shrink(0.0),
        );
        ui.set_layout_style(b, LayoutStyle::new().basis(SizeBasis::Px(100.0)).grow(1.0));
        ui.layout(viewport(300.0, 100.0));
        assert_eq!(rect(&ui, a).size.width, 100.0);
        assert_eq!(rect(&ui, b).size.width, 200.0);
    }

    #[test]
    fn align_center_centers_cross_axis() {
        let mut ui = Ui::new();
        let row = ui.add_flex(
            ui.root(),
            FlexStyle::row().align(Align::Center).padding(Edges::ZERO),
        );
        let a = label(&mut ui, row, "A", "hi", 10.0);
        ui.layout(viewport(200.0, 100.0));
        let r = rect(&ui, a);
        let expected = (100.0 - r.size.height) / 2.0;
        assert!((r.top() - expected).abs() < 1e-3);
    }

    #[test]
    fn grid_tracks_and_gaps() {
        let mut ui = Ui::new();
        let grid = ui.add_grid(
            ui.root(),
            GridStyle::new(vec![Track::Px(50.0), Track::Fr(1.0)])
                .rows(vec![Track::Px(40.0)])
                .gap(10.0)
                .padding(Edges::ZERO),
        );
        panel(&mut ui, grid, "a");
        panel(&mut ui, grid, "b");
        ui.layout(viewport(200.0, 100.0));
        let children = ui.children_vec(grid);
        let a = rect(&ui, children[0]);
        let b = rect(&ui, children[1]);
        assert_eq!(a.left(), 0.0);
        assert_eq!(a.size.width, 50.0);
        assert_eq!(a.size.height, 40.0);
        assert_eq!(b.left(), 60.0);
        assert_eq!(b.size.width, 140.0);
    }

    #[test]
    fn order_reorders_children() {
        let mut ui = Ui::new();
        let row = ui.add_flex(ui.root(), FlexStyle::row().gap(0.0).padding(Edges::ZERO));
        let a = panel(&mut ui, row, "A");
        let b = panel(&mut ui, row, "B");
        for id in [a, b] {
            ui.set_layout_style(
                id,
                LayoutStyle::new().basis(SizeBasis::Px(50.0)).shrink(0.0),
            );
        }
        ui.set_layout_style(
            a,
            LayoutStyle::new()
                .basis(SizeBasis::Px(50.0))
                .shrink(0.0)
                .order(2),
        );
        ui.set_layout_style(
            b,
            LayoutStyle::new()
                .basis(SizeBasis::Px(50.0))
                .shrink(0.0)
                .order(1),
        );
        ui.layout(viewport(200.0, 100.0));
        assert!(rect(&ui, b).left() < rect(&ui, a).left());
    }

    #[test]
    fn align_content_centers_wrapped_lines() {
        let mut ui = Ui::new();
        let row = ui.add_flex(
            ui.root(),
            FlexStyle::row()
                .wrap(true)
                .align_content(AlignContent::Center)
                .gap(0.0)
                .cross_gap(0.0)
                .padding(Edges::ZERO),
        );
        let mut ids = Vec::new();
        for name in ["a", "b", "c"] {
            ids.push(label(&mut ui, row, name, "x", 10.0));
        }
        for id in &ids {
            ui.set_layout_style(
                *id,
                LayoutStyle::new().basis(SizeBasis::Px(80.0)).shrink(0.0),
            );
        }
        ui.layout(viewport(200.0, 100.0));
        let line_h = crate::layout::line_height(10.0);
        let expected = (100.0 - line_h * 2.0) / 2.0;
        assert!((rect(&ui, ids[0]).top() - expected).abs() < 1e-3);
        assert!(rect(&ui, ids[0]).top() < rect(&ui, ids[2]).top());
    }

    #[test]
    fn grid_span_grows_auto_tracks() {
        let mut ui = Ui::new();
        let grid = ui.add_grid(
            ui.root(),
            GridStyle::new(vec![Track::Auto, Track::Auto])
                .gap(0.0)
                .padding(Edges::ZERO),
        );
        let wide = panel(&mut ui, grid, "wide");
        ui.set_layout_style(
            wide,
            LayoutStyle::new().grid(GridPlacement::new(0, 0).column_span(2)),
        );
        ui.set_min_size(wide, Size::new(200.0, 20.0));
        let b = panel(&mut ui, grid, "b");
        panel(&mut ui, grid, "c");
        ui.layout(viewport(400.0, 200.0));
        assert_eq!(rect(&ui, wide).size.width, 200.0);
        assert_eq!(rect(&ui, b).size.width, 100.0);
    }

    #[test]
    fn grid_item_alignment_within_cell() {
        let mut ui = Ui::new();
        let grid = ui.add_grid(
            ui.root(),
            GridStyle::new(vec![Track::Px(100.0)])
                .rows(vec![Track::Px(100.0)])
                .align_items(Align::End)
                .justify_items(Align::Center)
                .gap(0.0)
                .padding(Edges::ZERO),
        );
        let id = label(&mut ui, grid, "L", "hi", 10.0);
        ui.layout(viewport(200.0, 200.0));
        let r = rect(&ui, id);
        assert!((r.bottom() - 100.0).abs() < 1e-3);
        assert!((r.left() - (100.0 - r.size.width) / 2.0).abs() < 1e-3);
    }

    #[test]
    fn wrapped_flex_uses_multiple_lines() {
        let mut ui = Ui::new();
        let row = ui.add_flex(
            ui.root(),
            FlexStyle::row().wrap(true).gap(0.0).padding(Edges::ZERO),
        );
        let mut ids = Vec::new();
        for name in ["a", "b", "c"] {
            ids.push(label(&mut ui, row, name, "x", 10.0));
        }
        for id in &ids {
            ui.set_layout_style(
                *id,
                LayoutStyle::new().basis(SizeBasis::Px(80.0)).shrink(0.0),
            );
        }
        ui.layout(viewport(200.0, 200.0));
        assert!(rect(&ui, ids[0]).top() < rect(&ui, ids[2]).top());
    }

    #[test]
    fn stretch_does_not_grow_a_definite_cross_axis() {
        // A fixed-width column must not widen when a child's preferred width
        // exceeds it (a fixed sibling plus a wrapping label); the child stays at
        // the column width.
        let mut ui = Ui::new();
        let column = ui.add_flex(ui.root(), FlexStyle::column().gap(0.0).padding(Edges::ZERO));
        ui.set_anchors(column, Edges::new(0.0, 0.0, 0.0, 1.0));
        ui.set_offsets(column, Edges::new(0.0, 0.0, 100.0, 0.0));

        let row = ui.add_flex(column, FlexStyle::row().gap(0.0).padding(Edges::ZERO));
        let fixed = panel(&mut ui, row, "fixed");
        ui.set_layout_style(
            fixed,
            LayoutStyle::new().basis(SizeBasis::Px(44.0)).shrink(0.0),
        );
        // Words stay short (min-content fits) but the line wants to be wider.
        label(&mut ui, row, "label", "aaaa bbbb cccc dddd eeee ffff", 20.0);

        ui.layout(viewport(200.0, 200.0));
        let column_rect = rect(&ui, column);
        let row_rect = rect(&ui, row);
        assert!((column_rect.size.width - 100.0).abs() < 1e-3);
        assert!(
            (row_rect.size.width - column_rect.size.width).abs() < 1e-3,
            "row {} escaped column {}",
            row_rect.size.width,
            column_rect.size.width
        );
    }

    #[test]
    fn shrink_respects_min_size() {
        let mut ui = Ui::new();
        let row = ui.add_flex(ui.root(), FlexStyle::row().gap(0.0).padding(Edges::ZERO));
        let a = panel(&mut ui, row, "A");
        let b = panel(&mut ui, row, "B");
        ui.set_layout_style(
            a,
            LayoutStyle::new().basis(SizeBasis::Px(100.0)).shrink(1.0),
        );
        ui.set_min_size(a, Size::new(80.0, 0.0));
        ui.set_layout_style(
            b,
            LayoutStyle::new().basis(SizeBasis::Px(100.0)).shrink(1.0),
        );
        ui.layout(viewport(150.0, 100.0));
        assert_eq!(rect(&ui, a).size.width, 80.0);
        assert!(rect(&ui, b).size.width < 100.0);
    }

    #[test]
    fn partial_change_rearranges_fewer_nodes() {
        let mut ui = Ui::new();

        let root = ui.root();
        let panel_a = panel(&mut ui, root, "PanelA");
        ui.set_anchors(panel_a, Edges::new(0.0, 0.0, 0.0, 0.0));
        ui.set_offsets(panel_a, Edges::new(0.0, 0.0, 140.0, 200.0));
        let col_a = ui.add_flex(panel_a, FlexStyle::column().padding(Edges::ZERO));
        let changing = label(&mut ui, col_a, "LA", "left", 10.0);

        let panel_b = panel(&mut ui, root, "PanelB");
        ui.set_anchors(panel_b, Edges::new(1.0, 0.0, 1.0, 0.0));
        ui.set_offsets(panel_b, Edges::new(-140.0, 0.0, 0.0, 200.0));
        let col_b = ui.add_flex(panel_b, FlexStyle::column().padding(Edges::ZERO));
        label(&mut ui, col_b, "B1", "b1", 10.0);
        label(&mut ui, col_b, "B2", "b2", 10.0);

        let vp = viewport(300.0, 200.0);
        ui.layout(vp);
        let full = ui.last_arranged_nodes();

        ui.set_text(changing, "changed longer text");
        ui.layout(vp);
        let partial = ui.last_arranged_nodes();

        assert!(full >= 7, "expected a full pass to visit every control");
        assert!(
            partial < full,
            "the untouched panel subtree should be skipped ({partial} < {full})"
        );
        assert_eq!(ui.layout_count(), 2);
    }

    #[test]
    fn order_change_invalidates_order_cache() {
        let mut ui = Ui::new();
        let row = ui.add_flex(ui.root(), FlexStyle::row().gap(0.0).padding(Edges::ZERO));
        let a = panel(&mut ui, row, "A");
        let b = panel(&mut ui, row, "B");
        for id in [a, b] {
            ui.set_layout_style(
                id,
                LayoutStyle::new().basis(SizeBasis::Px(50.0)).shrink(0.0),
            );
        }
        let vp = viewport(200.0, 100.0);
        ui.layout(vp);
        assert!(rect(&ui, a).left() < rect(&ui, b).left());

        ui.set_layout_style(
            a,
            LayoutStyle::new()
                .basis(SizeBasis::Px(50.0))
                .shrink(0.0)
                .order(2),
        );
        ui.set_layout_style(
            b,
            LayoutStyle::new()
                .basis(SizeBasis::Px(50.0))
                .shrink(0.0)
                .order(1),
        );
        ui.layout(vp);
        assert!(rect(&ui, b).left() < rect(&ui, a).left());
    }

    #[test]
    fn grid_rows_are_offset_by_the_container_origin() {
        let mut ui = Ui::new();
        let grid = ui.add_grid(
            ui.root(),
            GridStyle::new(vec![Track::Px(100.0)])
                .rows(vec![Track::Px(40.0)])
                .gap(0.0)
                .padding(Edges::ZERO),
        );
        ui.set_anchors(grid, Edges::new(0.0, 0.0, 0.0, 0.0));
        ui.set_offsets(grid, Edges::new(0.0, 50.0, 200.0, 150.0));
        let child = panel(&mut ui, grid, "cell");
        ui.layout(viewport(300.0, 300.0));
        assert_eq!(rect(&ui, child).top(), 50.0);
        assert_eq!(rect(&ui, child).left(), 0.0);
    }

    #[test]
    fn wrapped_button_grows_height() {
        let mut ui = Ui::new();
        let col = ui.add_flex(ui.root(), FlexStyle::column().gap(0.0));
        let double = ui.add_flex(ui.root(), FlexStyle::column().gap(0.0));
        let button = ui.add(col, Button::new("hello world hello world").wrap(true));
        let plain = ui.add(double, Button::new("hello world hello world"));
        ui.layout(viewport(100.0, 400.0));
        assert!(rect(&ui, button.id()).size.height > 48.0);
        // A non-wrapping button stays one line tall.
        assert!(rect(&ui, plain.id()).size.height <= 48.0);
    }

    #[test]
    fn layout_cache_skips_unchanged_viewport() {
        let mut ui = Ui::new();
        let col = ui.add_flex(ui.root(), FlexStyle::column());
        let id = label(&mut ui, col, "L", "x", 10.0);
        let vp = viewport(200.0, 200.0);
        ui.layout(vp);
        let count = ui.layout_count();

        ui.layout(vp);
        assert_eq!(
            ui.layout_count(),
            count,
            "redundant layout should be a no-op"
        );

        ui.set_text(id, "y");
        ui.layout(vp);
        assert_eq!(ui.layout_count(), count + 1);

        ui.layout(viewport(100.0, 200.0));
        assert_eq!(ui.layout_count(), count + 2);
    }

    #[test]
    fn injected_measurer_changes_wrapping() {
        let text = "hello world hello world";
        let narrow = viewport(120.0, 400.0);

        let mut approx = Ui::new();
        let col = approx.add_flex(approx.root(), FlexStyle::column().gap(0.0));
        let id = label(&mut approx, col, "L", text, 20.0);
        approx.layout(narrow);
        let approx_height = rect(&approx, id).size.height;

        let mut fixed = Ui::new();
        fixed.set_text_measurer(Rc::new(crate::layout::FixedWidthTextMeasurer::default()));
        let col = fixed.add_flex(fixed.root(), FlexStyle::column().gap(0.0));
        let id = label(&mut fixed, col, "L", text, 20.0);
        fixed.layout(narrow);
        let fixed_height = rect(&fixed, id).size.height;

        assert!(fixed_height > approx_height);
    }

    #[test]
    fn label_wraps_and_grows_height_in_column() {
        let mut ui = Ui::new();
        let col = ui.add_flex(ui.root(), FlexStyle::column().gap(0.0));
        let id = label(&mut ui, col, "L", "hello world hello world", 20.0);
        ui.layout(viewport(120.0, 400.0));
        let r = rect(&ui, id);
        assert!(r.size.width <= 120.0 + 1e-3);
        assert!(r.size.height > crate::layout::line_height(20.0));
    }
}
