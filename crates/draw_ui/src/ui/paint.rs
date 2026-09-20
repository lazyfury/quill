//! Emitting control visuals and debug bounds into a `DrawList`.

use super::*;
use crate::control::{control_of, control_visible};
use crate::debug::DebugDrawOptions;
use draw_core::{Rect, Vec2};
use draw_render::{PaintContext, TextAlign};
use draw_scene::SceneTree;

impl Ui {
    /// Emits control visuals into `ctx` in draw order.
    pub fn paint(&self, tree: &SceneTree, ctx: &mut PaintContext) {
        // Hold the root's text cache and measurer for the whole pass (created
        // empty / default if the tree has never been laid out).
        let root = crate::control::root_state(tree);
        let mut fallback = crate::control::LayoutCache::default();
        let mut cache_ref = root.map(|state| state.layout.borrow_mut());
        let cache: &mut crate::control::LayoutCache =
            cache_ref.as_deref_mut().unwrap_or(&mut fallback);
        let measurer: &dyn TextMeasurer = root
            .map(|state| state.text_measurer.as_ref())
            .unwrap_or(&crate::control::DEFAULT_MEASURER);
        // Every control draws under the clip its layout resolved. Emitting that
        // clip at the boundaries of a region (rather than once per control)
        // keeps a scrolling list at one `Save`/`ClipRect` pair for all of its
        // rows, and costs nothing at all while no control clips.
        let mut active: Option<Rect> = None;
        for id in tree.iter_visible() {
            let Some(control) = control_of(tree, id) else {
                continue;
            };
            // A node hidden at runtime (e.g. a router switch) is skipped even if
            // `SceneTree::update` has not run since it was hidden.
            if !control_visible(tree, id) {
                continue;
            }
            let clip = control.data.clip_rect;
            // Clipped away entirely: the intersection of its ancestors' clips
            // and its own rectangle is empty, so none of it can be seen.
            if clip.is_some_and(Rect::is_empty) {
                continue;
            }
            if clip != active {
                if active.is_some() {
                    ctx.restore();
                }
                active = clip;
                if let Some(rect) = clip {
                    ctx.save();
                    ctx.clip_rect(rect);
                }
            }
            let rect = control.data.rect;
            let state = self.state_for(tree, id);
            for decor in &control.decorations {
                decor.paint_behind(ctx, rect, state);
            }
            match &control.widget {
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
                    let lines = self.layout_text_cached(
                        cache,
                        measurer,
                        id,
                        text,
                        *font_size,
                        rect.size.width,
                        *options,
                    );
                    let step = measurer.line_height(*font_size);
                    let mut baseline = rect.top() + measurer.ascent(*font_size);
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
                        cache,
                        measurer,
                        id,
                        &button.text,
                        button.font_size,
                        inner,
                        button.options,
                    );
                    let step = measurer.line_height(button.font_size);
                    let block = lines.len() as f32 * step;
                    let mut baseline =
                        rect.center().y - block / 2.0 + measurer.ascent(button.font_size);
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
            for decor in &control.decorations {
                decor.paint_front(ctx, rect, state);
            }
        }
        if active.is_some() {
            ctx.restore();
        }
    }

    /// Draws debug bounds plus `name#id` labels for every visible control.
    ///
    /// Emits ordinary backend-neutral commands, so any backend renders it.
    /// Typical use: paint the UI first, then call this so the yellow boxes sit
    /// on top of the components.
    pub fn paint_debug(
        &self,
        tree: &SceneTree,
        ctx: &mut PaintContext,
        options: &DebugDrawOptions,
    ) {
        for id in tree.iter_visible() {
            let Some(control) = control_of(tree, id) else {
                continue;
            };
            let rect = control.data.rect;
            ctx.stroke_rect(rect, options.width, options.border_color);

            let name = tree.get(id).map_or("", |node| node.name());
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::control::Control;
    use crate::layout::TextOptions;
    use crate::widget::Widget;
    use draw_core::{Color, Edges, Size, ViewportSize};
    use draw_render::DrawCommand;
    use draw_scene::SceneTree;

    fn panel() -> Widget {
        Widget::Panel {
            color: Color::RED,
            border: None,
        }
    }

    fn label(text: &str) -> Widget {
        Widget::Label {
            text: text.to_string(),
            font_size: 12.0,
            color: Color::WHITE,
            options: TextOptions::default(),
        }
    }

    fn add(tree: &mut SceneTree, parent: NodeId, mut data: ControlData, widget: Widget) -> NodeId {
        data.anchors = Edges::ZERO;
        let id = tree.add_control(parent, "test");
        tree.set_data(id, Control::new(data, widget));
        id
    }

    fn rect(left: f32, top: f32, right: f32, bottom: f32) -> ControlData {
        ControlData {
            offsets: Edges::new(left, top, right, bottom),
            ..ControlData::default()
        }
    }

    fn paint(tree: &SceneTree) -> draw_render::DrawList {
        let mut ctx = PaintContext::new();
        crate::paint(tree, &mut ctx);
        ctx.into_draw_list()
    }

    fn clips(list: &draw_render::DrawList) -> Vec<Rect> {
        list.commands()
            .iter()
            .filter_map(|command| match command {
                DrawCommand::ClipRect(rect) => Some(*rect),
                _ => None,
            })
            .collect()
    }

    /// Backward compatibility: a tree that never asks to clip is painted
    /// exactly as before — not a single extra command.
    #[test]
    fn a_tree_without_clips_emits_no_clip_commands() {
        let mut tree = SceneTree::new();
        let root = tree.root();
        let container = add(&mut tree, root, ControlData::fill_parent(), panel());
        add(
            &mut tree,
            container,
            rect(0.0, 0.0, 100.0, 20.0),
            label("a"),
        );
        add(
            &mut tree,
            container,
            rect(0.0, 20.0, 100.0, 40.0),
            label("b"),
        );

        crate::layout(&mut tree, ViewportSize::new(Size::new(200.0, 200.0)));
        let list = paint(&tree);

        assert!(clips(&list).is_empty());
        assert!(!list
            .commands()
            .iter()
            .any(|command| matches!(command, DrawCommand::Save | DrawCommand::Restore)));
    }

    /// A clipped region costs one pair of commands no matter how much is inside
    /// it, and the clip is the region's own rectangle.
    #[test]
    fn a_clipped_region_pushes_one_clip_before_its_content() {
        let mut tree = SceneTree::new();
        let root = tree.root();
        let container = add(&mut tree, root, ControlData::fill_parent(), panel());
        let mut clipper = rect(10.0, 10.0, 110.0, 60.0);
        clipper.clip = true;
        let clipper = add(&mut tree, container, clipper, panel());
        for index in 0..3 {
            let top = index as f32 * 20.0;
            add(
                &mut tree,
                clipper,
                rect(0.0, top, 100.0, top + 20.0),
                label("row"),
            );
        }

        crate::layout(&mut tree, ViewportSize::new(Size::new(200.0, 200.0)));
        let list = paint(&tree);

        let region = Rect::from_min_size(Vec2::new(10.0, 10.0), Size::new(100.0, 50.0));
        assert_eq!(clips(&list), vec![region]);
        assert_eq!(
            list.commands()
                .iter()
                .filter(|command| matches!(command, DrawCommand::Save))
                .count(),
            1
        );
        assert_eq!(
            list.commands()
                .iter()
                .filter(|command| matches!(command, DrawCommand::Restore))
                .count(),
            1
        );

        let push = list
            .commands()
            .iter()
            .position(|command| matches!(command, DrawCommand::ClipRect(_)))
            .unwrap();
        let first_row = list
            .commands()
            .iter()
            .position(|command| matches!(command, DrawCommand::DrawText { .. }))
            .unwrap();
        assert!(push < first_row, "the clip is pushed before the rows");
        assert!(matches!(list.commands().last(), Some(DrawCommand::Restore)));
    }

    /// Intersecting an inherited clip with a clipper that misses it leaves
    /// nothing to draw, so the whole subtree is dropped instead of being sent
    /// to the backend to be scissored away.
    #[test]
    fn a_subtree_clipped_away_emits_nothing() {
        let mut tree = SceneTree::new();
        let root = tree.root();
        let container = add(&mut tree, root, ControlData::fill_parent(), panel());
        let mut outer = rect(0.0, 0.0, 100.0, 100.0);
        outer.clip = true;
        let outer = add(&mut tree, container, outer, panel());

        let mut far = rect(500.0, 500.0, 600.0, 600.0);
        far.clip = true;
        let far = add(&mut tree, outer, far, panel());
        add(&mut tree, far, rect(0.0, 0.0, 50.0, 20.0), label("hidden"));

        crate::layout(&mut tree, ViewportSize::new(Size::new(800.0, 800.0)));
        let list = paint(&tree);

        assert!(
            !list.commands().iter().any(|command| matches!(
                command,
                DrawCommand::DrawText { text, .. } if text == "hidden"
            )),
            "nothing of the clipped-away subtree is emitted"
        );
        assert_eq!(
            clips(&list),
            vec![Rect::from_min_size(Vec2::ZERO, Size::new(100.0, 100.0))],
            "only the real clip reaches the backend; the empty intersection \
             never does"
        );
    }

    /// A control that sits outside the clip is still painted — the backend
    /// scissor is what removes it — but the point is that it is tagged with the
    /// clip, not with nothing.
    #[test]
    fn a_control_outside_the_clip_still_draws_under_it() {
        let mut tree = SceneTree::new();
        let root = tree.root();
        let container = add(&mut tree, root, ControlData::fill_parent(), panel());
        let mut clipper = rect(0.0, 0.0, 100.0, 100.0);
        clipper.clip = true;
        let clipper = add(&mut tree, container, clipper, panel());
        let sticking_out = add(
            &mut tree,
            clipper,
            rect(0.0, 0.0, 300.0, 20.0),
            label("overhang"),
        );

        crate::layout(&mut tree, ViewportSize::new(Size::new(400.0, 400.0)));
        let list = paint(&tree);

        assert_eq!(
            crate::control(&tree, sticking_out).unwrap().clip_rect,
            Some(Rect::from_min_size(Vec2::ZERO, Size::new(100.0, 100.0)))
        );
        assert!(list.commands().iter().any(|command| matches!(
            command,
            DrawCommand::DrawText { text, .. } if text == "overhang"
        )));
    }
}
