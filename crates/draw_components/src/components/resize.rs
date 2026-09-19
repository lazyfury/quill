//! Draggable split dividers.

use std::cell::Cell;
use std::rc::Rc;

use draw_app::{update_control, Component, Spec};
use draw_core::{Color, Cursor, Edges, NodeId, Size, Vec2};
use draw_theme::Theme;
use draw_ui::{DragPhase, MouseFilter, SizeBasis, Widget};

/// A divider that resizes the pane before it while dragged.
///
/// Visually it is a 1px [`Divider`](crate::Divider); the node itself is a wider
/// gutter (`size`, default 6px) so the pointer can grab it. Dragging updates the
/// target pane's flex basis through the shared width cell, and the flex
/// container re-adapts the remaining panes.
///
/// ```ignore
/// let width = Rc::new(Cell::new(220.0));
/// let sidebar = tree.add_child(split, Sidebar::new(...).basis(SizeBasis::Px(width.get())));
/// tree.add_child(split, ResizeHandle::vertical(theme)
///     .target(sidebar)
///     .width(width)
///     .min(140.0));
/// ```
pub struct ResizeHandle {
    spec: Spec,
    theme: Theme,
    vertical: bool,
    size: f32,
    target: Option<NodeId>,
    width: Option<Rc<Cell<f32>>>,
    min: f32,
    max: f32,
    color: Option<Color>,
}

impl ResizeHandle {
    /// A vertical line that resizes the pane to its left/right.
    pub fn vertical(theme: Theme) -> Self {
        Self {
            spec: Spec::default(),
            theme,
            vertical: true,
            size: 6.0,
            target: None,
            width: None,
            min: 0.0,
            max: f32::INFINITY,
            color: None,
        }
    }

    /// A horizontal line that resizes the pane above/below it.
    pub fn horizontal(theme: Theme) -> Self {
        Self {
            vertical: false,
            ..Self::vertical(theme)
        }
    }

    /// Gutter width (the pointer hit area). The visible line stays 1px.
    pub fn size(mut self, size: f32) -> Self {
        self.size = size.max(1.0);
        self
    }

    /// The pane whose main-axis size this handle drives.
    pub fn target(mut self, target: NodeId) -> Self {
        self.target = Some(target);
        self
    }

    /// Shared current size of the target pane, in logical pixels.
    pub fn width(mut self, width: Rc<Cell<f32>>) -> Self {
        self.width = Some(width);
        self
    }

    /// Lower clamp for the target size.
    pub fn min(mut self, min: f32) -> Self {
        self.min = min;
        self
    }

    /// Upper clamp for the target size.
    pub fn max(mut self, max: f32) -> Self {
        self.max = max;
        self
    }

    pub fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }
}

impl Component for ResizeHandle {
    fn spec(&mut self) -> &mut Spec {
        &mut self.spec
    }

    fn name(&self) -> &'static str {
        "ResizeHandle"
    }

    fn widget(&self) -> Widget {
        Widget::Flex(draw_ui::FlexStyle::default().padding(Edges::ZERO))
    }

    fn prepare(&mut self) {
        let theme = self.theme;
        let base = self.color.unwrap_or(theme.palette.border_subtle);
        let vertical = self.vertical;
        let size = self.size;
        let resize_cursor = if vertical {
            Cursor::ColResize
        } else {
            Cursor::RowResize
        };

        self.spec.data.mouse_filter = MouseFilter::Stop;
        self.spec.data.cursor = resize_cursor;
        // While dragging the handle reports a grabbed cursor; otherwise the
        // resize cursor. The component owns this state entirely.
        let dragging = Rc::new(Cell::new(false));
        let dragging_cursor = dragging.clone();
        self.spec.cursor_provider = Some(Box::new(move || {
            if dragging_cursor.get() {
                Cursor::Grabbing
            } else {
                resize_cursor
            }
        }));
        // A fixed gutter: never grow or shrink along the main axis.
        self.spec.data.layout.grow = 0.0;
        self.spec.data.layout.shrink = 0.0;
        self.spec.data.min_size = if vertical {
            Size::new(size, 0.0)
        } else {
            Size::new(0.0, size)
        };
        self.spec.foreground = Some(Box::new(move |ctx, rect, state| {
            let color = if state.hovered || state.pressed {
                theme.palette.accent
            } else {
                base
            };
            if vertical {
                ctx.draw_line(
                    Vec2::new(rect.center().x, rect.top()),
                    Vec2::new(rect.center().x, rect.bottom()),
                    1.0,
                    color,
                );
            } else {
                ctx.draw_line(
                    Vec2::new(rect.left(), rect.center().y),
                    Vec2::new(rect.right(), rect.center().y),
                    1.0,
                    color,
                );
            }
        }));

        let (Some(target), Some(width)) = (self.target, self.width.clone()) else {
            return;
        };
        let (min, max) = (self.min, self.max);
        self.spec.on_drag = Some(Box::new(move |tree, phase, delta| match phase {
            DragPhase::Start => dragging.set(true),
            DragPhase::End => dragging.set(false),
            DragPhase::Move => {
                let current = width.get();
                let step = if vertical { delta.x } else { delta.y };
                let next = (current + step).clamp(min, max);
                if (next - current).abs() > f32::EPSILON {
                    width.set(next);
                    update_control(tree, target, |data| {
                        data.layout.basis = SizeBasis::Px(next);
                    });
                }
            }
        }));
    }
}

draw_app::impl_scene_child!(ResizeHandle);
