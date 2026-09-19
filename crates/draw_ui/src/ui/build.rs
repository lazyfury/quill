//! Adding controls and setting their properties.

use super::*;
use crate::component::{Component, ControlRef};
use crate::layout::{FlexStyle, GridStyle, LayoutStyle, SizeBasis, TextOptions};
use crate::widget::ButtonData;
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
                options: TextOptions::default(),
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

    /// Adds a vertical flex container (`Flex` with a column direction).
    pub fn add_vbox(&mut self, parent: NodeId) -> NodeId {
        self.insert(
            parent,
            "VBox",
            ControlData::fill_parent(),
            Widget::Flex(FlexStyle::column()),
        )
    }

    /// Adds a horizontal flex container (`Flex` with a row direction).
    pub fn add_hbox(&mut self, parent: NodeId) -> NodeId {
        self.insert(
            parent,
            "HBox",
            ControlData::fill_parent(),
            Widget::Flex(FlexStyle::row()),
        )
    }

    pub fn add_flex(&mut self, parent: NodeId, style: FlexStyle) -> NodeId {
        self.insert(
            parent,
            "Flex",
            ControlData::fill_parent(),
            Widget::Flex(style),
        )
    }

    pub fn add_grid(&mut self, parent: NodeId, style: GridStyle) -> NodeId {
        self.insert(
            parent,
            "Grid",
            ControlData::fill_parent(),
            Widget::Grid(style),
        )
    }

    pub(crate) fn insert(
        &mut self,
        parent: NodeId,
        name: &str,
        control: ControlData,
        widget: Widget,
    ) -> NodeId {
        let id = self.tree.add_control(parent, name);
        self.controls.insert(id, control);
        self.widgets.insert(id, widget);
        self.mark_dirty(id);
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

    /// Replaces a control's layout participation (grow/shrink/basis/align/grid).
    pub fn set_layout_style(&mut self, id: NodeId, style: LayoutStyle) -> bool {
        self.with_control(id, |control| control.layout = style)
    }

    pub fn set_flex_grow(&mut self, id: NodeId, grow: f32) -> bool {
        self.with_control(id, |control| control.layout.grow = grow)
    }

    pub fn set_flex_shrink(&mut self, id: NodeId, shrink: f32) -> bool {
        self.with_control(id, |control| control.layout.shrink = shrink)
    }

    pub fn set_flex_basis(&mut self, id: NodeId, basis: SizeBasis) -> bool {
        self.with_control(id, |control| control.layout.basis = basis)
    }

    pub fn set_text(&mut self, id: NodeId, text: impl Into<String>) -> bool {
        let Some(widget) = self.widgets.get_mut(&id) else {
            return false;
        };
        if widget.set_text(text) {
            self.mark_dirty(id);
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
                self.mark_dirty(id);
                true
            }
            None => false,
        }
    }
}
