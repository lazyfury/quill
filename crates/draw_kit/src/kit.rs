//! The [`Kit`] runtime: theme state, surfaces and interactive controls.
//!
//! `Kit` layers themed chrome on top of a [`Ui`] without extending the core
//! `Widget` enum:
//!
//! - **Surfaces** are drawn before [`Ui::paint`] so content sits on top.
//! - **Foregrounds** are drawn after [`Ui::paint`] for indicators and marks.
//! - **Interactions** are hit-tested by walking up from [`Ui::hit_test`], so a
//!   click on any descendant of a component root activates it.
//!
//! ```ignore
//! kit.paint_surfaces(&ui, &mut ctx);
//! ui.paint(&mut ctx);
//! kit.paint_foreground(&ui, &mut ctx);
//! ```

use std::cell::RefCell;
use std::rc::Rc;

use draw_core::{InputEvent, NodeId, PointerButton, Rect, Vec2};
use draw_render::PaintContext;
use draw_theme::Theme;
use draw_ui::{ControlRef, Ui};

use crate::paint::{self, SurfaceStyle};

/// Visual state of a component for a given frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct InteractState {
    pub hovered: bool,
    pub pressed: bool,
    pub focused: bool,
}

type ForegroundFn = Box<dyn Fn(&mut PaintContext, Rect, &Theme, InteractState)>;
/// A surface that either has a fixed style or resolves one per frame.
enum SurfacePaint {
    Static(SurfaceStyle),
    Dynamic(Box<dyn Fn(&Theme, InteractState) -> SurfaceStyle>),
}
type Callback = Rc<RefCell<dyn FnMut()>>;

struct Interaction {
    node: NodeId,
    callback: Callback,
}

/// The themed component runtime.
pub struct Kit {
    theme: Theme,
    surfaces: Vec<(NodeId, SurfacePaint)>,
    foregrounds: Vec<(NodeId, ForegroundFn)>,
    interactions: Vec<Interaction>,
    hovered: Option<NodeId>,
    pressed: Option<NodeId>,
}

impl Kit {
    /// Creates a kit for `theme`.
    pub fn new(theme: Theme) -> Self {
        Self {
            theme,
            surfaces: Vec::new(),
            foregrounds: Vec::new(),
            interactions: Vec::new(),
            hovered: None,
            pressed: None,
        }
    }

    pub fn theme(&self) -> &Theme {
        &self.theme
    }

    /// Replaces the theme. Surfaces keep their already-resolved colors; call
    /// this before mounting components when switching modes.
    pub fn set_theme(&mut self, theme: Theme) {
        self.theme = theme;
    }

    /// Mounts a component under `parent`.
    pub fn add<C: crate::Component>(
        &mut self,
        ui: &mut Ui,
        parent: NodeId,
        component: C,
    ) -> ControlRef {
        component.mount(self, ui, parent)
    }

    /// Registers a rounded surface to be painted behind `node`.
    pub fn surface(&mut self, node: NodeId, style: SurfaceStyle) {
        self.surfaces.push((node, SurfacePaint::Static(style)));
    }

    /// Registers a surface whose style is recomputed from the theme and the
    /// node's interaction state each frame (selection, hover, active states).
    pub fn dynamic_surface(
        &mut self,
        node: NodeId,
        style: impl Fn(&Theme, InteractState) -> SurfaceStyle + 'static,
    ) {
        self.surfaces
            .push((node, SurfacePaint::Dynamic(Box::new(style))));
    }

    /// Registers chrome to be painted in front of `node`'s content.
    pub fn foreground(
        &mut self,
        node: NodeId,
        draw: impl Fn(&mut PaintContext, Rect, &Theme, InteractState) + 'static,
    ) {
        self.foregrounds.push((node, Box::new(draw)));
    }

    /// Registers a click callback for `node` and all of its descendants.
    pub fn on_click(&mut self, node: NodeId, callback: impl FnMut() + 'static) {
        self.interactions.push(Interaction {
            node,
            callback: Rc::new(RefCell::new(callback)),
        });
    }

    pub fn hovered(&self) -> Option<NodeId> {
        self.hovered
    }

    pub fn pressed(&self) -> Option<NodeId> {
        self.pressed
    }

    /// Resolves the interaction state for `node`.
    ///
    /// A component root owns its interaction, so descendants inherit the
    /// hovered/pressed state of the root they live under.
    pub fn state_for(&self, ui: &Ui, node: NodeId) -> InteractState {
        InteractState {
            hovered: self
                .hovered
                .is_some_and(|h| is_self_or_ancestor(ui, Some(h), node)),
            pressed: self
                .pressed
                .is_some_and(|p| is_self_or_ancestor(ui, Some(p), node)),
            focused: is_self_or_ancestor(ui, ui.focused(), node),
        }
    }

    /// Paints every registered surface. Call before [`Ui::paint`].
    pub fn paint_surfaces(&self, ui: &Ui, ctx: &mut PaintContext) {
        for (node, surface) in &self.surfaces {
            let Some(control) = ui.control(*node) else {
                continue;
            };
            let style = match surface {
                SurfacePaint::Static(style) => *style,
                SurfacePaint::Dynamic(resolve) => resolve(&self.theme, self.state_for(ui, *node)),
            };
            paint::surface(ctx, control.rect, &style);
        }
    }

    /// Paints every registered foreground. Call after [`Ui::paint`].
    pub fn paint_foreground(&self, ui: &Ui, ctx: &mut PaintContext) {
        for (node, draw) in &self.foregrounds {
            if let Some(control) = ui.control(*node) {
                let state = self.state_for(ui, *node);
                draw(ctx, control.rect, &self.theme, state);
            }
        }
    }

    /// Routes an input event to kit interactions.
    ///
    /// Returns `true` when the kit consumed the event. Hosts still forward the
    /// event to [`Ui::handle_input`] for regular controls.
    pub fn handle_input(&mut self, ui: &Ui, event: &InputEvent) -> bool {
        match event {
            InputEvent::PointerMove { position } => {
                self.hovered = self.hit_component(ui, *position);
                self.hovered.is_some()
            }
            InputEvent::PointerLeave => {
                self.hovered = None;
                false
            }
            InputEvent::PointerDown {
                position,
                button: PointerButton::Left,
            } => {
                let hit = self.hit_component(ui, *position);
                self.hovered = hit;
                self.pressed = hit;
                hit.is_some()
            }
            InputEvent::PointerUp {
                position,
                button: PointerButton::Left,
            } => {
                let hit = self.hit_component(ui, *position);
                let pressed = self.pressed.take();
                let mut consumed = pressed.is_some();
                if let (Some(pressed), Some(hit)) = (pressed, hit) {
                    if pressed == hit {
                        self.fire(pressed);
                        consumed = true;
                    }
                }
                consumed
            }
            _ => false,
        }
    }

    fn fire(&mut self, node: NodeId) {
        if let Some(index) = self.interactions.iter().position(|i| i.node == node) {
            let callback = self.interactions[index].callback.clone();
            (callback.borrow_mut())();
        }
    }

    fn hit_component(&self, ui: &Ui, position: Vec2) -> Option<NodeId> {
        let mut current = ui.hit_test(position);
        while let Some(id) = current {
            if self.interactions.iter().any(|i| i.node == id) {
                return Some(id);
            }
            current = ui.tree().parent(id);
        }
        None
    }
}

fn is_self_or_ancestor(ui: &Ui, candidate: Option<NodeId>, node: NodeId) -> bool {
    let mut current = candidate;
    while let Some(id) = current {
        if id == node {
            return true;
        }
        current = ui.tree().parent(id);
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    use draw_core::{InputEvent, PointerButton, Size, Viewport};
    use draw_theme::Theme;
    use draw_ui::{Panel, Ui};

    fn build() -> (Kit, Ui, NodeId, Rc<Cell<u32>>) {
        let mut ui = Ui::new();
        let mut kit = Kit::new(Theme::dark());
        let node = {
            let panel = ui.add(ui.root(), Panel::new());
            panel.id()
        };
        let clicks = Rc::new(Cell::new(0));
        let counter = clicks.clone();
        kit.on_click(node, move || counter.set(counter.get() + 1));
        kit.surface(node, SurfaceStyle::new(draw_core::Color::WHITE).radius(6.0));
        ui.layout(Viewport::new(Size::new(200.0, 200.0)));
        (kit, ui, node, clicks)
    }

    #[test]
    fn click_fires_once_on_matching_press_release() {
        let (mut kit, ui, node, clicks) = build();
        let center = ui.control(node).unwrap().rect.center();
        kit.handle_input(
            &ui,
            &InputEvent::PointerDown {
                position: center,
                button: PointerButton::Left,
            },
        );
        kit.handle_input(
            &ui,
            &InputEvent::PointerUp {
                position: center,
                button: PointerButton::Left,
            },
        );
        assert_eq!(clicks.get(), 1);
        assert_eq!(kit.pressed(), None);
    }

    #[test]
    fn release_outside_does_not_fire() {
        let (mut kit, ui, node, clicks) = build();
        let center = ui.control(node).unwrap().rect.center();
        kit.handle_input(
            &ui,
            &InputEvent::PointerDown {
                position: center,
                button: PointerButton::Left,
            },
        );
        kit.handle_input(
            &ui,
            &InputEvent::PointerUp {
                position: Vec2::new(500.0, 500.0),
                button: PointerButton::Left,
            },
        );
        assert_eq!(clicks.get(), 0);
    }

    #[test]
    fn hover_tracks_interactive_node() {
        let (mut kit, ui, node, _) = build();
        let center = ui.control(node).unwrap().rect.center();
        kit.handle_input(&ui, &InputEvent::PointerMove { position: center });
        assert_eq!(kit.hovered(), Some(node));
        assert!(kit.state_for(&ui, node).hovered);
    }

    #[test]
    fn surfaces_paint_behind_content() {
        let (kit, ui, _, _) = build();
        let mut ctx = PaintContext::new();
        kit.paint_surfaces(&ui, &mut ctx);
        assert!(ctx.len() > 0);
    }

    #[test]
    fn dynamic_surface_resolves_from_state() {
        let (mut kit, ui, node, _) = build();
        kit.dynamic_surface(node, |theme, state| {
            let fill = if state.hovered {
                theme.palette.accent
            } else {
                theme.palette.surface
            };
            SurfaceStyle::new(fill).radius(4.0)
        });
        // Static surface plus the dynamic one.
        let mut ctx = PaintContext::new();
        kit.paint_surfaces(&ui, &mut ctx);
        assert!(ctx.len() > 0);
    }
}
