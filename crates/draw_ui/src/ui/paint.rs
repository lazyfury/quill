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
                    options,
                } => {
                    let lines =
                        self.layout_text_cached(id, text, *font_size, rect.size.width, *options);
                    let step = self.text_measurer.line_height(*font_size);
                    let mut baseline = rect.top() + self.text_measurer.ascent(*font_size);
                    for line in lines.iter() {
                        ctx.draw_text(
                            line.clone(),
                            Vec2::new(rect.left(), baseline),
                            *font_size,
                            TextAlign::Left,
                            *color,
                        );
                        baseline += step;
                    }
                }
                Widget::Button(button) => {
                    ctx.fill_rect(rect, button.fill());
                    ctx.stroke_rect(rect, 1.0, button.text_color.with_alpha(0.35));

                    let inner = (rect.size.width - 32.0).max(0.0);
                    let lines = self.layout_text_cached(
                        id,
                        &button.text,
                        button.font_size,
                        inner,
                        button.options,
                    );
                    let step = self.text_measurer.line_height(button.font_size);
                    let block = lines.len() as f32 * step;
                    let mut baseline =
                        rect.center().y - block / 2.0 + self.text_measurer.ascent(button.font_size);
                    for line in lines.iter() {
                        ctx.draw_text(
                            line.clone(),
                            Vec2::new(rect.center().x, baseline),
                            button.font_size,
                            TextAlign::Center,
                            button.text_color,
                        );
                        baseline += step;
                    }
                }
                Widget::Flex(_) | Widget::Grid(_) => {}
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
