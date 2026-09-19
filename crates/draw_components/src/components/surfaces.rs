//! Structural, presentational components.

use draw_core::{Color, Edges, NodeId, Size, Vec2};
use draw_render::PaintContext;
use draw_theme::{radius, space, TextSize};
use draw_ui::{Align, Flex, Justify, Label, TextOptions};

use crate::{Component, ControlRef, Text, Ui};
use draw_ui::{foreground_decor, surface_decor};
use draw_ui::{surface, SurfaceStyle};
use draw_ui::{SurfaceTone, Tone};

/// A structured container: thin border, subtle surface, restrained radius.
///
/// The card is a column flex container, so children flow vertically. Its
/// surface is attached as a `draw_ui::NodeDecor` and painted behind its content.
#[derive(Debug, Clone, Copy)]
pub struct Card {
    tone: SurfaceTone,
    fill: Option<Color>,
    hairline: bool,
    radius: f32,
    padding: Edges,
    gap: f32,
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
}

impl Component for Card {
    fn mount(self, ui: &mut Ui, parent: NodeId) -> ControlRef {
        let theme = ui.theme();
        let fill = self.fill.unwrap_or_else(|| self.tone.color(&theme));
        let border = if self.hairline {
            Some(theme.palette.border)
        } else {
            None
        };
        let card = ui.add(parent, Flex::column().gap(self.gap).padding(self.padding));
        let style = SurfaceStyle::new(fill)
            .radius(self.radius)
            .border_opt(border);
        ui.add_decor(card.id(), surface_decor(style));
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
    fn mount(self, ui: &mut Ui, parent: NodeId) -> ControlRef {
        let node = ui.add(parent, Flex::new().padding(Edges::ZERO));
        crate::detach(ui, node.id());
        let min = if self.vertical {
            Size::new(1.0, 0.0)
        } else {
            Size::new(0.0, 1.0)
        };
        ui.set_min_size(node.id(), min);
        let style = SurfaceStyle::new(ui.theme().palette.border_subtle);
        ui.add_decor(node.id(), surface_decor(style));
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
    fn mount(self, ui: &mut Ui, parent: NodeId) -> ControlRef {
        let theme = ui.theme();
        let font = TextSize::Caption.px();
        let accent = self.tone.color(&theme);
        let pad = Edges::symmetric(space::SM, space::XXS);

        let node = ui.add(
            parent,
            Flex::row()
                .align(Align::Center)
                .justify(Justify::Center)
                .gap(0.0)
                .padding(pad),
        );
        crate::detach(ui, node.id());

        let style = if self.solid {
            SurfaceStyle::new(accent).radius(self.radius)
        } else {
            SurfaceStyle::new(accent.with_alpha(0.12))
                .border(accent.with_alpha(0.30))
                .radius(self.radius)
        };
        ui.add_decor(node.id(), surface_decor(style));

        let text_color = if self.solid {
            theme.palette.on_accent
        } else {
            accent
        };
        ui.add(
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
    fn mount(self, ui: &mut Ui, parent: NodeId) -> ControlRef {
        let theme = ui.theme();
        let block = ui.add(
            parent,
            Flex::column().gap(space::SM).padding(Edges::all(space::LG)),
        );
        let style = SurfaceStyle::new(theme.palette.code_surface)
            .border(theme.palette.border)
            .radius(radius::LG);
        ui.add_decor(block.id(), surface_decor(style));

        if self.filename.is_some() || self.language.is_some() {
            let header = ui.add(block.id(), Flex::row().align(Align::Center).gap(space::SM));
            if let Some(filename) = self.filename {
                ui.add(header.id(), Text::small(filename).tone(Tone::Muted));
            }
            if let Some(language) = self.language {
                ui.add(header.id(), Text::caption(language).tone(Tone::Subtle));
            }
        }

        ui.add(
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
    fn mount(self, ui: &mut Ui, parent: NodeId) -> ControlRef {
        let theme = ui.theme();
        let terminal = ui.add(
            parent,
            Flex::column().gap(space::SM).padding(Edges::all(space::LG)),
        );
        let style = SurfaceStyle::new(theme.palette.code_surface)
            .border(theme.palette.border)
            .radius(radius::LG);
        ui.add_decor(terminal.id(), surface_decor(style));

        let header = ui.add(terminal.id(), Flex::row().gap(space::XS));
        ui.set_min_size(header.id(), Size::new(0.0, 8.0));
        ui.add_decor(
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
            ui.add(
                terminal.id(),
                Label::new(format!("$ {command}"))
                    .font_size(TextSize::Small.px())
                    .color(theme.palette.foreground)
                    .text_options(TextOptions::no_wrap()),
            );
        }
        if !self.output.is_empty() {
            ui.add(
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
    fn mount(self, ui: &mut Ui, parent: NodeId) -> ControlRef {
        let theme = ui.theme();
        let container = ui.add(
            parent,
            Flex::column()
                .align(Align::Center)
                .gap(space::MD)
                .padding(Edges::all(space::XXXL)),
        );

        let icon = ui.add(container.id(), Flex::new().padding(Edges::ZERO));
        ui.set_min_size(icon.id(), Size::new(32.0, 32.0));
        ui.add_decor(
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

        ui.add(container.id(), Text::subheading(self.title));
        if let Some(description) = self.description {
            ui.add(container.id(), Text::small(description).tone(Tone::Muted));
        }
        container
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use draw_core::{Size as CoreSize, Viewport};
    use draw_render::DrawCommand;
    use draw_theme::Theme;

    fn layout(ui: &mut Ui, width: f32, height: f32) {
        ui.layout(Viewport::new(CoreSize::new(width, height)));
    }

    #[test]
    fn card_registers_a_surface_and_flows_children() {
        let mut ui = Ui::new();
        ui.set_theme(Theme::dark());
        let root = ui.root();
        let card = ui.add(root, Card::new());
        ui.add(card.id(), Text::heading("Title"));
        layout(&mut ui, 400.0, 300.0);

        let mut ctx = PaintContext::new();
        ui.paint(&mut ctx);
        let list = ctx.into_draw_list();
        assert!(list
            .commands()
            .iter()
            .any(|c| matches!(c, DrawCommand::FillRoundedRect { .. })));
    }

    #[test]
    fn divider_has_a_one_pixel_min_extent() {
        let mut ui = Ui::new();
        ui.set_theme(Theme::light());
        let root = ui.root();
        let divider = ui.add(root, Divider::horizontal());
        layout(&mut ui, 300.0, 100.0);
        let rect = ui.control(divider.id()).unwrap().rect;
        assert!((rect.size.height - 1.0).abs() < 1e-3);
    }

    #[test]
    fn badge_sizes_to_its_text() {
        let mut ui = Ui::new();
        ui.set_theme(Theme::light());
        let root = ui.root();
        let badge = ui.add(root, Badge::new("Stable"));
        layout(&mut ui, 300.0, 100.0);
        let rect = ui.control(badge.id()).unwrap().rect;
        assert!(rect.size.width > 0.0);
        assert!(rect.size.height > 0.0);
    }

    #[test]
    fn code_block_draws_surface_and_text() {
        let mut ui = Ui::new();
        ui.set_theme(Theme::dark());
        let root = ui.root();
        let block = ui.add(
            root,
            CodeBlock::new("let x = 1;")
                .filename("main.rs")
                .language("rust"),
        );
        layout(&mut ui, 500.0, 300.0);
        let mut ctx = PaintContext::new();
        ui.paint(&mut ctx);
        let list = ctx.into_draw_list();
        let texts: Vec<&str> = list
            .commands()
            .iter()
            .filter_map(|c| match c {
                DrawCommand::DrawText { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert!(texts.iter().any(|t| t.contains("let x = 1;")));
        let _ = block;
    }

    #[test]
    fn empty_state_has_a_title() {
        let mut ui = Ui::new();
        ui.set_theme(Theme::light());
        let root = ui.root();
        let empty = ui.add(
            root,
            EmptyState::new("No results").description("Try another query."),
        );
        layout(&mut ui, 400.0, 300.0);
        assert!(ui.control(empty.id()).is_some());
    }
}
