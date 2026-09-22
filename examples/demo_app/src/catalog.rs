//! The component gallery catalog: the sidebar groups and the preview cards they
//! contain.
//!
//! This is pure data. [`ITEMS`] is parallel to [`GROUPS`] (`ITEMS[g]` are the
//! cards of `GROUPS[g]`); the catalog drives both the sidebar and the preview
//! pane in [`crate::previews`], and its length is the router's view count.

/// A sidebar section of the gallery.
pub struct Group {
    pub name: &'static str,
    pub blurb: &'static str,
}

/// A preview card inside a group.
pub struct Item {
    pub name: &'static str,
    pub blurb: &'static str,
    /// A one-line Rust snippet shown under the card's live example.
    pub snippet: &'static str,
}

/// The sidebar groups, in display order.
pub const GROUPS: &[Group] = &[
    Group {
        name: "Layout",
        blurb: "Flex, grid, anchors, alignment and spacing.",
    },
    Group {
        name: "Text",
        blurb: "The type scale, weights, overflow and wrapping.",
    },
    Group {
        name: "Surfaces",
        blurb: "Cards, dividers and metadata badges.",
    },
    Group {
        name: "Content",
        blurb: "Code, terminal and empty-state surfaces.",
    },
    Group {
        name: "Controls",
        blurb: "Buttons, toggles and a draggable handle.",
    },
    Group {
        name: "Data",
        blurb: "A virtualized list and a view router.",
    },
    Group {
        name: "Overlays",
        blurb: "Menus, dialogs and transient toasts.",
    },
    Group {
        name: "Theme & Platform",
        blurb: "Tokens, density, input and cursor feedback.",
    },
];

/// The preview cards of each group (`ITEMS[g]` belongs to `GROUPS[g]`).
pub const ITEMS: &[&[Item]] = &[
    // Layout
    &[
        Item {
            name: "Flex · Row / Column",
            blurb: "Distribute children along one axis.",
            snippet: "Row::new().gap(8.0).child(a).child(b)",
        },
        Item {
            name: "Grow & Shrink",
            blurb: "Weight the leftover space, or pin a basis.",
            snippet: "Row::new().child(a.grow(2.0)).child(b.grow(1.0))",
        },
        Item {
            name: "Grid tracks",
            blurb: "Fixed, fractional and auto tracks.",
            snippet: "Grid::new(vec![Track::Px(72.0), Track::Fr(1.0)])",
        },
        Item {
            name: "Anchors & Offsets",
            blurb: "Pin a child to any parent edge.",
            snippet: ".anchors(Edges::new(1.0, 1.0, 1.0, 1.0))",
        },
        Item {
            name: "Alignment",
            blurb: "Start / center / end / stretch on the cross axis.",
            snippet: "Row::new().align(Align::Center)",
        },
        Item {
            name: "Padding & Gap",
            blurb: "Spacing comes from the theme scale.",
            snippet: "Column::new().padding(Edges::all(space::LG)).gap(space::MD)",
        },
    ],
    // Text
    &[
        Item {
            name: "Type scale",
            blurb: "display → caption, resolved from the theme.",
            snippet: "Text::heading(\"Heading\", theme)",
        },
        Item {
            name: "Font weight",
            blurb: "Numeric weights (100–900).",
            snippet: "Text::new(\"Bold\", theme).bold()",
        },
        Item {
            name: "Wrapping & ellipsis",
            blurb: "Wrap, clamp lines and ellipsize.",
            snippet: "Text::new(body).max_lines(2).ellipsis(true)",
        },
        Item {
            name: "Word break",
            blurb: "Word, BreakAll and KeepAll for CJK / long words.",
            snippet: "Text::new(cjk).word_break(WordBreak::KeepAll)",
        },
        Item {
            name: "Tone & colour",
            blurb: "Semantic tones and explicit colours.",
            snippet: "Text::small(\"Error\", theme).tone(Tone::Error)",
        },
    ],
    // Surfaces
    &[
        Item {
            name: "Card",
            blurb: "A themed column surface with a hairline border.",
            snippet: "Card::new(theme).gap(8.0).child(body)",
        },
        Item {
            name: "Divider",
            blurb: "1px horizontal and vertical rules.",
            snippet: "Divider::horizontal(theme)",
        },
        Item {
            name: "Badge",
            blurb: "Toned metadata tags, pill or solid.",
            snippet: "Badge::new(\"New\", theme).pill()",
        },
    ],
    // Content
    &[
        Item {
            name: "CodeBlock",
            blurb: "Code surface with filename and language.",
            snippet: "CodeBlock::new(src, theme).language(\"rust\")",
        },
        Item {
            name: "Terminal",
            blurb: "A command and its output in a window.",
            snippet: "Terminal::new(theme).command(\"cargo test\")",
        },
        Item {
            name: "EmptyState",
            blurb: "Icon placeholder, title and description.",
            snippet: "EmptyState::new(\"Nothing here\", theme)",
        },
    ],
    // Controls
    &[
        Item {
            name: "Button",
            blurb: "Primary / secondary / ghost / destructive.",
            snippet: "Button::primary(\"Save\", theme).on_click(save)",
        },
        Item {
            name: "Checkbox",
            blurb: "Labeled boolean with shared state.",
            snippet: "Checkbox::new(\"Enabled\", theme).checked(true)",
        },
        Item {
            name: "Switch",
            blurb: "Compact on/off toggle.",
            snippet: "Switch::new(theme).label(\"Sync\").on(true)",
        },
        Item {
            name: "ResizeHandle",
            blurb: "Drag to resize a target pane.",
            snippet: "ResizeHandle::vertical(theme).target(pane)",
        },
    ],
    // Data
    &[
        Item {
            name: "List",
            blurb: "Virtualized rows over a data source.",
            snippet: "List::new(theme, 28.0, row).count(count)",
        },
        Item {
            name: "ScrollView",
            blurb: "A clipped, offset viewport with a scrollbar.",
            snippet: "ScrollView::new(theme).child(long_column)",
        },
        Item {
            name: "Router",
            blurb: "Show one of several mounted views.",
            snippet: "Router::with_route(pane, route).add(view)",
        },
    ],
    // Overlays
    &[
        Item {
            name: "Menu",
            blurb: "An anchored drop-down of menu items.",
            snippet: "overlays.menu(anchor, |tree, node| { .. })",
        },
        Item {
            name: "Confirm",
            blurb: "A modal dialog with a scrim.",
            snippet: "overlays.confirm(\"Delete item?\", \"Undo is final\")",
        },
        Item {
            name: "Message",
            blurb: "A transient toast, auto-dismissed.",
            snippet: "overlays.message_tone(\"Saved\", Tone::Success)",
        },
    ],
    // Theme & Platform
    &[
        Item {
            name: "Palette",
            blurb: "Semantic colour tokens (light and dark).",
            snippet: "theme.palette().accent",
        },
        Item {
            name: "Surface levels",
            blurb: "Base / surface / raised / floating.",
            snippet: "theme.surface(SurfaceLevel::Raised)",
        },
        Item {
            name: "Semantic tones",
            blurb: "Tone → palette colour.",
            snippet: "Tone::Success.color(theme)",
        },
        Item {
            name: "Density",
            blurb: "Spacing and control metrics as tokens.",
            snippet: "theme.control_height(ControlSize::Regular)",
        },
        Item {
            name: "Radius & spacing",
            blurb: "The radius and spacing scales.",
            snippet: "theme.radius(Radius::MD)",
        },
        Item {
            name: "Input & cursor",
            blurb: "Hit-testing and hover cursor feedback.",
            snippet: "draw_ui::hovered_cursor(&tree)",
        },
    ],
];
