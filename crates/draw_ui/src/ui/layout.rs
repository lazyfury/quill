//! Resolving control rectangles: anchors/offsets and container arrangement.

use super::*;
use draw_core::{Rect, Size, Vec2, Viewport};

impl Ui {
    /// Resolves every control's absolute rectangle against the viewport and
    /// refreshes scene visibility.
    pub fn layout(&mut self, viewport: Viewport) {
        let viewport_rect = viewport.logical_rect();
        if let Some(root) = self.controls.get_mut(&self.root) {
            root.rect = viewport_rect;
        }
        for child in self.children_vec(self.root) {
            self.layout_node(child, viewport_rect);
        }
        self.tree.update();
    }

    fn layout_node(&mut self, id: NodeId, parent_rect: Rect) {
        let parent_is_container = self
            .tree
            .parent(id)
            .and_then(|parent| self.widgets.get(&parent))
            .is_some_and(Widget::is_container);

        let rect = if parent_is_container {
            // Already assigned by the parent container.
            self.controls
                .get(&id)
                .map_or(parent_rect, |control| control.rect)
        } else {
            match self.controls.get(&id).copied() {
                Some(control) => {
                    let resolved = control.resolve_rect(parent_rect);
                    if let Some(entry) = self.controls.get_mut(&id) {
                        entry.rect = resolved;
                    }
                    resolved
                }
                None => parent_rect,
            }
        };

        if self.widgets.get(&id).is_some_and(Widget::is_container) {
            let children = self.children_vec(id);
            self.arrange_container(id, rect, &children);
        }

        for child in self.children_vec(id) {
            self.layout_node(child, rect);
        }
    }

    fn arrange_container(&mut self, id: NodeId, rect: Rect, children: &[NodeId]) {
        let Some(widget) = self.widgets.get(&id).cloned() else {
            return;
        };
        let (vertical, layout) = match widget {
            Widget::VBox(layout) => (true, layout),
            Widget::HBox(layout) => (false, layout),
            _ => return,
        };

        let content = Rect::from_min_size(
            Vec2::new(
                rect.left() + layout.padding.left,
                rect.top() + layout.padding.top,
            ),
            Size::new(
                (rect.size.width - layout.padding.horizontal()).max(0.0),
                (rect.size.height - layout.padding.vertical()).max(0.0),
            ),
        );

        let mut cursor = if vertical {
            content.top()
        } else {
            content.left()
        };

        for child in children {
            let Some(control) = self.controls.get(child).copied() else {
                continue;
            };
            let child_rect = if vertical {
                let height = control.min_size.height;
                let rect = Rect::from_min_size(
                    Vec2::new(content.left(), cursor),
                    Size::new(content.size.width, height),
                );
                cursor += height + layout.separation;
                rect
            } else {
                let width = control.min_size.width;
                let rect = Rect::from_min_size(
                    Vec2::new(cursor, content.top()),
                    Size::new(width, content.size.height),
                );
                cursor += width + layout.separation;
                rect
            };
            if let Some(entry) = self.controls.get_mut(child) {
                entry.rect = child_rect;
            }
        }
    }
}
