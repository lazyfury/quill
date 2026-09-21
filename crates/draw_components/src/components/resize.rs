//! Draggable split dividers.

use std::cell::Cell;
use std::rc::Rc;

use crate::base::{update_control, Component, Spec};
use crate::NodeRef;
use draw_core::{Color, Cursor, Edges, Size, Vec2};
use draw_theme::Theme;
use draw_ui::{DragPhase, MouseFilter, SizeBasis, Widget};

/// A divider that resizes the pane before it while dragged.
///
/// Visually it is a 1px [`Divider`](crate::Divider); the node itself is a wider
/// gutter (`size`, default 6px) so the pointer can grab it. Dragging updates the
/// target pane's flex basis through the shared width cell, and the flex
/// container re-adapts the remaining panes.
///
/// The target is a [`NodeRef`] so siblings can reference each other before
/// mount:
///
/// ```ignore
/// let sidebar = NodeRef::new();
/// let tree = Flex::row()
///     .child(Sidebar::new(...).ref_(&sidebar))
///     .child(ResizeHandle::vertical(theme).target(sidebar).width(width).min(140.0))
///     .into_tree();
/// ```
pub struct ResizeHandle {
    spec: Spec,
    theme: Theme,
    vertical: bool,
    size: f32,
    target: Option<NodeRef>,
    width: Option<Rc<Cell<f32>>>,
    min: f32,
    max: f32,
    color: Option<Color>,
    /// Flip the drag direction: the target pane is on the far side of the
    /// handle (a right-hand sidebar resized from its left edge).
    invert: bool,
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
            invert: false,
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
    pub fn target(mut self, target: NodeRef) -> Self {
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

    /// Flips the drag direction: use it when the target pane is on the far side
    /// of the handle (e.g. a right sidebar resized from its left edge, so
    /// dragging left grows the sidebar).
    pub fn invert(mut self) -> Self {
        self.invert = true;
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

        let (Some(target), Some(width)) = (self.target.clone(), self.width.clone()) else {
            return;
        };
        let (min, max) = (self.min, self.max);
        let invert = self.invert;
        self.spec.on_drag = Some(Box::new(move |tree, phase, delta| match phase {
            DragPhase::Start => dragging.set(true),
            DragPhase::End => dragging.set(false),
            DragPhase::Move => {
                let current = width.get();
                let mut step = if vertical { delta.x } else { delta.y };
                if invert {
                    step = -step;
                }
                let next = (current + step).clamp(min, max);
                if (next - current).abs() > f32::EPSILON {
                    width.set(next);
                    if let Some(target) = target.get() {
                        update_control(tree, target, |data| {
                            data.layout.basis = SizeBasis::Px(next);
                        });
                    }
                }
            }
        }));
    }
}

crate::impl_scene_child!(ResizeHandle);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::base::Flex;
    use draw_core::{InputEvent, PointerButton, ViewportSize};
    use draw_scene::SceneTree;
    use draw_ui::control;

    /// Drags the handle 50px right and returns the target width it drove.
    fn dragged_width(invert: bool) -> f32 {
        let theme = Theme::dark();
        let width = Rc::new(Cell::new(200.0));
        let target = NodeRef::new();
        let mut tree = SceneTree::new();
        let root = tree.root();
        let mut handle = ResizeHandle::vertical(theme)
            .target(target.clone())
            .width(width.clone())
            .min(100.0)
            .max(300.0);
        if invert {
            handle = handle.invert();
        }
        let page = tree.add_child(
            root,
            Flex::row()
                .gap(0.0)
                .padding(Edges::ZERO)
                .child(
                    Flex::column()
                        .basis(SizeBasis::Px(200.0))
                        .shrink(0.0)
                        .ref_(&target),
                )
                .child(handle),
        );
        draw_ui::layout(&mut tree, ViewportSize::new(Size::new(800.0, 600.0)));
        let node = tree.children(page).unwrap()[1];
        let start = control(&tree, node).unwrap().rect.center();
        let end = start + Vec2::new(50.0, 0.0);
        draw_ui::handle_input(
            &mut tree,
            &InputEvent::PointerDown {
                position: start,
                button: PointerButton::Left,
            },
        );
        draw_ui::handle_input(&mut tree, &InputEvent::PointerMove { position: end });
        draw_ui::handle_input(
            &mut tree,
            &InputEvent::PointerUp {
                position: end,
                button: PointerButton::Left,
            },
        );
        width.get()
    }

    #[test]
    fn dragging_right_grows_a_target_on_the_left() {
        assert!((dragged_width(false) - 250.0).abs() < 1e-3);
    }

    #[test]
    fn invert_flips_the_drag_direction() {
        assert!((dragged_width(true) - 150.0).abs() < 1e-3);
    }
}
