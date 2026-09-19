//! Reusable component API.
//!
//! Components are small builder structs mounted into a [`Ui`]:
//!
//! ```ignore
//! let panel = ui.add(ui.root(), Panel::new());
//! let vbox = ui.add(panel.id(), VBox::new());
//! let label = ui.add(vbox.id(), Label::new("Hello"));
//! let button = ui.add(vbox.id(), Button::new("Click me").on_click(|| { /* ... */ }));
//! ```
//!
//! Layout is applied per-frame via [`Ui::layout`](crate::Ui::layout); state
//! changes only touch core data and the next `paint` produces a fresh
//! `DrawList`.

use draw_core::{Color, Edges, NodeId};

use crate::control::ControlData;
use crate::ui::Ui;
use crate::widget::{BoxLayout, ButtonData, Widget};

/// An owned handle to a mounted control.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ControlRef {
    id: NodeId,
}

impl ControlRef {
    pub fn new(id: NodeId) -> Self {
        Self { id }
    }

    pub fn id(self) -> NodeId {
        self.id
    }
}

impl From<ControlRef> for NodeId {
    fn from(control: ControlRef) -> Self {
        control.id
    }
}

/// Something that can be mounted into a [`Ui`].
pub trait Component {
    fn mount(self, ui: &mut Ui, parent: NodeId) -> ControlRef;
}

/// A card/background control. Fills its parent by default.
#[derive(Debug, Clone, Copy)]
pub struct Panel {
    color: Color,
    border: Option<Color>,
}

impl Default for Panel {
    fn default() -> Self {
        Self {
            color: Color::new(0.13, 0.15, 0.20, 1.0),
            border: Some(Color::new(0.26, 0.30, 0.40, 1.0)),
        }
    }
}

impl Panel {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    pub fn border(mut self, border: Option<Color>) -> Self {
        self.border = border;
        self
    }

    pub fn flat(mut self) -> Self {
        self.border = None;
        self
    }
}

impl Component for Panel {
    fn mount(self, ui: &mut Ui, parent: NodeId) -> ControlRef {
        ControlRef::new(ui.insert(
            parent,
            "Panel",
            ControlData::fill_parent(),
            Widget::Panel {
                color: self.color,
                border: self.border,
            },
        ))
    }
}

/// A text label.
#[derive(Debug, Clone)]
pub struct Label {
    text: String,
    font_size: f32,
    color: Color,
}

impl Label {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            font_size: 20.0,
            color: Color::new(0.92, 0.94, 0.98, 1.0),
        }
    }

    pub fn font_size(mut self, font_size: f32) -> Self {
        self.font_size = font_size;
        self
    }

    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }
}

impl Component for Label {
    fn mount(self, ui: &mut Ui, parent: NodeId) -> ControlRef {
        ControlRef::new(ui.insert(
            parent,
            "Label",
            ControlData::default(),
            Widget::Label {
                text: self.text,
                font_size: self.font_size,
                color: self.color,
            },
        ))
    }
}

/// A clickable button with an optional click callback.
pub struct Button {
    text: String,
    on_click: Option<Box<dyn FnMut()>>,
}

impl Button {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            on_click: None,
        }
    }

    /// Registers a callback invoked when the button is clicked (or activated
    /// with Enter/Space).
    pub fn on_click(mut self, callback: impl FnMut() + 'static) -> Self {
        self.on_click = Some(Box::new(callback));
        self
    }
}

impl Component for Button {
    fn mount(self, ui: &mut Ui, parent: NodeId) -> ControlRef {
        let id = ui.insert(
            parent,
            "Button",
            ControlData::default(),
            Widget::Button(ButtonData::new(self.text)),
        );
        if let Some(callback) = self.on_click {
            ui.set_on_click(id, callback);
        }
        ControlRef::new(id)
    }
}

/// A vertical stacking container.
#[derive(Debug, Clone, Copy)]
pub struct VBox {
    separation: f32,
    padding: Edges,
}

impl Default for VBox {
    fn default() -> Self {
        Self {
            separation: 8.0,
            padding: Edges::all(16.0),
        }
    }
}

impl VBox {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn separation(mut self, separation: f32) -> Self {
        self.separation = separation;
        self
    }

    pub fn padding(mut self, padding: Edges) -> Self {
        self.padding = padding;
        self
    }
}

impl Component for VBox {
    fn mount(self, ui: &mut Ui, parent: NodeId) -> ControlRef {
        ControlRef::new(ui.insert(
            parent,
            "VBox",
            ControlData::fill_parent(),
            Widget::VBox(BoxLayout {
                separation: self.separation,
                padding: self.padding,
            }),
        ))
    }
}

/// A horizontal stacking container.
#[derive(Debug, Clone, Copy)]
pub struct HBox {
    separation: f32,
    padding: Edges,
}

impl Default for HBox {
    fn default() -> Self {
        Self {
            separation: 8.0,
            padding: Edges::all(16.0),
        }
    }
}

impl HBox {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn separation(mut self, separation: f32) -> Self {
        self.separation = separation;
        self
    }

    pub fn padding(mut self, padding: Edges) -> Self {
        self.padding = padding;
        self
    }
}

impl Component for HBox {
    fn mount(self, ui: &mut Ui, parent: NodeId) -> ControlRef {
        ControlRef::new(ui.insert(
            parent,
            "HBox",
            ControlData::fill_parent(),
            Widget::HBox(BoxLayout {
                separation: self.separation,
                padding: self.padding,
            }),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::rc::Rc;

    use draw_core::{InputEvent, PointerButton, Size, Viewport};

    #[test]
    fn components_compose_and_click() {
        let mut ui = Ui::new();
        let clicks = Rc::new(Cell::new(0));

        let panel = ui.add(ui.root(), Panel::new());
        let vbox = ui.add(panel.id(), VBox::new());
        let label = ui.add(vbox.id(), Label::new("Hello"));
        let counter = clicks.clone();
        let button = ui.add(
            vbox.id(),
            Button::new("Click me").on_click(move || counter.set(counter.get() + 1)),
        );

        ui.layout(Viewport::new(Size::new(800.0, 600.0)));

        let label_rect = ui.control(label.id()).unwrap().rect;
        let button_rect = ui.control(button.id()).unwrap().rect;
        assert!(label_rect.top() < button_rect.top());

        let center = button_rect.center();
        ui.handle_input(&InputEvent::PointerDown {
            position: center,
            button: PointerButton::Left,
        });
        ui.handle_input(&InputEvent::PointerUp {
            position: center,
            button: PointerButton::Left,
        });

        assert_eq!(clicks.get(), 1);
        assert_eq!(ui.click_count(button.id()), 1);
    }

    #[test]
    fn control_ref_converts_to_node_id() {
        let mut ui = Ui::new();
        let label = ui.add(ui.root(), Label::new("x"));
        let id: NodeId = label.into();
        assert_eq!(id, label.id());
    }
}
