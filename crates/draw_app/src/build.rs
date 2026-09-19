//! Building controls into a [`SceneTree`] and mutating their runtime.
//!
//! Construction creates a `Control` node and stores the `draw_ui::Control`
//! runtime in the node's extension slot. Property changes mutate that runtime
//! directly through [`control_mut`] / [`SceneTree::data_mut`]; there is no
//! parallel `set_anchors`-style method per field.

use std::cell::RefCell;
use std::rc::Rc;

use draw_core::{Color, NodeId};
use draw_scene::SceneTree;
use draw_ui::layout::{FlexStyle, GridStyle, TextOptions};
use draw_ui::{Control, ControlData, Widget};

/// The callback type stored on a control.
pub type ClickCallback = Rc<RefCell<dyn FnMut()>>;

/// Borrows a control's runtime from the node's extension slot.
pub fn control_mut(tree: &mut SceneTree, id: NodeId) -> Option<&mut Control> {
    tree.data_mut::<Control>(id)
}

/// Inserts a control node carrying `data` + `widget` under `parent`.
pub fn insert(
    tree: &mut SceneTree,
    parent: NodeId,
    name: &str,
    data: ControlData,
    widget: Widget,
) -> NodeId {
    let id = tree.add_control(parent, name);
    tree.set_data(id, Control::new(data, widget));
    draw_ui::mark_dirty(tree, id);
    id
}

/// Adds a card/background control under `parent`.
pub fn add_panel(tree: &mut SceneTree, parent: NodeId) -> NodeId {
    insert(
        tree,
        parent,
        "Panel",
        ControlData::fill_parent(),
        Widget::Panel {
            color: Color::new(0.13, 0.15, 0.20, 1.0),
            border: Some(Color::new(0.26, 0.30, 0.40, 1.0)),
        },
    )
}

/// Adds a text label under `parent`.
pub fn add_label(tree: &mut SceneTree, parent: NodeId, text: impl Into<String>) -> NodeId {
    insert(
        tree,
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

/// Adds a button under `parent`.
pub fn add_button(tree: &mut SceneTree, parent: NodeId, text: impl Into<String>) -> NodeId {
    insert(
        tree,
        parent,
        "Button",
        ControlData::default(),
        Widget::Button(draw_ui::ButtonData::new(text)),
    )
}

/// Adds a vertical flex container under `parent`.
pub fn add_vbox(tree: &mut SceneTree, parent: NodeId) -> NodeId {
    insert(
        tree,
        parent,
        "VBox",
        ControlData::fill_parent(),
        Widget::Flex(FlexStyle::column()),
    )
}

/// Adds a horizontal flex container under `parent`.
pub fn add_hbox(tree: &mut SceneTree, parent: NodeId) -> NodeId {
    insert(
        tree,
        parent,
        "HBox",
        ControlData::fill_parent(),
        Widget::Flex(FlexStyle::row()),
    )
}

/// Adds a flex container under `parent`.
pub fn add_flex(tree: &mut SceneTree, parent: NodeId, style: FlexStyle) -> NodeId {
    insert(
        tree,
        parent,
        "Flex",
        ControlData::fill_parent(),
        Widget::Flex(style),
    )
}

/// Adds a grid container under `parent`.
pub fn add_grid(tree: &mut SceneTree, parent: NodeId, style: GridStyle) -> NodeId {
    insert(
        tree,
        parent,
        "Grid",
        ControlData::fill_parent(),
        Widget::Grid(style),
    )
}

/// Replaces a control's text, marking layout dirty only when it changed.
pub fn set_text(tree: &mut SceneTree, id: NodeId, text: impl Into<String>) -> bool {
    let changed = match tree.data_mut::<Control>(id) {
        Some(control) => control.widget.set_text(text),
        None => return false,
    };
    if changed {
        draw_ui::mark_dirty(tree, id);
    }
    true
}

/// Registers a click callback on `id`.
pub fn set_on_click<F>(tree: &mut SceneTree, id: NodeId, callback: F) -> bool
where
    F: FnMut() + 'static,
{
    match tree.data_mut::<Control>(id) {
        Some(control) => {
            control.callback = Some(Rc::new(RefCell::new(callback)));
            true
        }
        None => false,
    }
}

/// Mutates a control's layout data and marks the tree dirty.
pub fn update_control(tree: &mut SceneTree, id: NodeId, f: impl FnOnce(&mut ControlData)) -> bool {
    let changed = match tree.data_mut::<Control>(id) {
        Some(control) => {
            f(&mut control.data);
            true
        }
        None => false,
    };
    if changed {
        draw_ui::mark_dirty(tree, id);
    }
    changed
}
