//! Emitting control visuals and debug bounds into a `DrawList`.

use super::*;
use crate::debug::DebugDrawOptions;
use draw_core::Vec2;
use draw_render::{PaintContext, TextAlign};

impl Ui {
    /// Emits control visuals into `ctx` in draw order.
    pub fn paint(&self, ctx: &mut PaintContext) {
        for id in self.tree.iter_visible() {
            let (Some(control), Some(widget)) = (self.controls.get(&id), self.widgets.get(&id))
            else {
                continue;
            };
            let rect = control.rect;
            match widget {
                Widget::Panel { color, border } => {
                    ctx.fill_rect(rect, *color);
                    if let Some(border) = border {
                        ctx.stroke_rect(rect, 1.0, *border);
                    }
                }
                Widget::Label {
                    text,
                    font_size,
                    color,
                } => {
                    let position = Vec2::new(rect.left(), rect.center().y + font_size * 0.4);
                    ctx.draw_text(text.clone(), position, *font_size, TextAlign::Left, *color);
                }
                Widget::Button(button) => {
                    ctx.fill_rect(rect, button.fill());
                    ctx.stroke_rect(rect, 1.0, button.text_color.with_alpha(0.35));
                    let position =
                        Vec2::new(rect.center().x, rect.center().y + button.font_size * 0.4);
                    ctx.draw_text(
                        button.text.clone(),
                        position,
                        button.font_size,
                        TextAlign::Center,
                        button.text_color,
                    );
                }
                Widget::VBox(_) | Widget::HBox(_) => {}
            }
        }
    }

    /// Draws debug bounds plus `name#id` labels for every visible control.
    ///
    /// Emits ordinary backend-neutral commands, so any backend renders it.
    /// Typical use: paint the UI first, then call this so the yellow boxes sit
    /// on top of the components.
    pub fn paint_debug(&self, ctx: &mut PaintContext, options: &DebugDrawOptions) {
        for id in self.tree.iter_visible() {
            let Some(control) = self.controls.get(&id) else {
                continue;
            };
            let rect = control.rect;
            ctx.stroke_rect(rect, options.width, options.border_color);

            let name = self.tree.get(id).map_or("", |node| node.name());
            let label = options.label(name, id);
            if label.is_empty() {
                continue;
            }
            let position = Vec2::new(
                rect.left() + options.label_offset.x,
                rect.top() + options.label_offset.y + options.font_size,
            );
            ctx.draw_text(
                label,
                position,
                options.font_size,
                TextAlign::Left,
                options.text_color,
            );
        }
    }
}
