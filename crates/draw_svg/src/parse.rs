//! A small XML/SVG reader: enough of the SVG shape + style grammar for icon
//! packs, not a general XML parser. Unknown elements and attributes are ignored;
//! `<defs>`-style containers have their contents skipped.

use draw_core::{Color, Vec2};

use crate::path;
use crate::shapes;
use crate::{
    ColorSource, LineCap, LineJoin, Shape, StrokeStyle, Subpath, SvgDocument, SvgError, ViewBox,
};

/// Containers whose children must not be treated as drawable shapes.
const IGNORED: &[&str] = &[
    "defs",
    "symbol",
    "clippath",
    "mask",
    "pattern",
    "lineargradient",
    "radialgradient",
    "style",
    "title",
    "desc",
    "filter",
    "marker",
    "metadata",
    "script",
];

/// Parses an SVG document.
pub fn parse(source: &str) -> Result<SvgDocument, SvgError> {
    let mut index = 0usize;
    let mut shapes: Vec<Shape> = Vec::new();
    let mut view_box: Option<ViewBox> = None;
    let mut found_root = false;
    let mut current = StrokeStyle::default();

    struct Frame {
        restore: StrokeStyle,
        ignored: bool,
    }
    let mut stack: Vec<Frame> = Vec::new();

    while index < source.len() {
        let Some(offset) = source[index..].find('<') else {
            break;
        };
        index += offset;

        if source[index..].starts_with("<!--") {
            index = advance_past(source, index, "-->", 3);
            continue;
        }
        if source[index..].starts_with("<!") {
            index = advance_past(source, index, ">", 1);
            continue;
        }
        if source[index..].starts_with("<?") {
            index = advance_past(source, index, "?>", 2);
            continue;
        }
        if source[index..].starts_with("</") {
            let Some(end) = source[index..].find('>') else {
                break;
            };
            if let Some(frame) = stack.pop() {
                current = frame.restore;
            }
            index += end + 1;
            continue;
        }

        let Some(end) = find_tag_end(source, index) else {
            break;
        };
        let inner = &source[index + 1..index + end];
        let self_closing = inner.trim_end().ends_with('/');
        let inner = inner.trim_end().trim_end_matches('/');
        let (name, attrs) = split_tag(inner);
        let tag = local_name(name);

        let parent_ignored = stack.last().map(|frame| frame.ignored).unwrap_or(false);
        let effective = apply_style(current, &attrs);
        if !parent_ignored {
            match tag.as_str() {
                "svg" => {
                    found_root = true;
                    if let Some(value) = attr(&attrs, "viewBox") {
                        view_box = Some(parse_view_box(value)?);
                    }
                }
                "path" | "line" | "polyline" | "polygon" | "rect" | "circle" | "ellipse" => {
                    let subpaths = build_shape(&tag, &attrs)?;
                    if !subpaths.is_empty() {
                        shapes.push(Shape {
                            subpaths,
                            stroke: effective,
                        });
                    }
                }
                _ => {}
            }
        }

        if !self_closing {
            let ignored = parent_ignored || IGNORED.contains(&tag.as_str());
            stack.push(Frame {
                restore: current,
                ignored,
            });
            if !ignored {
                current = effective;
            }
        }
        index += end + 1;
    }

    if !found_root {
        return Err(SvgError::MissingRoot);
    }
    Ok(SvgDocument {
        view_box: view_box.unwrap_or_default(),
        shapes,
    })
}

fn build_shape(tag: &str, attrs: &[(&str, &str)]) -> Result<Vec<Subpath>, SvgError> {
    match tag {
        "path" => match attr(attrs, "d") {
            Some(data) => path::parse(data),
            None => Ok(Vec::new()),
        },
        "line" => Ok(vec![shapes::line(
            number(attrs, "x1")?,
            number(attrs, "y1")?,
            number(attrs, "x2")?,
            number(attrs, "y2")?,
        )]),
        "polyline" | "polygon" => {
            let points = match attr(attrs, "points") {
                Some(value) => parse_points(value)?,
                None => Vec::new(),
            };
            if points.len() < 2 {
                return Ok(Vec::new());
            }
            Ok(vec![shapes::poly(points, tag == "polygon")])
        }
        "rect" => {
            let rx = optional_number(attrs, "rx")?;
            let ry = optional_number(attrs, "ry")?.or(rx);
            Ok(vec![shapes::rect(
                number(attrs, "x")?,
                number(attrs, "y")?,
                number(attrs, "width")?,
                number(attrs, "height")?,
                rx.unwrap_or(0.0),
                ry.unwrap_or(0.0),
            )])
        }
        "circle" => Ok(vec![shapes::circle(
            number(attrs, "cx")?,
            number(attrs, "cy")?,
            number(attrs, "r")?,
        )]),
        "ellipse" => Ok(vec![shapes::ellipse(
            number(attrs, "cx")?,
            number(attrs, "cy")?,
            number(attrs, "rx")?,
            number(attrs, "ry")?,
        )]),
        _ => Ok(Vec::new()),
    }
}

// -- style -----------------------------------------------------------------

fn apply_style(base: StrokeStyle, attrs: &[(&str, &str)]) -> StrokeStyle {
    let mut style = base;
    if let Some(value) = attr(attrs, "stroke") {
        style.source = parse_color(value);
    }
    if let Some(value) = attr(attrs, "stroke-width") {
        if let Ok(width) = value.trim().parse::<f32>() {
            style.width = width.max(0.0);
        }
    }
    if let Some(value) = attr(attrs, "stroke-linecap") {
        style.cap = match value.trim() {
            "round" => LineCap::Round,
            "square" => LineCap::Square,
            _ => LineCap::Butt,
        };
    }
    if let Some(value) = attr(attrs, "stroke-linejoin") {
        style.join = match value.trim() {
            "round" => LineJoin::Round,
            "bevel" => LineJoin::Bevel,
            _ => LineJoin::Miter,
        };
    }
    // `fill` is intentionally not tracked yet: this crate strokes only.
    style
}

fn parse_color(value: &str) -> ColorSource {
    let value = value.trim();
    if value.eq_ignore_ascii_case("none") || value.eq_ignore_ascii_case("transparent") {
        return ColorSource::None;
    }
    if value.eq_ignore_ascii_case("currentcolor") {
        return ColorSource::CurrentColor;
    }
    if let Some(hex) = value.strip_prefix('#') {
        if let Some(color) = parse_hex(hex) {
            return ColorSource::Color(color);
        }
    }
    match value.to_ascii_lowercase().as_str() {
        "black" => ColorSource::Color(Color::BLACK),
        "white" => ColorSource::Color(Color::WHITE),
        "red" => ColorSource::Color(Color::RED),
        "green" => ColorSource::Color(Color::GREEN),
        "blue" => ColorSource::Color(Color::BLUE),
        "yellow" => ColorSource::Color(Color::YELLOW),
        // Unknown paint (url(#...) etc.) falls back to the caller's color.
        _ => ColorSource::CurrentColor,
    }
}

fn parse_hex(hex: &str) -> Option<Color> {
    let digits: Vec<u32> = hex
        .chars()
        .map(|character| character.to_digit(16))
        .collect::<Option<Vec<_>>>()?;
    let channel = |high: u32, low: u32| (high * 16 + low) as f32 / 255.0;
    match digits.len() {
        3 => Some(Color::new(
            digits[0] as f32 / 15.0,
            digits[1] as f32 / 15.0,
            digits[2] as f32 / 15.0,
            1.0,
        )),
        6 => Some(Color::new(
            channel(digits[0], digits[1]),
            channel(digits[2], digits[3]),
            channel(digits[4], digits[5]),
            1.0,
        )),
        8 => Some(Color::new(
            channel(digits[0], digits[1]),
            channel(digits[2], digits[3]),
            channel(digits[4], digits[5]),
            channel(digits[6], digits[7]),
        )),
        _ => None,
    }
}

// -- attribute / geometry helpers ------------------------------------------

fn attr<'a>(attrs: &[(&'a str, &'a str)], name: &str) -> Option<&'a str> {
    attrs
        .iter()
        .find(|(candidate, _)| *candidate == name)
        .map(|(_, value)| *value)
}

fn number(attrs: &[(&str, &str)], name: &str) -> Result<f32, SvgError> {
    Ok(optional_number(attrs, name)?.unwrap_or(0.0))
}

fn optional_number(attrs: &[(&str, &str)], name: &str) -> Result<Option<f32>, SvgError> {
    match attr(attrs, name) {
        None => Ok(None),
        Some(value) => value
            .trim()
            .parse::<f32>()
            .map(Some)
            .map_err(|_| SvgError::BadAttribute(format!("{name}=\"{value}\""))),
    }
}

fn parse_points(value: &str) -> Result<Vec<Vec2>, SvgError> {
    let numbers = split_numbers(value);
    if numbers.len() % 2 != 0 {
        return Err(SvgError::BadPath(format!(
            "`points` has an odd number of coordinates: {value:?}"
        )));
    }
    Ok(numbers
        .chunks_exact(2)
        .map(|pair| Vec2::new(pair[0], pair[1]))
        .collect())
}

fn parse_view_box(value: &str) -> Result<ViewBox, SvgError> {
    let numbers = split_numbers(value);
    if numbers.len() != 4 {
        return Err(SvgError::BadAttribute(format!("viewBox=\"{value}\"")));
    }
    Ok(ViewBox::new(
        numbers[0],
        numbers[1],
        numbers[2].abs(),
        numbers[3].abs(),
    ))
}

fn split_numbers(value: &str) -> Vec<f32> {
    value
        .split(|character: char| character == ',' || character.is_ascii_whitespace())
        .filter(|token| !token.is_empty())
        .filter_map(|token| token.parse::<f32>().ok())
        .collect()
}

fn local_name(name: &str) -> String {
    name.rsplit(':').next().unwrap_or(name).to_ascii_lowercase()
}

fn split_tag(inner: &str) -> (&str, Vec<(&str, &str)>) {
    let trimmed = inner.trim_start();
    let name_end = trimmed
        .find(|character: char| character.is_ascii_whitespace())
        .unwrap_or(trimmed.len());
    let name = &trimmed[..name_end];
    (name, split_attrs(&trimmed[name_end..]))
}

fn split_attrs(mut rest: &str) -> Vec<(&str, &str)> {
    let mut attrs = Vec::new();
    loop {
        rest = rest.trim_start();
        if rest.is_empty() {
            break;
        }
        let Some(equals) = rest.find('=') else {
            break;
        };
        let name = rest[..equals].trim();
        let after = rest[equals + 1..].trim_start();
        let (value, tail) = if let Some(stripped) = after.strip_prefix('"') {
            match stripped.find('"') {
                Some(end) => (&stripped[..end], &stripped[end + 1..]),
                None => break,
            }
        } else if let Some(stripped) = after.strip_prefix('\'') {
            match stripped.find('\'') {
                Some(end) => (&stripped[..end], &stripped[end + 1..]),
                None => break,
            }
        } else {
            break;
        };
        attrs.push((name, value));
        rest = tail;
    }
    attrs
}

/// Byte offset of the `>` that closes the tag starting at `start`, respecting
/// quoted attribute values.
fn find_tag_end(source: &str, start: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut index = start + 1;
    let mut quote: Option<u8> = None;
    while index < bytes.len() {
        let byte = bytes[index];
        match quote {
            Some(open) => {
                if byte == open {
                    quote = None;
                }
            }
            None => {
                if byte == b'"' || byte == b'\'' {
                    quote = Some(byte);
                } else if byte == b'>' {
                    return Some(index - start);
                }
            }
        }
        index += 1;
    }
    None
}

fn advance_past(source: &str, start: usize, needle: &str, needle_len: usize) -> usize {
    match source[start..].find(needle) {
        Some(offset) => start + offset + needle_len,
        None => source.len(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BRUSH: &str = r#"<!-- @license lucide-static v1.47.0 - ISC -->
<svg
  class="lucide lucide-brush"
  xmlns="http://www.w3.org/2000/svg"
  width="24"
  height="24"
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width="2"
  stroke-linecap="round"
  stroke-linejoin="round"
>
  <path d="m11 10 3 3" />
  <path d="M6.5 21A3.5 3.5 0 1 0 3 17.5a2.62 2.62 0 0 1-.708 1.792A1 1 0 0 0 3 21z" />
  <path d="M9.969 17.031 21.378 5.624a1 1 0 0 0-3.002-3.002L6.967 14.031" />
</svg>"#;

    #[test]
    fn a_lucide_icon_parses_into_three_stroked_shapes() {
        let document = SvgDocument::parse(BRUSH).unwrap();
        assert_eq!(document.view_box, ViewBox::ICON_24);
        assert_eq!(document.shapes.len(), 3);
        let stroke = document.shapes[0].stroke;
        assert_eq!(stroke.source, ColorSource::CurrentColor);
        assert_eq!(stroke.width, 2.0);
        assert_eq!(stroke.cap, LineCap::Round);
        assert_eq!(stroke.join, LineJoin::Round);
    }

    #[test]
    fn stroke_style_is_inherited_through_a_group() {
        let source = r##"<svg viewBox="0 0 10 10"><g stroke="#f00" stroke-width="3">
            <path d="M0 0 L1 1" /></g><circle cx="5" cy="5" r="2" /></svg>"##;
        let document = SvgDocument::parse(source).unwrap();
        assert_eq!(document.shapes.len(), 2);
        assert_eq!(
            document.shapes[0].stroke.source,
            ColorSource::Color(Color::RED)
        );
        assert_eq!(document.shapes[0].stroke.width, 3.0);
        // The circle is outside the group, so it does not inherit the stroke.
        assert_eq!(document.shapes[1].stroke.source, ColorSource::None);
    }

    #[test]
    fn defs_contents_are_skipped() {
        let source = r#"<svg viewBox="0 0 10 10" stroke="black">
            <defs><circle cx="0" cy="0" r="5" /></defs>
            <rect x="1" y="1" width="2" height="2" /></svg>"#;
        let document = SvgDocument::parse(source).unwrap();
        assert_eq!(document.shapes.len(), 1, "only the rect is drawable");
    }

    #[test]
    fn a_document_without_a_root_is_an_error() {
        assert_eq!(SvgDocument::parse("<circle/>"), Err(SvgError::MissingRoot));
    }

    #[test]
    fn hex_colors_parse_in_short_and_long_form() {
        assert_eq!(
            parse_color("#0f0"),
            ColorSource::Color(Color::new(0.0, 1.0, 0.0, 1.0))
        );
        assert_eq!(
            parse_color("#00ff00"),
            ColorSource::Color(Color::new(0.0, 1.0, 0.0, 1.0))
        );
        assert_eq!(parse_color("none"), ColorSource::None);
        assert_eq!(parse_color("currentColor"), ColorSource::CurrentColor);
    }

    #[test]
    fn a_polygon_closes_and_a_polyline_does_not() {
        let source = r#"<svg viewBox="0 0 10 10"><polyline points="0,0 1,1 2,0"/>
            <polygon points="0,0 1,1 2,0"/></svg>"#;
        let document = SvgDocument::parse(source).unwrap();
        assert!(!document.shapes[0].subpaths[0].closed);
        assert!(document.shapes[1].subpaths[0].closed);
    }
}
