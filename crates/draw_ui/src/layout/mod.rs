//! Backend-neutral layout vocabulary and measurement helpers.
//!
//! Layout is resolved in two passes:
//!
//! 1. **Measure** — bottom-up intrinsic sizes ([`ContentSize`]) derived from
//!    content (text, padding) and the parent's available space.
//! 2. **Arrange** — top-down placement of each control's absolute rectangle.
//!
//! Containers ([`FlexStyle`], [`GridStyle`]) own the arrange pass; every control
//! carries a [`LayoutStyle`] telling its parent how to size and align it. This
//! module only defines the types and pure text math — the traversal lives in
//! `crate::ui::layout`.

pub mod text;

pub use text::{
    char_advance, is_wide, layout_text, line_height, longest_unit_width, longest_unit_width_with,
    measure, measure_line, measure_line_with, measure_with, wrap_text, wrap_text_with,
    wrap_text_with_break, ApproxTextMeasurer, FixedWidthTextMeasurer, TextMeasurer, TextOptions,
    WordBreak,
};

use draw_core::{Edges, Size};

/// Intrinsic size of a control: the smallest it may become (`min`) and its
/// natural size (`preferred`), both independent of any parent.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ContentSize {
    pub min: Size,
    pub preferred: Size,
}

impl ContentSize {
    pub const ZERO: Self = Self {
        min: Size::ZERO,
        preferred: Size::ZERO,
    };

    pub const fn new(min: Size, preferred: Size) -> Self {
        Self { min, preferred }
    }
}

/// Main-axis direction of a [`FlexStyle`] container.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum FlexDirection {
    #[default]
    Row,
    Column,
    RowReverse,
    ColumnReverse,
}

impl FlexDirection {
    pub fn is_horizontal(self) -> bool {
        matches!(self, Self::Row | Self::RowReverse)
    }

    pub fn is_reverse(self) -> bool {
        matches!(self, Self::RowReverse | Self::ColumnReverse)
    }
}

/// Distribution of free space along the main axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Justify {
    #[default]
    Start,
    Center,
    End,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
}

/// Alignment along the cross axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Align {
    Start,
    Center,
    End,
    /// Stretch to fill the container's cross size.
    #[default]
    Stretch,
}

/// Distribution of whole lines/tracks along the cross axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum AlignContent {
    Start,
    Center,
    End,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
    /// Grow auto-sized lines/tracks to fill the leftover cross space.
    #[default]
    Stretch,
}

/// How a control decides its main-axis size before flexing.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum SizeBasis {
    /// Use the control's intrinsic preferred main size.
    #[default]
    Auto,
    /// Fixed logical pixels.
    Px(f32),
    /// Fraction of the container's content main size (`0.0..=1.0`).
    Percent(f32),
}

/// Per-control participation in its parent's layout.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayoutStyle {
    /// Share of leftover main-axis space this control absorbs.
    pub grow: f32,
    /// Willingness to shrink (weighted by basis) when space is tight.
    pub shrink: f32,
    /// Main-axis basis.
    pub basis: SizeBasis,
    /// Cross-axis alignment override for flex containers.
    pub align_self: Option<Align>,
    /// Paint/placement order within the parent (lower first, stable ties).
    pub order: i32,
    /// Explicit grid cell for grid containers (`None` = auto-flow).
    pub grid: Option<GridPlacement>,
}

impl Default for LayoutStyle {
    fn default() -> Self {
        Self {
            grow: 0.0,
            shrink: 1.0,
            basis: SizeBasis::Auto,
            align_self: None,
            order: 0,
            grid: None,
        }
    }
}

impl LayoutStyle {
    pub const fn new() -> Self {
        Self {
            grow: 0.0,
            shrink: 1.0,
            basis: SizeBasis::Auto,
            align_self: None,
            order: 0,
            grid: None,
        }
    }

    pub const fn grow(mut self, grow: f32) -> Self {
        self.grow = grow;
        self
    }

    pub const fn shrink(mut self, shrink: f32) -> Self {
        self.shrink = shrink;
        self
    }

    pub const fn basis(mut self, basis: SizeBasis) -> Self {
        self.basis = basis;
        self
    }

    pub const fn align_self(mut self, align: Align) -> Self {
        self.align_self = Some(align);
        self
    }

    pub const fn grid(mut self, placement: GridPlacement) -> Self {
        self.grid = Some(placement);
        self
    }

    pub const fn order(mut self, order: i32) -> Self {
        self.order = order;
        self
    }
}

/// Flex container configuration.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FlexStyle {
    pub direction: FlexDirection,
    pub justify: Justify,
    pub align: Align,
    pub align_content: AlignContent,
    /// Whether items wrap onto multiple main-axis lines.
    pub wrap: bool,
    /// Gap between items along the main axis.
    pub gap: f32,
    /// Gap between wrapped lines along the cross axis.
    pub cross_gap: f32,
    pub padding: Edges,
}

impl Default for FlexStyle {
    fn default() -> Self {
        Self {
            direction: FlexDirection::Row,
            justify: Justify::Start,
            align: Align::Stretch,
            align_content: AlignContent::Stretch,
            wrap: false,
            gap: 8.0,
            cross_gap: 8.0,
            padding: Edges::all(16.0),
        }
    }
}

impl FlexStyle {
    pub fn row() -> Self {
        Self::default()
    }

    pub fn column() -> Self {
        Self {
            direction: FlexDirection::Column,
            ..Self::default()
        }
    }

    pub fn direction(mut self, direction: FlexDirection) -> Self {
        self.direction = direction;
        self
    }

    pub fn justify(mut self, justify: Justify) -> Self {
        self.justify = justify;
        self
    }

    pub fn align(mut self, align: Align) -> Self {
        self.align = align;
        self
    }

    pub fn align_content(mut self, align_content: AlignContent) -> Self {
        self.align_content = align_content;
        self
    }

    pub fn wrap(mut self, wrap: bool) -> Self {
        self.wrap = wrap;
        self
    }

    /// Sets the main- and cross-axis gaps together.
    pub fn gap(mut self, gap: f32) -> Self {
        self.gap = gap;
        self.cross_gap = gap;
        self
    }

    /// Sets only the cross-axis (wrapped-line) gap.
    pub fn cross_gap(mut self, cross_gap: f32) -> Self {
        self.cross_gap = cross_gap;
        self
    }

    pub fn padding(mut self, padding: Edges) -> Self {
        self.padding = padding;
        self
    }
}

/// A grid track (column or row) definition.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Track {
    /// Sized to the largest item in the track.
    #[default]
    Auto,
    /// Fixed logical pixels.
    Px(f32),
    /// Fraction of the leftover space.
    Fr(f32),
    /// Fraction of the container's content size (`0.0..=1.0`).
    Percent(f32),
}

/// Grid container configuration.
#[derive(Debug, Clone, PartialEq)]
pub struct GridStyle {
    pub columns: Vec<Track>,
    pub rows: Vec<Track>,
    /// Cross-axis (vertical) alignment of items within their cell.
    pub align_items: Align,
    /// Main-axis (horizontal) alignment of items within their cell.
    pub justify_items: Align,
    /// Distribution of rows when they do not fill the container height.
    pub align_content: AlignContent,
    pub column_gap: f32,
    pub row_gap: f32,
    pub padding: Edges,
}

impl Default for GridStyle {
    fn default() -> Self {
        Self {
            columns: Vec::new(),
            rows: Vec::new(),
            align_items: Align::Stretch,
            justify_items: Align::Stretch,
            align_content: AlignContent::Stretch,
            column_gap: 8.0,
            row_gap: 8.0,
            padding: Edges::all(16.0),
        }
    }
}

impl GridStyle {
    pub fn new(columns: Vec<Track>) -> Self {
        Self {
            columns,
            ..Self::default()
        }
    }

    pub fn rows(mut self, rows: Vec<Track>) -> Self {
        self.rows = rows;
        self
    }

    pub fn align_items(mut self, align: Align) -> Self {
        self.align_items = align;
        self
    }

    pub fn justify_items(mut self, align: Align) -> Self {
        self.justify_items = align;
        self
    }

    pub fn align_content(mut self, align: AlignContent) -> Self {
        self.align_content = align;
        self
    }

    pub fn column_gap(mut self, gap: f32) -> Self {
        self.column_gap = gap;
        self
    }

    pub fn row_gap(mut self, gap: f32) -> Self {
        self.row_gap = gap;
        self
    }

    pub fn gap(mut self, gap: f32) -> Self {
        self.column_gap = gap;
        self.row_gap = gap;
        self
    }

    pub fn padding(mut self, padding: Edges) -> Self {
        self.padding = padding;
        self
    }
}

/// Explicit placement and span of a control inside a grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct GridPlacement {
    pub column: usize,
    pub row: usize,
    pub column_span: usize,
    pub row_span: usize,
}

impl GridPlacement {
    pub const fn new(column: usize, row: usize) -> Self {
        Self {
            column,
            row,
            column_span: 1,
            row_span: 1,
        }
    }

    pub fn column_span(mut self, span: usize) -> Self {
        self.column_span = span.max(1);
        self
    }

    pub fn row_span(mut self, span: usize) -> Self {
        self.row_span = span.max(1);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flex_direction_axes() {
        assert!(FlexDirection::Row.is_horizontal());
        assert!(!FlexDirection::Column.is_horizontal());
        assert!(FlexDirection::RowReverse.is_reverse());
        assert!(!FlexDirection::Column.is_reverse());
    }

    #[test]
    fn layout_style_defaults_and_builder() {
        let style = LayoutStyle::default();
        assert_eq!(style.grow, 0.0);
        assert_eq!(style.shrink, 1.0);
        assert_eq!(style.basis, SizeBasis::Auto);

        let style = LayoutStyle::new().grow(2.0).basis(SizeBasis::Px(40.0));
        assert_eq!(style.grow, 2.0);
        assert_eq!(style.basis, SizeBasis::Px(40.0));
    }

    #[test]
    fn grid_placement_span_is_at_least_one() {
        let placement = GridPlacement::new(1, 2).column_span(0).row_span(3);
        assert_eq!(placement.column_span, 1);
        assert_eq!(placement.row_span, 3);
    }
}
