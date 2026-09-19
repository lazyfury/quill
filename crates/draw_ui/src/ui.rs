use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use draw_core::{
    Color, Edges, EventResult, InputEvent, Key, NodeId, PointerButton, Rect, Size, Vec2, Viewport,
};
use draw_render::{PaintContext, TextAlign};
use draw_scene::SceneTree;

use crate::component::{Component, ControlRef};
use crate::control::{ControlData, MouseFilter};
use crate::widget::{BoxLayout, ButtonData, ButtonState, Widget};

/// A callback invoked when a control is activated (clicked / Enter).
pub type ClickCallback = Rc<RefCell<dyn FnMut()>>;

/// The UI tree: a [`SceneTree`] of `Control` nodes plus layout, painting and
/// input dispatch.
///
/// Layout is absolute: after [`Ui::layout`], each control has a resolved
/// viewport-space [`Rect`](draw_core::Rect) in its [`ControlData`]. Painting
/// iterates the scene tree in draw order, and input uses reverse-order hit
/// testing.
pub struct Ui {
    tree: SceneTree,
    root: NodeId,
    controls: HashMap<NodeId, ControlData>,
    widgets: HashMap<NodeId, Widget>,
    callbacks: HashMap<NodeId, ClickCallback>,
    hovered: Option<NodeId>,
    pressed: Option<NodeId>,
    focused: Option<NodeId>,
    activated: Vec<NodeId>,
}

impl Default for Ui {
    fn default() -> Self {
        Self::new()
    }
}

impl Ui {
    /// Creates a UI with a root control that fills the viewport.
    pub fn new() -> Self {
        let mut tree = SceneTree::new();
        let tree_root = tree.root();
        let root = tree.add_control(tree_root, "Root");

        let mut controls = HashMap::new();
        controls.insert(
            root,
            ControlData {
                anchors: Edges::new(0.0, 0.0, 1.0, 1.0),
                mouse_filter: MouseFilter::Ignore,
                ..ControlData::default()
            },
        );

        Self {
            tree,
            root,
            controls,
            widgets: HashMap::new(),
            callbacks: HashMap::new(),
            hovered: None,
            pressed: None,
            focused: None,
            activated: Vec::new(),
        }
    }

    pub fn tree(&self) -> &SceneTree {
        &self.tree
    }

    pub fn tree_mut(&mut self) -> &mut SceneTree {
        &mut self.tree
    }

    pub fn root(&self) -> NodeId {
        self.root
    }

    /// Number of controls in this UI (root included).
    pub fn control_count(&self) -> usize {
        self.controls.len()
    }

    pub fn control(&self, id: NodeId) -> Option<&ControlData> {
        self.controls.get(&id)
    }

    pub fn widget(&self, id: NodeId) -> Option<&Widget> {
        self.widgets.get(&id)
    }

    pub fn hovered(&self) -> Option<NodeId> {
        self.hovered
    }

    pub fn focused(&self) -> Option<NodeId> {
        self.focused
    }

    pub fn button_state(&self, id: NodeId) -> Option<ButtonState> {
        match self.widgets.get(&id) {
            Some(Widget::Button(button)) => Some(button.state),
            _ => None,
        }
    }

    pub fn click_count(&self, id: NodeId) -> u32 {
        self.button_state(id).map_or(0, |state| state.click_count)
    }

    // -- construction ------------------------------------------------------

    pub fn add_panel(&mut self, parent: NodeId) -> NodeId {
        self.insert(
            parent,
            "Panel",
            ControlData::fill_parent(),
            Widget::Panel {
                color: Color::new(0.13, 0.15, 0.20, 1.0),
                border: Some(Color::new(0.26, 0.30, 0.40, 1.0)),
            },
        )
    }

    pub fn add_label(&mut self, parent: NodeId, text: impl Into<String>) -> NodeId {
        self.insert(
            parent,
            "Label",
            ControlData::default(),
            Widget::Label {
                text: text.into(),
                font_size: 20.0,
                color: Color::new(0.92, 0.94, 0.98, 1.0),
            },
        )
    }

    pub fn add_button(&mut self, parent: NodeId, text: impl Into<String>) -> NodeId {
        self.insert(
            parent,
            "Button",
            ControlData::default(),
            Widget::Button(ButtonData::new(text)),
        )
    }

    pub fn add_vbox(&mut self, parent: NodeId) -> NodeId {
        self.insert(
            parent,
            "VBox",
            ControlData::fill_parent(),
            Widget::VBox(BoxLayout::default()),
        )
    }

    pub fn add_hbox(&mut self, parent: NodeId) -> NodeId {
        self.insert(
            parent,
            "HBox",
            ControlData::fill_parent(),
            Widget::HBox(BoxLayout::default()),
        )
    }

    pub(crate) fn insert(
        &mut self,
        parent: NodeId,
        name: &str,
        mut control: ControlData,
        widget: Widget,
    ) -> NodeId {
        let id = self.tree.add_control(parent, name);
        control.min_size = control.min_size.max(widget.content_min_size());
        self.controls.insert(id, control);
        self.widgets.insert(id, widget);
        id
    }

    // -- property setters --------------------------------------------------

    pub fn set_anchors(&mut self, id: NodeId, anchors: Edges) -> bool {
        self.with_control(id, |control| control.anchors = anchors)
    }

    pub fn set_offsets(&mut self, id: NodeId, offsets: Edges) -> bool {
        self.with_control(id, |control| control.offsets = offsets)
    }

    pub fn set_min_size(&mut self, id: NodeId, min_size: Size) -> bool {
        self.with_control(id, |control| control.min_size = min_size)
    }

    pub fn set_mouse_filter(&mut self, id: NodeId, filter: MouseFilter) -> bool {
        self.with_control(id, |control| control.mouse_filter = filter)
    }

    pub fn set_text(&mut self, id: NodeId, text: impl Into<String>) -> bool {
        let Some(widget) = self.widgets.get_mut(&id) else {
            return false;
        };
        widget.set_text(text);
        let min_size = widget.content_min_size();
        if let Some(control) = self.controls.get_mut(&id) {
            control.min_size = control.min_size.max(min_size);
        }
        true
    }

    pub fn set_on_click<F>(&mut self, id: NodeId, callback: F) -> bool
    where
        F: FnMut() + 'static,
    {
        if !self.widgets.get(&id).is_some_and(Widget::is_button) {
            return false;
        }
        self.callbacks.insert(id, Rc::new(RefCell::new(callback)));
        true
    }

    /// Mounts a [`Component`] under `parent` and returns its handle.
    ///
    /// ```ignore
    /// let panel = ui.add(ui.root(), Panel::new());
    /// let vbox = ui.add(panel.id(), VBox::new());
    /// ui.add(vbox.id(), Label::new("Hello"));
    /// ui.add(vbox.id(), Button::new("Click me").on_click(|| { /* ... */ }));
    /// ```
    pub fn add<C: Component>(&mut self, parent: NodeId, component: C) -> ControlRef {
        component.mount(self, parent)
    }

    fn with_control(&mut self, id: NodeId, f: impl FnOnce(&mut ControlData)) -> bool {
        match self.controls.get_mut(&id) {
            Some(control) => {
                f(control);
                true
            }
            None => false,
        }
    }

    // -- layout ------------------------------------------------------------

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

    // -- painting ----------------------------------------------------------

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

    // -- input -------------------------------------------------------------

    /// Returns the topmost control under `position`, respecting visibility and
    /// [`MouseFilter`].
    pub fn hit_test(&self, position: Vec2) -> Option<NodeId> {
        self.hit_node(self.root, position)
    }

    fn hit_node(&self, id: NodeId, position: Vec2) -> Option<NodeId> {
        // Children are drawn after the parent, so test them first (topmost first).
        let children = self.children_vec(id);
        for child in children.iter().rev() {
            if !self.tree.is_visible_in_tree(*child).unwrap_or(false) {
                continue;
            }
            if let Some(hit) = self.hit_node(*child, position) {
                return Some(hit);
            }
        }
        let control = self.controls.get(&id)?;
        if control.mouse_filter != MouseFilter::Ignore && control.rect.contains(position) {
            Some(id)
        } else {
            None
        }
    }

    /// Dispatches an event. MVP does target routing only; capture/bubble is a
    /// future extension point.
    pub fn handle_input(&mut self, event: &InputEvent) -> EventResult {
        let result = match event {
            InputEvent::PointerMove { position } => {
                let hit = self.hit_test(*position);
                self.set_hover(hit);
                if hit.is_some() {
                    EventResult::Handled
                } else {
                    EventResult::Ignored
                }
            }
            InputEvent::PointerLeave => {
                self.set_hover(None);
                EventResult::Ignored
            }
            InputEvent::PointerDown {
                position,
                button: PointerButton::Left,
            } => {
                let hit = self.hit_test(*position);
                self.set_hover(hit);
                self.focused = hit;
                if let Some(id) = hit {
                    if self.widgets.get(&id).is_some_and(Widget::is_button) {
                        self.pressed = Some(id);
                        if let Some(Widget::Button(button)) = self.widgets.get_mut(&id) {
                            button.state.pressed = true;
                        }
                    }
                    EventResult::Handled
                } else {
                    EventResult::Ignored
                }
            }
            InputEvent::PointerUp {
                position,
                button: PointerButton::Left,
            } => {
                let hit = self.hit_test(*position);
                if let Some(pressed) = self.pressed.take() {
                    if let Some(Widget::Button(button)) = self.widgets.get_mut(&pressed) {
                        button.state.pressed = false;
                    }
                    if hit == Some(pressed) {
                        self.activate(pressed);
                    }
                }
                EventResult::Handled
            }
            InputEvent::KeyDown { key } if matches!(*key, Key::Enter | Key::Space) => {
                match self.focused {
                    Some(focused) if self.widgets.get(&focused).is_some_and(Widget::is_button) => {
                        self.activate(focused);
                        EventResult::Handled
                    }
                    _ => EventResult::Ignored,
                }
            }
            _ => EventResult::Ignored,
        };

        self.dispatch_click_callbacks();
        result
    }

    fn activate(&mut self, id: NodeId) {
        if let Some(Widget::Button(button)) = self.widgets.get_mut(&id) {
            button.state.click_count += 1;
            self.activated.push(id);
        }
    }

    fn dispatch_click_callbacks(&mut self) {
        let activated = std::mem::take(&mut self.activated);
        for id in activated {
            let callback = self.callbacks.get(&id).cloned();
            if let Some(callback) = callback {
                (callback.borrow_mut())();
            }
        }
    }

    fn set_hover(&mut self, hit: Option<NodeId>) {
        if self.hovered == hit {
            return;
        }
        if let Some(old) = self.hovered {
            if let Some(Widget::Button(button)) = self.widgets.get_mut(&old) {
                button.state.hovered = false;
            }
        }
        self.hovered = hit;
        if let Some(new) = hit {
            if let Some(Widget::Button(button)) = self.widgets.get_mut(&new) {
                button.state.hovered = true;
            }
        }
    }

    fn children_vec(&self, id: NodeId) -> Vec<NodeId> {
        self.tree
            .children(id)
            .map(|children| children.to_vec())
            .unwrap_or_default()
    }
}
