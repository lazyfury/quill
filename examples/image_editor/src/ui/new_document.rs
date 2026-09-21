//! 「新建文档」窗口：选画布尺寸 / 背景色。
//!
//! 由首页的「新建窗口」在**原生**窗口里打开（不是模态框）。点「创建」把选择
//! 通过共享格交回宿主（[`crate::app::application`]），宿主把它变成**主窗口**里的
//! 一个新 [`EditorView`](crate::ui::EditorView)；点「取消」或关窗就放弃。

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use draw_components::{Button, Component, Flex, NodeRef, Text};
use draw_core::{Color as UiColor, Edges, EventResult, InputEvent, NodeId, Vec2, ViewportSize};
use draw_render::PaintContext;
use draw_scene::{SceneChild, SceneTree};
use draw_theme::{radius, space, SurfaceLevel, TextSize, Theme, Tone};
use draw_ui::{Align, MouseFilter, SurfaceStyle, TextMeasurer};

use crate::document::Color;

/// 「新建窗口」选出来的默认值。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NewDocumentSpec {
    pub width: u32,
    pub height: u32,
    pub background: Color,
}

impl Default for NewDocumentSpec {
    fn default() -> Self {
        Self {
            width: 128,
            height: 128,
            background: Color::WHITE,
        }
    }
}

/// 侧边的最小 / 最大边长，以及步进按钮的步长（逻辑像素即文档像素）。
const MIN_SIDE: u32 = 16;
const MAX_SIDE: u32 = 2048;
const SIDE_STEP: u32 = 16;

/// 预设尺寸。
const PRESETS: [(u32, u32); 5] = [(16, 16), (64, 64), (128, 128), (256, 256), (512, 512)];

/// 背景色预设（名字，颜色）。第一个是透明。
const BACKGROUNDS: [(&str, Color); 4] = [
    ("透明", Color::TRANSPARENT),
    ("白色", Color::WHITE),
    ("黑色", Color::BLACK),
    ("红色", Color::RED),
];

/// 「新建文档」窗口给出的结果。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NewDocumentResult {
    Create(NewDocumentSpec),
    Cancel,
}

/// 「新建文档」视图。
pub struct NewDocumentView {
    tree: SceneTree,
    /// 当前选择（共享格，回调只能往里写）。
    choices: Rc<RefCell<NewDocumentSpec>>,
    /// 有改动、还没同步到标签。
    dirty: Rc<Cell<bool>>,
    /// 创建 / 取消的结果；宿主取走（[`take_result`]）后关窗。
    result: Rc<Cell<Option<NewDocumentResult>>>,
    width_label: NodeId,
    height_label: NodeId,
    background_label: NodeId,
    /// 供测试点击的节点。
    #[allow(dead_code)]
    create_button: NodeId,
    #[allow(dead_code)]
    cancel_button: NodeId,
    #[allow(dead_code)]
    presets: Vec<NodeId>,
    #[allow(dead_code)]
    backgrounds: Vec<NodeId>,
}

impl NewDocumentView {
    pub fn new(theme: Theme) -> Self {
        let choices: Rc<RefCell<NewDocumentSpec>> =
            Rc::new(RefCell::new(NewDocumentSpec::default()));
        let dirty = Rc::new(Cell::new(true));
        let result: Rc<Cell<Option<NewDocumentResult>>> = Rc::new(Cell::new(None));

        let width_ref = NodeRef::new();
        let height_ref = NodeRef::new();
        let background_ref = NodeRef::new();
        let create_ref = NodeRef::new();
        let cancel_ref = NodeRef::new();
        let mut preset_refs: Vec<NodeRef> = Vec::new();
        let mut background_refs: Vec<NodeRef> = Vec::new();

        let size_steppers = Flex::column()
            .gap(space::XS)
            .padding(Edges::ZERO)
            .mouse_filter(MouseFilter::Ignore)
            .child(stepper_row(
                theme,
                "宽度",
                &width_ref,
                choices.clone(),
                dirty.clone(),
                |spec| {
                    spec.width = spec.width.saturating_sub(SIDE_STEP).max(MIN_SIDE);
                },
                |spec| {
                    spec.width = (spec.width + SIDE_STEP).min(MAX_SIDE);
                },
            ))
            .child(stepper_row(
                theme,
                "高度",
                &height_ref,
                choices.clone(),
                dirty.clone(),
                |spec| {
                    spec.height = spec.height.saturating_sub(SIDE_STEP).max(MIN_SIDE);
                },
                |spec| {
                    spec.height = (spec.height + SIDE_STEP).min(MAX_SIDE);
                },
            ));

        let mut presets = Flex::row()
            .wrap(true)
            .gap(space::XXS)
            .padding(Edges::ZERO)
            .mouse_filter(MouseFilter::Ignore);
        for (width, height) in PRESETS {
            let slot = NodeRef::new();
            preset_refs.push(slot.clone());
            presets = presets.child(preset_button(
                theme,
                width,
                height,
                choices.clone(),
                dirty.clone(),
                &slot,
            ));
        }

        let mut backgrounds = Flex::row()
            .gap(space::XS)
            .padding(Edges::ZERO)
            .align(Align::Center)
            .mouse_filter(MouseFilter::Ignore);
        for (name, color) in BACKGROUNDS {
            let slot = NodeRef::new();
            background_refs.push(slot.clone());
            backgrounds = backgrounds.child(background_swatch(
                theme,
                name,
                color,
                choices.clone(),
                dirty.clone(),
                &slot,
            ));
        }

        let buttons = {
            let choices = choices.clone();
            let result = result.clone();
            let cancel_result = result.clone();
            Flex::row()
                .gap(space::SM)
                .padding(Edges::ZERO)
                .justify(draw_ui::Justify::End)
                .mouse_filter(MouseFilter::Ignore)
                .child(
                    Button::secondary("取消", theme)
                        .on_click(move || cancel_result.set(Some(NewDocumentResult::Cancel)))
                        .ref_(&cancel_ref),
                )
                .child(
                    Button::primary("创建", theme)
                        .on_click(move || {
                            let spec = *choices.borrow();
                            result.set(Some(NewDocumentResult::Create(spec)));
                        })
                        .ref_(&create_ref),
                )
        };

        let page = Flex::column()
            .gap(space::LG)
            .padding(Edges::all(space::HUGE))
            .mouse_filter(MouseFilter::Ignore)
            .child(Text::title("新建文档", theme))
            .child(
                Text::small("选择画布尺寸和背景色；创建后在主窗口里编辑。", theme)
                    .tone(Tone::Muted),
            )
            .child(Text::subheading("尺寸", theme))
            .child(size_steppers)
            .child(presets)
            .child(Text::subheading("背景色", theme))
            .child(backgrounds)
            .child(
                Text::caption("", theme)
                    .tone(Tone::Muted)
                    .ref_(&background_ref),
            )
            .child(buttons);

        let tree = Flex::new()
            .gap(0.0)
            .padding(Edges::ZERO)
            .background(theme.surface(SurfaceLevel::Base))
            .mouse_filter(MouseFilter::Ignore)
            .child(page)
            .into_tree();

        Self {
            tree,
            choices,
            dirty,
            result,
            width_label: width_ref.get().expect("宽度标签"),
            height_label: height_ref.get().expect("高度标签"),
            background_label: background_ref.get().expect("背景标签"),
            create_button: create_ref.get().expect("创建按钮"),
            cancel_button: cancel_ref.get().expect("取消按钮"),
            presets: preset_refs
                .into_iter()
                .map(|slot| slot.get().expect("预设按钮"))
                .collect(),
            backgrounds: background_refs
                .into_iter()
                .map(|slot| slot.get().expect("背景色块"))
                .collect(),
        }
    }

    pub fn set_text_measurer(&mut self, measurer: Rc<dyn TextMeasurer>) {
        draw_ui::set_text_measurer(&mut self.tree, measurer);
    }

    /// 宿主的每一帧调用：把共享格里的选择写进标签（只在改动后）。
    pub fn update(&mut self) {
        if !self.dirty.replace(false) {
            return;
        }
        let (width, height, background) = {
            let spec = self.choices.borrow();
            (spec.width, spec.height, spec.background)
        };
        draw_components::set_text(&mut self.tree, self.width_label, format!("{width} px"));
        draw_components::set_text(&mut self.tree, self.height_label, format!("{height} px"));
        draw_components::set_text(
            &mut self.tree,
            self.background_label,
            format!("背景：{}", background_name(background)),
        );
    }

    /// 创建 / 取消的结果；取走一次。
    pub fn take_result(&self) -> Option<NewDocumentResult> {
        self.result.take()
    }

    pub fn event(&mut self, event: &InputEvent) -> EventResult {
        draw_ui::handle_input(&mut self.tree, event)
    }

    pub fn layout(&mut self, viewport: ViewportSize) {
        self.tree.set_viewport_size(viewport.logical_size());
        draw_ui::layout(&mut self.tree, viewport);
        self.tree.update();
    }

    pub fn paint(&self, ctx: &mut PaintContext) {
        self.tree.paint(ctx);
        draw_ui::paint(&self.tree, ctx);
    }

    /// 当前选择（测试用）。
    #[allow(dead_code)]
    pub fn spec(&self) -> NewDocumentSpec {
        *self.choices.borrow()
    }

    /// 创建按钮 / 预设按钮 / 背景色块的中心点（测试模拟点击用）。
    #[allow(dead_code)]
    pub fn create_center(&self) -> Option<Vec2> {
        center(&self.tree, self.create_button)
    }

    #[allow(dead_code)]
    pub fn cancel_center(&self) -> Option<Vec2> {
        center(&self.tree, self.cancel_button)
    }

    #[allow(dead_code)]
    pub fn preset_center(&self, index: usize) -> Option<Vec2> {
        center(&self.tree, *self.presets.get(index)?)
    }

    #[allow(dead_code)]
    pub fn background_center(&self, index: usize) -> Option<Vec2> {
        center(&self.tree, *self.backgrounds.get(index)?)
    }
}

fn center(tree: &SceneTree, id: NodeId) -> Option<Vec2> {
    draw_ui::control(tree, id).map(|control| control.rect.center())
}

/// 背景色的名字（状态行显示用）。
fn background_name(color: Color) -> &'static str {
    BACKGROUNDS
        .iter()
        .find(|(_, candidate)| *candidate == color)
        .map(|(name, _)| *name)
        .unwrap_or("自定义")
}

/// 一行「标签  [−]  数值  [+]」。
fn stepper_row(
    theme: Theme,
    label: &'static str,
    value_ref: &NodeRef,
    choices: Rc<RefCell<NewDocumentSpec>>,
    dirty: Rc<Cell<bool>>,
    less: fn(&mut NewDocumentSpec),
    more: fn(&mut NewDocumentSpec),
) -> impl Component {
    Flex::row()
        .gap(space::SM)
        .padding(Edges::ZERO)
        .align(Align::Center)
        .mouse_filter(MouseFilter::Ignore)
        .child(Text::small(label, theme))
        .child(step_button(
            theme,
            "−",
            choices.clone(),
            dirty.clone(),
            less,
        ))
        .child(Text::small("", theme).min_size(72.0, 0.0).ref_(value_ref))
        .child(step_button(theme, "+", choices.clone(), dirty, more))
}

fn step_button(
    theme: Theme,
    label: &'static str,
    choices: Rc<RefCell<NewDocumentSpec>>,
    dirty: Rc<Cell<bool>>,
    apply: fn(&mut NewDocumentSpec),
) -> Button {
    Button::secondary(label, theme).on_click(move || {
        apply(&mut choices.borrow_mut());
        dirty.set(true);
    })
}

fn preset_button(
    theme: Theme,
    width: u32,
    height: u32,
    choices: Rc<RefCell<NewDocumentSpec>>,
    dirty: Rc<Cell<bool>>,
    slot: &NodeRef,
) -> impl Component {
    Button::secondary(format!("{width}×{height}"), theme)
        .font_size(TextSize::Small.px())
        .on_click(move || {
            let mut spec = choices.borrow_mut();
            spec.width = width;
            spec.height = height;
            dirty.set(true);
        })
        .ref_(slot)
}

/// 一个背景色块：点击选中，选中的描一圈前景边。
fn background_swatch(
    theme: Theme,
    _name: &'static str,
    color: Color,
    choices: Rc<RefCell<NewDocumentSpec>>,
    dirty: Rc<Cell<bool>>,
    slot: &NodeRef,
) -> impl Component {
    let ui = to_ui(color);
    let selected = choices.clone();
    let clicked = choices;
    Flex::new()
        .padding(Edges::ZERO)
        .min_size(28.0, 28.0)
        .on_click(move || {
            clicked.borrow_mut().background = color;
            dirty.set(true);
        })
        .dynamic_background(move |interact| {
            let is_selected = selected.borrow().background == color;
            let style = SurfaceStyle::new(ui).radius(radius::SM);
            if is_selected || interact.hovered {
                style.border(theme.palette.foreground)
            } else {
                style.border(theme.palette.border)
            }
        })
        .ref_(slot)
}

/// 文档色（8 位 RGBA）-> UI 色（0..1）。
fn to_ui(color: Color) -> UiColor {
    UiColor::new(
        color.r as f32 / 255.0,
        color.g as f32 / 255.0,
        color.b as f32 / 255.0,
        color.a as f32 / 255.0,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use draw_core::{PointerButton, Size};

    fn viewport() -> ViewportSize {
        ViewportSize::new(Size::new(560.0, 640.0))
    }

    fn click(view: &mut NewDocumentView, position: Vec2) {
        view.event(&InputEvent::PointerDown {
            position,
            button: PointerButton::Left,
        });
        view.event(&InputEvent::PointerUp {
            position,
            button: PointerButton::Left,
        });
        view.update();
    }

    fn preset_rects(view: &NewDocumentView) -> Vec<draw_core::Rect> {
        view.presets
            .iter()
            .filter_map(|id| draw_ui::control(&view.tree, *id).map(|control| control.rect))
            .collect()
    }

    fn background_rects(view: &NewDocumentView) -> Vec<draw_core::Rect> {
        view.backgrounds
            .iter()
            .filter_map(|id| draw_ui::control(&view.tree, *id).map(|control| control.rect))
            .collect()
    }

    #[test]
    fn a_wrapped_preset_row_grows_and_does_not_overlap_the_background_row() {
        let mut view = NewDocumentView::new(Theme::dark());
        // 窄窗口：预设必然换行。
        view.layout(ViewportSize::new(Size::new(260.0, 420.0)));

        let presets = preset_rects(&view);
        let first_row = presets.first().map(|rect| rect.top()).unwrap_or(0.0);
        let rows = presets
            .iter()
            .filter(|rect| (rect.top() - first_row).abs() > 0.5)
            .count();
        assert!(rows >= 1, "预设应在窄窗口里换行：{presets:?}");
        for (i, a) in presets.iter().enumerate() {
            for b in &presets[i + 1..] {
                assert!(a.intersection(*b).is_none(), "预设不应重叠：{a:?} vs {b:?}");
            }
        }

        let preset_bottom = presets
            .iter()
            .map(|rect| rect.bottom())
            .fold(0.0f32, f32::max);
        let background_top = background_rects(&view)
            .iter()
            .map(|rect| rect.top())
            .fold(f32::INFINITY, f32::min);
        assert!(
            preset_bottom <= background_top + 0.5,
            "换行后的预设压住了下一行：preset_bottom={preset_bottom} background_top={background_top}"
        );
    }

    #[test]
    fn the_default_spec_is_128_white() {
        assert_eq!(
            NewDocumentSpec::default(),
            NewDocumentSpec {
                width: 128,
                height: 128,
                background: Color::WHITE,
            }
        );
    }

    #[test]
    fn a_preset_then_create_returns_that_spec() {
        let mut view = NewDocumentView::new(Theme::dark());
        view.layout(viewport());
        let index = PRESETS.len() - 1;
        let (width, height) = PRESETS[index];
        let center = view.preset_center(index).expect("预设按钮");
        click(&mut view, center);
        assert_eq!(view.spec().width, width);
        assert_eq!(view.spec().height, height);

        let center = view.create_center().expect("创建按钮");
        click(&mut view, center);
        assert_eq!(
            view.take_result(),
            Some(NewDocumentResult::Create(NewDocumentSpec {
                width,
                height,
                background: Color::WHITE,
            }))
        );
        assert_eq!(view.take_result(), None, "结果只能取走一次");
    }

    #[test]
    fn choosing_a_background_then_create_returns_it() {
        let mut view = NewDocumentView::new(Theme::dark());
        view.layout(viewport());
        let center = view.background_center(0).expect("透明色块");
        click(&mut view, center);
        assert_eq!(view.spec().background, Color::TRANSPARENT);

        let center = view.create_center().expect("创建按钮");
        click(&mut view, center);
        assert_eq!(
            view.take_result(),
            Some(NewDocumentResult::Create(NewDocumentSpec {
                width: 128,
                height: 128,
                background: Color::TRANSPARENT,
            }))
        );
    }

    #[test]
    fn cancel_reports_cancel() {
        let mut view = NewDocumentView::new(Theme::dark());
        view.layout(viewport());
        let center = view.cancel_center().expect("取消按钮");
        click(&mut view, center);
        assert_eq!(view.take_result(), Some(NewDocumentResult::Cancel));
    }
}
