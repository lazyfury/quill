//! Deterministic text measurement, text options and line wrapping.
//!
//! The MVP has no font shaping in core, so widths come from a pluggable
//! [`TextMeasurer`]. The default [`ApproxTextMeasurer`] is a deterministic
//! per-character estimate; a host can inject real font metrics (for example a
//! fixed-width measurer matching the bitmap-font backend) via
//! [`Ui::set_text_measurer`](crate::Ui::set_text_measurer).
//!
//! Wrapping rules:
//!
//! - Explicit `\n` always breaks.
//! - East-Asian wide characters (CJK/kana/hangul) are individual break units.
//! - Latin-like runs form words that stay together; runs of spaces are
//!   collapsed to a single separating space between words.
//! - A unit wider than the available width is hard-broken per character.
//! - `max_lines` truncates to the last line; with `ellipsis` the last line gets
//!   an `…` that fits the available width.

use draw_core::Size;

/// Measures glyph advances and line height.
///
/// Keeping this a trait lets layout stay backend-neutral while the host supplies
/// real metrics. Implementations must be deterministic.
pub trait TextMeasurer {
    /// Advance width of a single character at `font_size`.
    fn advance(&self, ch: char, font_size: f32) -> f32;

    /// Height of one text line at `font_size`.
    fn line_height(&self, font_size: f32) -> f32;

    /// Distance from the top of a line to its baseline.
    ///
    /// Defaults to `0.8 * font_size`, which is close enough for the built-in
    /// measurers and can be overridden by hosts with real font metrics.
    fn ascent(&self, font_size: f32) -> f32 {
        font_size * 0.8
    }

    /// Width of `text` on a single line.
    fn measure_line(&self, text: &str, font_size: f32) -> f32 {
        self.measure_run(text, font_size)
    }

    /// Advance width of `text` as one run, applying shaping (kerning,
    /// ligatures) where the host supports it.
    ///
    /// Defaults to summing [`TextMeasurer::advance`]. A host with a real shaper
    /// overrides this so layout width matches the shaped width a backend
    /// renders (see the wgpu backend's `FontMetrics::measure_run`).
    fn measure_run(&self, text: &str, font_size: f32) -> f32 {
        text.chars().map(|ch| self.advance(ch, font_size)).sum()
    }
}

/// Default deterministic estimate.
///
/// Latin/general glyphs use `0.55 * font_size`, spaces `0.33`, and East-Asian
/// wide characters a full `font_size`. Line height is `1.25 * font_size`.
#[derive(Debug, Clone, Copy, Default)]
pub struct ApproxTextMeasurer;

impl TextMeasurer for ApproxTextMeasurer {
    fn advance(&self, ch: char, font_size: f32) -> f32 {
        match ch {
            '\t' => font_size * 2.0,
            ' ' => font_size * 0.33,
            _ if is_wide(ch) => font_size,
            _ => font_size * 0.55,
        }
    }

    fn line_height(&self, font_size: f32) -> f32 {
        font_size * 1.25
    }
}

/// A fixed-advance measurer, useful for monospace / bitmap-font backends.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FixedWidthTextMeasurer {
    /// Advance per glyph as a fraction of `font_size`.
    pub advance_ratio: f32,
    /// Line height as a fraction of `font_size`.
    pub line_height_ratio: f32,
}

impl Default for FixedWidthTextMeasurer {
    fn default() -> Self {
        Self {
            advance_ratio: 1.0,
            line_height_ratio: 1.3,
        }
    }
}

impl TextMeasurer for FixedWidthTextMeasurer {
    fn advance(&self, _ch: char, font_size: f32) -> f32 {
        font_size * self.advance_ratio
    }

    fn line_height(&self, font_size: f32) -> f32 {
        font_size * self.line_height_ratio
    }
}

/// Per-label wrapping / overflow behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextOptions {
    /// Soft-wrap to the available width.
    pub wrap: bool,
    /// Maximum number of lines (`None` = unlimited).
    pub max_lines: Option<usize>,
    /// Append `…` to the last line when `max_lines` clips it.
    pub ellipsis: bool,
}

impl Default for TextOptions {
    fn default() -> Self {
        Self {
            wrap: true,
            max_lines: None,
            ellipsis: false,
        }
    }
}

impl TextOptions {
    pub const fn new() -> Self {
        Self {
            wrap: true,
            max_lines: None,
            ellipsis: false,
        }
    }

    /// No soft wrapping (explicit `\n` still applies).
    pub const fn no_wrap() -> Self {
        Self {
            wrap: false,
            max_lines: None,
            ellipsis: false,
        }
    }

    pub const fn wrap(mut self, wrap: bool) -> Self {
        self.wrap = wrap;
        self
    }

    pub const fn max_lines(mut self, max_lines: usize) -> Self {
        self.max_lines = Some(max_lines);
        self
    }

    pub const fn ellipsis(mut self, ellipsis: bool) -> Self {
        self.ellipsis = ellipsis;
        self
    }
}

/// Whether `ch` is an East-Asian wide character (breakable on both sides).
pub fn is_wide(ch: char) -> bool {
    matches!(ch as u32,
        0x1100..=0x115F      // Hangul Jamo
        | 0x2E80..=0x303E    // CJK radicals, Kangxi, punctuation
        | 0x3041..=0x33FF    // Hiragana, Katakana, CJK symbols
        | 0x3400..=0x4DBF    // CJK Ext A
        | 0x4E00..=0x9FFF    // CJK Unified
        | 0xA000..=0xA4CF    // Yi
        | 0xAC00..=0xD7A3    // Hangul syllables
        | 0xF900..=0xFAFF    // CJK compatibility ideographs
        | 0xFE30..=0xFE4F    // CJK compatibility forms
        | 0xFF00..=0xFF60    // Fullwidth forms
        | 0xFFE0..=0xFFE6    // Fullwidth signs
        | 0x20000..=0x2FA1F  // CJK Ext B+
    )
}

/// Advance width of `ch` using the default [`ApproxTextMeasurer`].
pub fn char_advance(ch: char, font_size: f32) -> f32 {
    ApproxTextMeasurer.advance(ch, font_size)
}

/// Line height using the default [`ApproxTextMeasurer`].
pub fn line_height(font_size: f32) -> f32 {
    ApproxTextMeasurer.line_height(font_size)
}

/// Width of `text` on one line using the default measurer.
pub fn measure_line(text: &str, font_size: f32) -> f32 {
    ApproxTextMeasurer.measure_line(text, font_size)
}

/// Width of `text` on one line using `measurer`.
pub fn measure_line_with(measurer: &dyn TextMeasurer, text: &str, font_size: f32) -> f32 {
    measurer.measure_line(text, font_size)
}

/// Natural size of `text` with explicit newlines but no soft wrapping.
pub fn measure(text: &str, font_size: f32) -> Size {
    measure_with(&ApproxTextMeasurer, text, font_size)
}

/// Natural size of `text` with explicit newlines but no soft wrapping.
pub fn measure_with(measurer: &dyn TextMeasurer, text: &str, font_size: f32) -> Size {
    let mut width = 0.0f32;
    let mut lines = 0usize;
    for line in text.split('\n') {
        lines += 1;
        width = width.max(measurer.measure_line(line, font_size));
    }
    Size::new(width, lines.max(1) as f32 * measurer.line_height(font_size))
}

/// Width of the widest unbreakable unit in `text` (default measurer).
pub fn longest_unit_width(text: &str, font_size: f32) -> f32 {
    longest_unit_width_with(&ApproxTextMeasurer, text, font_size)
}

/// Width of the widest unbreakable unit in `text`.
///
/// This is the narrowest a label can become without hard-breaking a word or
/// wide character.
pub fn longest_unit_width_with(measurer: &dyn TextMeasurer, text: &str, font_size: f32) -> f32 {
    let mut max = 0.0f32;
    for line in text.split('\n') {
        for (token, _) in tokens(line) {
            max = max.max(measurer.measure_line(&token, font_size));
        }
    }
    max
}

/// Greedy soft wrapping using the default [`ApproxTextMeasurer`].
pub fn wrap_text(text: &str, font_size: f32, max_width: f32) -> Vec<String> {
    wrap_text_with(&ApproxTextMeasurer, text, font_size, max_width)
}

/// Greedy soft wrapping of `text` to `max_width` logical pixels.
///
/// Always returns at least one line. A non-positive `max_width` disables soft
/// wrapping (explicit `\n` still applies).
pub fn wrap_text_with(
    measurer: &dyn TextMeasurer,
    text: &str,
    font_size: f32,
    max_width: f32,
) -> Vec<String> {
    let mut out = Vec::new();
    for hard in text.split('\n') {
        wrap_hard_line(measurer, hard, font_size, max_width, &mut out);
    }
    if out.is_empty() {
        out.push(String::new());
    }
    out
}

/// Full text layout honoring [`TextOptions`].
///
/// Returns the lines to paint, already wrapped, clipped to `max_lines`, and
/// (optionally) ellipsized.
pub fn layout_text(
    measurer: &dyn TextMeasurer,
    text: &str,
    font_size: f32,
    max_width: f32,
    options: TextOptions,
) -> Vec<String> {
    let mut lines = if options.wrap {
        wrap_text_with(measurer, text, font_size, max_width)
    } else {
        text.split('\n').map(str::to_string).collect()
    };
    if lines.is_empty() {
        lines.push(String::new());
    }

    if let Some(max_lines) = options.max_lines {
        if max_lines == 0 {
            lines.truncate(1);
            lines[0] = String::new();
        } else if lines.len() > max_lines {
            lines.truncate(max_lines);
            if options.ellipsis {
                if let Some(last) = lines.last_mut() {
                    truncate_with_ellipsis(measurer, last, font_size, max_width);
                }
            }
        }
    }
    lines
}

fn truncate_with_ellipsis(
    measurer: &dyn TextMeasurer,
    line: &mut String,
    font_size: f32,
    max_width: f32,
) {
    const ELLIPSIS: char = '\u{2026}';
    if max_width <= 0.0 {
        line.push(ELLIPSIS);
        return;
    }
    let ellipsis_w = measurer.advance(ELLIPSIS, font_size);
    while !line.is_empty() && measurer.measure_line(line, font_size) + ellipsis_w > max_width {
        line.pop();
    }
    line.push(ELLIPSIS);
}

/// Sub-pixel tolerance so text whose measured width is exactly the available
/// width does not wrap from float rounding: the natural width and the running
/// width in [`wrap_hard_line`] sum the same advances in a different order.
const EPSILON: f32 = 1e-3;

fn wrap_hard_line(
    measurer: &dyn TextMeasurer,
    line: &str,
    font_size: f32,
    max_width: f32,
    out: &mut Vec<String>,
) {
    if max_width <= 0.0 {
        out.push(line.to_string());
        return;
    }
    let units = tokens(line);
    if units.is_empty() {
        out.push(String::new());
        return;
    }

    let space_w = measurer.advance(' ', font_size);
    let mut current = String::new();
    let mut current_w = 0.0f32;

    for (unit, space_before) in units {
        let unit_w = measurer.measure_line(&unit, font_size);
        let sep = if current.is_empty() || !space_before {
            0.0
        } else {
            space_w
        };

        if !current.is_empty() && current_w + sep + unit_w > max_width + EPSILON {
            out.push(std::mem::take(&mut current));
            current_w = 0.0;
        }

        // After a possible wrap, recompute the separator (leading one is dropped).
        let sep = if current.is_empty() || !space_before {
            0.0
        } else {
            space_w
        };

        if current.is_empty() && unit_w > max_width + EPSILON {
            hard_break(
                measurer,
                &unit,
                font_size,
                max_width,
                &mut current,
                &mut current_w,
                out,
            );
            continue;
        }

        if sep > 0.0 {
            current.push(' ');
            current_w += sep;
        }
        current.push_str(&unit);
        current_w += unit_w;
    }

    out.push(current);
}

fn hard_break(
    measurer: &dyn TextMeasurer,
    unit: &str,
    font_size: f32,
    max_width: f32,
    current: &mut String,
    current_w: &mut f32,
    out: &mut Vec<String>,
) {
    for ch in unit.chars() {
        let ch_w = measurer.advance(ch, font_size);
        if !current.is_empty() && *current_w + ch_w > max_width + EPSILON {
            out.push(std::mem::take(current));
            *current_w = 0.0;
        }
        current.push(ch);
        *current_w += ch_w;
    }
}

/// Splits a hard line into `(unit, space_before)` break units.
///
/// Spaces are treated as separators and collapsed; the exact count of
/// consecutive spaces is not preserved (acceptable for UI text in the MVP).
/// `space_before` is `true` when whitespace separated this unit from the
/// previous one, so wrapping never inserts a space between CJK characters.
fn tokens(line: &str) -> Vec<(String, bool)> {
    let mut tokens = Vec::new();
    let mut word = String::new();
    let mut word_space = false;
    let mut pending_space = false;
    for ch in line.chars() {
        if ch == ' ' || ch == '\t' {
            if !word.is_empty() {
                tokens.push((std::mem::take(&mut word), word_space));
                word_space = false;
            }
            pending_space = true;
        } else if is_wide(ch) {
            if !word.is_empty() {
                tokens.push((std::mem::take(&mut word), word_space));
                word_space = false;
            }
            tokens.push((ch.to_string(), pending_space));
            pending_space = false;
        } else {
            if word.is_empty() {
                word_space = pending_space;
                pending_space = false;
            }
            word.push(ch);
        }
    }
    if !word.is_empty() {
        tokens.push((word, word_space));
    }
    tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measure_matches_sum_of_advances() {
        let size = measure("abc", 10.0);
        assert!((size.width - measure_line("abc", 10.0)).abs() < 1e-4);
        assert!((size.height - line_height(10.0)).abs() < 1e-4);
    }

    #[test]
    fn explicit_newlines_increase_height() {
        let one = measure("a\nb", 20.0);
        assert!((one.height - line_height(20.0) * 2.0).abs() < 1e-4);
    }

    #[test]
    fn wide_chars_are_wider_than_ascii() {
        assert!(char_advance('中', 10.0) > char_advance('a', 10.0));
        assert!(is_wide('中'));
        assert!(!is_wide('a'));
    }

    #[test]
    fn wraps_on_spaces() {
        let lines = wrap_text("hello world", 10.0, 40.0);
        assert_eq!(lines, vec!["hello".to_string(), "world".to_string()]);
    }

    #[test]
    fn exact_fit_text_does_not_wrap() {
        // A label sized to its natural width must stay on one line even when
        // the running-width accumulation rounds the other way.
        let measurer = ApproxTextMeasurer;
        let width = measurer.measure_line("All Notes", 22.0);
        let lines = wrap_text_with(&measurer, "All Notes", 22.0, width);
        assert_eq!(lines, vec!["All Notes".to_string()]);
    }

    #[test]
    fn wraps_between_wide_chars_without_spaces() {
        let lines = wrap_text("你好世界", 10.0, 25.0);
        assert_eq!(lines, vec!["你好".to_string(), "世界".to_string()]);
    }

    #[test]
    fn hard_breaks_overlong_word() {
        let lines = wrap_text("abcdefghij", 10.0, 25.0);
        assert!(lines.len() > 1);
        assert!(lines
            .iter()
            .all(|line| measure_line(line, 10.0) <= 25.0 + 1e-4));
    }

    #[test]
    fn non_positive_width_disables_soft_wrap() {
        let lines = wrap_text("a b c", 10.0, 0.0);
        assert_eq!(lines, vec!["a b c".to_string()]);
    }

    #[test]
    fn longest_unit_is_a_single_word() {
        let w = longest_unit_width("hello world", 10.0);
        assert!((w - measure_line("hello", 10.0)).abs() < 1e-4);
    }

    #[test]
    fn fixed_width_measurer_uses_uniform_advance() {
        let m = FixedWidthTextMeasurer::default();
        assert_eq!(m.advance('a', 10.0), 10.0);
        assert_eq!(m.advance('中', 10.0), 10.0);
        assert_eq!(m.measure_line("abc", 10.0), 30.0);
    }

    #[test]
    fn max_lines_truncates_without_ellipsis() {
        let m = ApproxTextMeasurer;
        let opts = TextOptions::default().max_lines(1);
        let lines = layout_text(&m, "hello world hello", 10.0, 40.0, opts);
        assert_eq!(lines.len(), 1);
    }

    #[test]
    fn ellipsis_is_added_and_fits() {
        let m = ApproxTextMeasurer;
        let opts = TextOptions::default().max_lines(1).ellipsis(true);
        let lines = layout_text(&m, "hello world hello", 10.0, 40.0, opts);
        assert_eq!(lines.len(), 1);
        assert!(lines[0].ends_with('\u{2026}'));
        assert!(m.measure_line(&lines[0], 10.0) <= 40.0 + 1e-3);
    }

    #[test]
    fn no_wrap_keeps_explicit_newlines_only() {
        let m = ApproxTextMeasurer;
        let lines = layout_text(&m, "hello world\nagain", 10.0, 20.0, TextOptions::no_wrap());
        assert_eq!(lines, vec!["hello world".to_string(), "again".to_string()]);
    }

    #[test]
    fn wrapping_uses_the_shaped_run_advance() {
        /// Reports half the width for a whole run, emulating kerning/ligatures.
        struct Shaper;

        impl TextMeasurer for Shaper {
            fn advance(&self, _ch: char, font_size: f32) -> f32 {
                font_size * 0.5
            }

            fn line_height(&self, font_size: f32) -> f32 {
                font_size * 1.2
            }

            fn measure_run(&self, text: &str, font_size: f32) -> f32 {
                text.chars()
                    .map(|ch| self.advance(ch, font_size))
                    .sum::<f32>()
                    * 0.5
            }
        }

        // "ab" measures 10 unshaped (would hard-break) but 5 shaped, so it fits.
        let lines = wrap_text_with(&Shaper, "ab cd", 10.0, 8.0);
        assert_eq!(lines, vec!["ab".to_string(), "cd".to_string()]);
    }
}
