//! Structural, presentational components.

use crate::base::{Component, Flex, Label, Spec};
use draw_core::{Color, Edges, Size, Vec2};
use draw_render::PaintContext;
use draw_theme::{radius, Space, SurfaceTone, TextSize, Theme, Tone};
use draw_ui::{Align, Justify, SurfaceStyle, TextOptions, Widget};

use crate::Text;

/// A structured container: thin border, subtle surface, restrained radius.
///
/// The card is a column flex container, so children flow vertically. Its
/// surface is attached when it builds.
///
/// ```ignore
/// tree.add_child(root, Card::new(theme).gap(12.0)
///     .child(Text::heading("Settings", theme))
///     .child(Checkbox::new("Verbose", theme)));
/// ```
pub struct Card {
    spec: Spec,
    theme: &'static dyn Theme,
    tone: SurfaceTone,
    fill: Option<Color>,
    hairline: bool,
    radius: f32,
    padding: Edges,
    gap: f32,
}

impl Card {
    /// A raised card with a hairline border.
    pub fn new(theme: &'static dyn Theme) -> Self {
        Self {
            spec: Spec::default(),
            theme,
            tone: SurfaceTone::Raised,
            fill: None,
            hairline: true,
            radius: radius::LG,
            padding: Edges::all(theme.spacing(Space::LG)),
            gap: theme.spacing(Space::MD),
        }
    }

    /// A borderless card.
    pub fn flat(theme: &'static dyn Theme) -> Self {
        Self::new(theme).bordered(false)
    }

    /// Chooses the surface level used for the fill.
    pub fn surface(mut self, tone: SurfaceTone) -> Self {
        self.tone = tone;
        self
    }

    /// Overrides the resolved surface fill.
    pub fn fill(mut self, color: Color) -> Self {
        self.fill = Some(color);
        self
    }

    pub fn bordered(mut self, bordered: bool) -> Self {
        self.hairline = bordered;
        self
    }

    pub fn radius(mut self, radius: f32) -> Self {
        self.radius = radius;
        self
    }

    pub fn padding(mut self, padding: Edges) -> Self {
        self.padding = padding;
        self
    }

    pub fn gap(mut self, gap: f32) -> Self {
        self.gap = gap;
        self
    }
}

impl Component for Card {
    fn spec(&mut self) -> &mut Spec {
        &mut self.spec
    }

    fn name(&self) -> &'static str {
        "Card"
    }

    fn widget(&self) -> Widget {
        Widget::Flex(
            draw_ui::FlexStyle::column()
                .gap(self.gap)
                .padding(self.padding),
        )
    }

    fn prepare(&mut self) {
        let fill = self.fill.unwrap_or_else(|| self.tone.color(self.theme));
        let border = self.hairline.then(|| self.theme.palette().border);
        let style = SurfaceStyle::new(fill)
            .radius(self.radius)
            .border_opt(border);
        self.spec.background = Some(Box::new(move |_| style));
    }
}

/// A 1px grouping divider.
pub struct Divider {
    spec: Spec,
    theme: &'static dyn Theme,
    vertical: bool,
    color: Option<Color>,
}

impl Divider {
    /// A horizontal rule (stretches across a column).
    pub fn horizontal(theme: &'static dyn Theme) -> Self {
        Self {
            spec: Spec::leaf(),
            theme,
            vertical: false,
            color: None,
        }
    }

    /// A vertical rule (stretches down a row).
    pub fn vertical(theme: &'static dyn Theme) -> Self {
        Self {
            spec: Spec::leaf(),
            theme,
            vertical: true,
            color: None,
        }
    }

    pub fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }
}

impl Component for Divider {
    fn spec(&mut self) -> &mut Spec {
        &mut self.spec
    }

    fn name(&self) -> &'static str {
        "Divider"
    }

    fn widget(&self) -> Widget {
        Widget::Flex(draw_ui::FlexStyle::default().padding(Edges::ZERO))
    }

    fn prepare(&mut self) {
        let color = self.color.unwrap_or(self.theme.palette().border_subtle);
        let vertical = self.vertical;
        // A fixed rule: never grow or shrink along the main axis.
        self.spec.data.layout.grow = 0.0;
        self.spec.data.layout.shrink = 0.0;
        self.spec.data.min_size = if vertical {
            Size::new(1.0, 0.0)
        } else {
            Size::new(0.0, 1.0)
        };
        // A real 1px line (not a filled rect), drawn along the node's center so
        // it lands on a device-pixel edge.
        self.spec.foreground = Some(Box::new(move |ctx, rect, _| {
            if vertical {
                ctx.draw_line(
                    Vec2::new(rect.center().x, rect.top()),
                    Vec2::new(rect.center().x, rect.bottom()),
                    1.0,
                    color,
                );
            } else {
                ctx.draw_line(
                    Vec2::new(rect.left(), rect.center().y),
                    Vec2::new(rect.right(), rect.center().y),
                    1.0,
                    color,
                );
            }
        }));
    }
}

/// A compact metadata tag.
pub struct Badge {
    spec: Spec,
    theme: &'static dyn Theme,
    text: String,
    tone: Tone,
    solid: bool,
    fill: Option<Color>,
    text_color: Option<Color>,
    radius: f32,
}

impl Badge {
    pub fn new(text: impl Into<String>, theme: &'static dyn Theme) -> Self {
        Self {
            spec: Spec::leaf(),
            theme,
            text: text.into(),
            tone: Tone::Muted,
            solid: false,
            fill: None,
            text_color: None,
            radius: radius::SM,
        }
    }

    /// A fully rounded (pill) badge.
    pub fn pill(text: impl Into<String>, theme: &'static dyn Theme) -> Self {
        Self::new(text, theme).radius(radius::FULL)
    }

    pub fn tone(mut self, tone: Tone) -> Self {
        self.tone = tone;
        self
    }

    /// Fills the badge with the tone color instead of tinting it.
    pub fn solid(mut self) -> Self {
        self.solid = true;
        self
    }

    pub fn fill(mut self, color: Color) -> Self {
        self.fill = Some(color);
        self
    }

    pub fn text_color(mut self, color: Color) -> Self {
        self.text_color = Some(color);
        self
    }

    pub fn radius(mut self, radius: f32) -> Self {
        self.radius = radius;
        self
    }
}

impl Component for Badge {
    fn spec(&mut self) -> &mut Spec {
        &mut self.spec
    }

    fn name(&self) -> &'static str {
        "Badge"
    }

    fn widget(&self) -> Widget {
        Widget::Flex(
            draw_ui::FlexStyle::row()
                .align(Align::Center)
                .justify(Justify::Center)
                .gap(0.0)
                .padding(Edges::symmetric(
                    self.theme.spacing(Space::SM),
                    self.theme.spacing(Space::XXS),
                )),
        )
    }

    fn prepare(&mut self) {
        let accent = self.fill.unwrap_or_else(|| self.tone.color(self.theme));
        let style = if self.solid {
            SurfaceStyle::new(accent).radius(self.radius)
        } else {
            SurfaceStyle::new(accent.with_alpha(0.12))
                .border(accent.with_alpha(0.30))
                .radius(self.radius)
        };
        self.spec.background = Some(Box::new(move |_| style));

        let text_color = self.text_color.unwrap_or(if self.solid {
            self.theme.palette().on_accent
        } else {
            accent
        });
        let text = self.text.clone();
        let theme = self.theme;
        self.spec
            .child(Text::caption(text, theme).color(text_color));
    }
}

/// A code block: monospace content on a dedicated surface.
pub struct CodeBlock {
    spec: Spec,
    theme: &'static dyn Theme,
    code: String,
    filename: Option<String>,
    language: Option<String>,
}

impl CodeBlock {
    pub fn new(code: impl Into<String>, theme: &'static dyn Theme) -> Self {
        Self {
            spec: Spec::default(),
            theme,
            code: code.into(),
            filename: None,
            language: None,
        }
    }

    pub fn filename(mut self, filename: impl Into<String>) -> Self {
        self.filename = Some(filename.into());
        self
    }

    pub fn language(mut self, language: impl Into<String>) -> Self {
        self.language = Some(language.into());
        self
    }
}

impl Component for CodeBlock {
    fn spec(&mut self) -> &mut Spec {
        &mut self.spec
    }

    fn name(&self) -> &'static str {
        "CodeBlock"
    }

    fn widget(&self) -> Widget {
        Widget::Flex(
            draw_ui::FlexStyle::column()
                .gap(self.theme.spacing(Space::SM))
                .padding(Edges::all(self.theme.spacing(Space::LG))),
        )
    }

    fn prepare(&mut self) {
        let theme = self.theme;
        let style = SurfaceStyle::new(theme.palette().code_surface)
            .border(theme.palette().border)
            .radius(radius::LG);
        self.spec.background = Some(Box::new(move |_| style));

        if self.filename.is_some() || self.language.is_some() {
            let mut header = Flex::row()
                .align(Align::Center)
                .gap(theme.spacing(Space::SM))
                .anchors(Edges::ZERO)
                .offsets(Edges::ZERO);
            if let Some(filename) = self.filename.clone() {
                header = header.child(Text::small(filename, theme).tone(Tone::Muted));
            }
            if let Some(language) = self.language.clone() {
                header = header.child(Text::caption(language, theme).tone(Tone::Subtle));
            }
            self.spec.child(header);
        }
        self.spec.child(
            Label::new(self.code.clone())
                .font_size(TextSize::Small.px())
                .color(theme.palette().foreground)
                .text_options(TextOptions::no_wrap())
                .anchors(Edges::ZERO)
                .offsets(Edges::ZERO),
        );
    }
}

/// A terminal window: header dots, a command and its output.
pub struct Terminal {
    spec: Spec,
    theme: &'static dyn Theme,
    command: Option<String>,
    output: Vec<String>,
}

impl Terminal {
    pub fn new(theme: &'static dyn Theme) -> Self {
        Self {
            spec: Spec::default(),
            theme,
            command: None,
            output: Vec::new(),
        }
    }

    pub fn command(mut self, command: impl Into<String>) -> Self {
        self.command = Some(command.into());
        self
    }

    /// Appends one line of output.
    pub fn output(mut self, line: impl Into<String>) -> Self {
        self.output.push(line.into());
        self
    }

    /// Appends several output lines.
    pub fn outputs<I, S>(mut self, lines: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.output.extend(lines.into_iter().map(Into::into));
        self
    }
}

impl Component for Terminal {
    fn spec(&mut self) -> &mut Spec {
        &mut self.spec
    }

    fn name(&self) -> &'static str {
        "Terminal"
    }

    fn widget(&self) -> Widget {
        Widget::Flex(
            draw_ui::FlexStyle::column()
                .gap(self.theme.spacing(Space::SM))
                .padding(Edges::all(self.theme.spacing(Space::LG))),
        )
    }

    fn prepare(&mut self) {
        let theme = self.theme;
        let style = SurfaceStyle::new(theme.palette().code_surface)
            .border(theme.palette().border)
            .radius(radius::LG);
        self.spec.background = Some(Box::new(move |_| style));

        let dots = [
            theme.palette().error,
            theme.palette().warning,
            theme.palette().success,
        ];
        self.spec.child(
            Flex::row()
                .gap(theme.spacing(Space::XS))
                .padding(Edges::ZERO)
                .anchors(Edges::ZERO)
                .offsets(Edges::ZERO)
                .min_size(0.0, 8.0)
                .foreground(move |ctx: &mut PaintContext, rect, _| {
                    let r = 3.5;
                    let step = r * 2.0 + theme.spacing(Space::XXS);
                    let y = rect.top() + r;
                    for (index, color) in dots.into_iter().enumerate() {
                        ctx.fill_circle(
                            Vec2::new(rect.left() + r + index as f32 * step, y),
                            r,
                            color,
                        );
                    }
                }),
        );
        if let Some(command) = self.command.clone() {
            self.spec.child(
                Label::new(format!("$ {command}"))
                    .font_size(TextSize::Small.px())
                    .color(theme.palette().foreground)
                    .text_options(TextOptions::no_wrap())
                    .anchors(Edges::ZERO)
                    .offsets(Edges::ZERO),
            );
        }
        let output = self.output.join("\n");
        if !output.is_empty() {
            self.spec.child(
                Label::new(output)
                    .font_size(TextSize::Small.px())
                    .color(theme.palette().muted)
                    .text_options(TextOptions::no_wrap())
                    .anchors(Edges::ZERO)
                    .offsets(Edges::ZERO),
            );
        }
    }
}

/// An informational empty state: small icon, title, description.
pub struct EmptyState {
    spec: Spec,
    theme: &'static dyn Theme,
    title: String,
    description: Option<String>,
}

impl EmptyState {
    pub fn new(title: impl Into<String>, theme: &'static dyn Theme) -> Self {
        Self {
            spec: Spec::default(),
            theme,
            title: title.into(),
            description: None,
        }
    }

    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }
}

impl Component for EmptyState {
    fn spec(&mut self) -> &mut Spec {
        &mut self.spec
    }

    fn name(&self) -> &'static str {
        "EmptyState"
    }

    fn widget(&self) -> Widget {
        Widget::Flex(
            draw_ui::FlexStyle::column()
                .align(Align::Center)
                .gap(self.theme.spacing(Space::MD))
                .padding(Edges::all(self.theme.spacing(Space::XXXL))),
        )
    }

    fn prepare(&mut self) {
        let theme = self.theme;
        self.spec.child(
            Flex::new()
                .padding(Edges::ZERO)
                .anchors(Edges::ZERO)
                .offsets(Edges::ZERO)
                .min_size(32.0, 32.0)
                .foreground(move |ctx, rect, _| {
                    draw_ui::surface(
                        ctx,
                        rect,
                        &SurfaceStyle::new(theme.palette().background)
                            .border(theme.palette().border)
                            .radius(radius::MD),
                    );
                }),
        );
        self.spec.child(Text::subheading(self.title.clone(), theme));
        if let Some(description) = self.description.clone() {
            self.spec
                .child(Text::small(description, theme).tone(Tone::Muted));
        }
    }
}

crate::impl_scene_child!(Card, Divider, Badge, CodeBlock, Terminal, EmptyState);
