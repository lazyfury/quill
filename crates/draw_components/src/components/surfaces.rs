//! Structural, presentational components.

use draw_app::{child, BuildContext, Child, Flex, Label, View};
use draw_core::{Color, Edges, NodeId, Size, Vec2};
use draw_render::PaintContext;
use draw_scene::SceneTree;
use draw_theme::{radius, space, TextSize};
use draw_ui::{Align, Justify, TextOptions};

use crate::{Component, ControlRef, Text};
use draw_theme::{SurfaceTone, Tone};
use draw_ui::{foreground_decor, surface_decor};
use draw_ui::{surface, SurfaceStyle};

/// A structured container: thin border, subtle surface, restrained radius.
///
/// The card is a column flex container, so children flow vertically. Its
/// surface is attached as a `draw_ui::NodeDecor` and painted behind its content.
///
/// ```ignore
/// ui.mount(root, Card::new().gap(12.0)
///     .child(Text::heading("Settings"))
///     .child(Checkbox::new("Verbose")));
/// ```
pub struct Card {
    tone: SurfaceTone,
    fill: Option<Color>,
    hairline: bool,
    radius: f32,
    padding: Edges,
    gap: f32,
    children: Vec<Child>,
}

impl std::fmt::Debug for Card {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Card")
            .field("tone", &self.tone)
            .field("children", &self.children.len())
            .finish()
    }
}

impl Default for Card {
    fn default() -> Self {
        Self::new()
    }
}

impl Card {
    /// A raised card with a hairline border.
    pub fn new() -> Self {
        Self {
            tone: SurfaceTone::Raised,
            fill: None,
            hairline: true,
            radius: radius::LG,
            padding: Edges::all(space::LG),
            gap: space::MD,
            children: Vec::new(),
        }
    }

    /// A borderless card.
    pub fn flat() -> Self {
        Self::new().bordered(false)
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

    /// Adds one child view.
    pub fn child<V: View + 'static>(mut self, view: V) -> Self {
        self.children.push(child(view));
        self
    }

    /// Adds several child views.
    pub fn children<I, V>(mut self, views: I) -> Self
    where
        I: IntoIterator<Item = V>,
        V: View + 'static,
    {
        self.children.extend(views.into_iter().map(child));
        self
    }
}

impl Component for Card {
    fn mount(self, tree: &mut SceneTree, parent: NodeId) -> ControlRef {
        let theme = draw_ui::theme(tree);
        let fill = self.fill.unwrap_or_else(|| self.tone.color(&theme));
        let border = if self.hairline {
            Some(theme.palette.border)
        } else {
            None
        };
        let card = draw_app::add(
            tree,
            parent,
            Flex::column().gap(self.gap).padding(self.padding),
        );
        let style = SurfaceStyle::new(fill)
            .radius(self.radius)
            .border_opt(border);
        draw_ui::add_decor(tree, card.id(), surface_decor(style));
        BuildContext::new(tree, card.id()).children(self.children);
        card
    }
}

/// A 1px grouping divider.
#[derive(Debug, Clone, Copy)]
pub struct Divider {
    vertical: bool,
}

impl Divider {
    /// A horizontal rule (stretches across a column).
    pub fn horizontal() -> Self {
        Self { vertical: false }
    }

    /// A vertical rule (stretches down a row).
    pub fn vertical() -> Self {
        Self { vertical: true }
    }
}

impl Component for Divider {
    fn mount(self, tree: &mut SceneTree, parent: NodeId) -> ControlRef {
        let node = draw_app::add(tree, parent, Flex::new().padding(Edges::ZERO));
        crate::detach(tree, node.id());
        let min = if self.vertical {
            Size::new(1.0, 0.0)
        } else {
            Size::new(0.0, 1.0)
        };
        draw_app::update_control(tree, node.id(), |d| d.min_size = min);
        let style = SurfaceStyle::new(draw_ui::theme(tree).palette.border_subtle);
        draw_ui::add_decor(tree, node.id(), surface_decor(style));
        node
    }
}

/// A compact metadata tag.
#[derive(Debug, Clone)]
pub struct Badge {
    text: String,
    tone: Tone,
    solid: bool,
    radius: f32,
}

impl Badge {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            tone: Tone::Muted,
            solid: false,
            radius: radius::SM,
        }
    }

    /// A fully rounded (pill) badge.
    pub fn pill(text: impl Into<String>) -> Self {
        Self::new(text).radius(radius::FULL)
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

    pub fn radius(mut self, radius: f32) -> Self {
        self.radius = radius;
        self
    }
}

impl Component for Badge {
    fn mount(self, tree: &mut SceneTree, parent: NodeId) -> ControlRef {
        let theme = draw_ui::theme(tree);
        let font = TextSize::Caption.px();
        let accent = self.tone.color(&theme);
        let pad = Edges::symmetric(space::SM, space::XXS);

        let node = draw_app::add(
            tree,
            parent,
            Flex::row()
                .align(Align::Center)
                .justify(Justify::Center)
                .gap(0.0)
                .padding(pad),
        );
        crate::detach(tree, node.id());

        let style = if self.solid {
            SurfaceStyle::new(accent).radius(self.radius)
        } else {
            SurfaceStyle::new(accent.with_alpha(0.12))
                .border(accent.with_alpha(0.30))
                .radius(self.radius)
        };
        draw_ui::add_decor(tree, node.id(), surface_decor(style));

        let text_color = if self.solid {
            theme.palette.on_accent
        } else {
            accent
        };
        draw_app::add(
            tree,
            node.id(),
            Label::new(&self.text)
                .font_size(font)
                .color(text_color)
                .text_options(TextOptions::no_wrap()),
        );
        node
    }
}

/// A code block: monospace content on a dedicated surface.
#[derive(Debug, Clone)]
pub struct CodeBlock {
    code: String,
    filename: Option<String>,
    language: Option<String>,
}

impl CodeBlock {
    pub fn new(code: impl Into<String>) -> Self {
        Self {
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
    fn mount(self, tree: &mut SceneTree, parent: NodeId) -> ControlRef {
        let theme = draw_ui::theme(tree);
        let block = draw_app::add(
            tree,
            parent,
            Flex::column().gap(space::SM).padding(Edges::all(space::LG)),
        );
        let style = SurfaceStyle::new(theme.palette.code_surface)
            .border(theme.palette.border)
            .radius(radius::LG);
        draw_ui::add_decor(tree, block.id(), surface_decor(style));

        if self.filename.is_some() || self.language.is_some() {
            let header = draw_app::add(
                tree,
                block.id(),
                Flex::row().align(Align::Center).gap(space::SM),
            );
            if let Some(filename) = self.filename {
                draw_app::add(tree, header.id(), Text::small(filename).tone(Tone::Muted));
            }
            if let Some(language) = self.language {
                draw_app::add(
                    tree,
                    header.id(),
                    Text::caption(language).tone(Tone::Subtle),
                );
            }
        }

        draw_app::add(
            tree,
            block.id(),
            Label::new(self.code)
                .font_size(TextSize::Small.px())
                .color(theme.palette.foreground)
                .text_options(TextOptions::no_wrap()),
        );
        block
    }
}

/// A terminal window: header dots, a command and its output.
#[derive(Debug, Clone, Default)]
pub struct Terminal {
    command: Option<String>,
    output: Vec<String>,
}

impl Terminal {
    pub fn new() -> Self {
        Self::default()
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
    fn mount(self, tree: &mut SceneTree, parent: NodeId) -> ControlRef {
        let theme = draw_ui::theme(tree);
        let terminal = draw_app::add(
            tree,
            parent,
            Flex::column().gap(space::SM).padding(Edges::all(space::LG)),
        );
        let style = SurfaceStyle::new(theme.palette.code_surface)
            .border(theme.palette.border)
            .radius(radius::LG);
        draw_ui::add_decor(tree, terminal.id(), surface_decor(style));

        let header = draw_app::add(tree, terminal.id(), Flex::row().gap(space::XS));
        draw_app::update_control(tree, header.id(), |d| d.min_size = Size::new(0.0, 8.0));
        draw_ui::add_decor(
            tree,
            header.id(),
            foreground_decor(theme, |ctx: &mut PaintContext, rect, theme, _| {
                let r = 3.5;
                let step = r * 2.0 + space::XXS;
                let y = rect.top() + r;
                for (index, color) in [
                    theme.palette.error,
                    theme.palette.warning,
                    theme.palette.success,
                ]
                .into_iter()
                .enumerate()
                {
                    ctx.fill_circle(
                        Vec2::new(rect.left() + r + index as f32 * step, y),
                        r,
                        color,
                    );
                }
            }),
        );

        if let Some(command) = self.command {
            draw_app::add(
                tree,
                terminal.id(),
                Label::new(format!("$ {command}"))
                    .font_size(TextSize::Small.px())
                    .color(theme.palette.foreground)
                    .text_options(TextOptions::no_wrap()),
            );
        }
        if !self.output.is_empty() {
            draw_app::add(
                tree,
                terminal.id(),
                Label::new(self.output.join("\n"))
                    .font_size(TextSize::Small.px())
                    .color(theme.palette.muted)
                    .text_options(TextOptions::no_wrap()),
            );
        }
        terminal
    }
}

/// An informational empty state: small icon, title, description.
#[derive(Debug, Clone)]
pub struct EmptyState {
    title: String,
    description: Option<String>,
}

impl EmptyState {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
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
    fn mount(self, tree: &mut SceneTree, parent: NodeId) -> ControlRef {
        let theme = draw_ui::theme(tree);
        let container = draw_app::add(
            tree,
            parent,
            Flex::column()
                .align(Align::Center)
                .gap(space::MD)
                .padding(Edges::all(space::XXXL)),
        );

        let icon = draw_app::add(tree, container.id(), Flex::new().padding(Edges::ZERO));
        draw_app::update_control(tree, icon.id(), |d| d.min_size = Size::new(32.0, 32.0));
        draw_ui::add_decor(
            tree,
            icon.id(),
            foreground_decor(theme, |ctx, rect, theme, _| {
                surface(
                    ctx,
                    rect,
                    &SurfaceStyle::new(theme.palette.background)
                        .border(theme.palette.border)
                        .radius(radius::MD),
                );
            }),
        );

        draw_app::add(tree, container.id(), Text::subheading(self.title));
        if let Some(description) = self.description {
            draw_app::add(
                tree,
                container.id(),
                Text::small(description).tone(Tone::Muted),
            );
        }
        container
    }
}
