//! Adding controls and setting their properties.

use super::*;
use crate::component::{Component, ControlRef};
use crate::widget::{BoxLayout, ButtonData};
use draw_core::{Color, Size};

impl Ui {
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
}
