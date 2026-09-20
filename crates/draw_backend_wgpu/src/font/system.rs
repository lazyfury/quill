//! System font loading and shaping (`ab_glyph` + `rustybuzz`) with a
//! lazily-populated glyph atlas.
//!
//! A font is located from `QUILL_FONT` (explicit path) or a short per-OS
//! candidate list. Glyphs are rasterized on demand at `font_size` and packed
//! into a shelf atlas; the caller uploads [`SystemFont::take_dirty_atlas`]
//! after processing a frame's commands.
//!
//! Text is shaped per bidi run with `rustybuzz`, so kerning, ligatures and
//! contextual forms are applied and the returned glyphs are in visual order.
//! The same shaping feeds both rasterization and advance measurement, keeping
//! layout and rendering in agreement.

use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::Range;
use std::path::PathBuf;
use std::rc::Rc;

use ab_glyph::{point, Font, FontRef, Glyph, GlyphId, PxScale, ScaleFont};
use rustybuzz::{Direction, Face, UnicodeBuffer};
use unicode_bidi::BidiInfo;

use super::GlyphSlot;

/// Dynamic atlas width in texels.
pub const ATLAS_WIDTH: u32 = 1024;
/// Dynamic atlas height in texels.
pub const ATLAS_HEIGHT: u32 = 1024;
/// Gap between packed glyphs, to avoid bilinear bleed.
const PADDING: u32 = 1;

/// A shaped glyph in font units (before scaling by `font_size / upem`).
#[derive(Clone, Copy)]
struct ShapedGlyph {
    glyph_id: u16,
    x_advance: f32,
    x_offset: f32,
    y_offset: f32,
}

/// Shaping cache capacity (entries). Scrolling a hex dump produces a stream of
/// one-shot row texts; past the cap the cache clears and starts over.
const SHAPE_CACHE_CAP: usize = 4096;
/// Per-character advance cache capacity (entries).
const ADVANCE_CACHE_CAP: usize = 8192;

/// A font loaded from disk with an on-demand glyph atlas.
pub struct SystemFont {
    name: String,
    // `ab_glyph::FontRef` and `rustybuzz::Face` both borrow the leaked bytes.
    font: FontRef<'static>,
    face: Face<'static>,
    atlas: RefCell<Atlas>,
    cache: RefCell<HashMap<(u32, u32), GlyphSlot>>,
    /// Shaped runs keyed by the text itself: shaping output is in font units,
    /// so it does not depend on the pixel size and one entry serves all sizes.
    shape_cache: RefCell<HashMap<Box<str>, Rc<[ShapedGlyph]>>>,
    /// Per-character advances keyed by `(char, px bits)` — layout measures
    /// every character of every candidate wrap line on resize frames.
    advance_cache: RefCell<HashMap<(u32, u32), f32>>,
}

impl SystemFont {
    /// Searches `QUILL_FONT` and per-OS candidates for a loadable font.
    pub fn load() -> Option<Self> {
        for path in candidate_paths() {
            let Ok(bytes) = std::fs::read(&path) else {
                continue;
            };
            let data: &'static [u8] = Box::leak(bytes.into_boxed_slice());
            let Ok(font) = FontRef::try_from_slice(data) else {
                continue;
            };
            let Some(face) = Face::from_slice(data, 0) else {
                continue;
            };
            return Some(Self::new(font, face, path.display().to_string()));
        }
        None
    }

    pub fn new(font: FontRef<'static>, face: Face<'static>, name: String) -> Self {
        Self {
            name,
            font,
            face,
            atlas: RefCell::new(Atlas::new()),
            cache: RefCell::new(HashMap::new()),
            shape_cache: RefCell::new(HashMap::new()),
            advance_cache: RefCell::new(HashMap::new()),
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// Advance in device pixels (callers convert to logical).
    pub fn advance_px(&self, ch: char, px: f32) -> f32 {
        let key = (ch as u32, px.to_bits());
        let cached = self.advance_cache.borrow().get(&key).copied();
        if let Some(advance) = cached {
            return advance;
        }
        let scaled = self.font.as_scaled(px_scale(px));
        let advance = scaled.h_advance(self.font.glyph_id(ch));
        let mut cache = self.advance_cache.borrow_mut();
        if cache.len() >= ADVANCE_CACHE_CAP {
            cache.clear();
        }
        cache.insert(key, advance);
        advance
    }

    /// Shaped advance of `text` in device pixels.
    pub fn advance_run_px(&self, text: &str, px: f32) -> f32 {
        let scale = self.unit_scale(px);
        self.shape(text)
            .iter()
            .map(|glyph| glyph.x_advance * scale)
            .sum()
    }

    /// Natural line height (ascent + descent + line gap) in device pixels.
    pub fn line_height_px(&self, px: f32) -> f32 {
        let scaled = self.font.as_scaled(px_scale(px));
        scaled.height() + scaled.line_gap()
    }

    pub fn ascent_px(&self, px: f32) -> f32 {
        let scaled = self.font.as_scaled(px_scale(px));
        scaled.ascent()
    }

    /// Atlas slot for `ch` at `px` device pixels, rasterizing it on first use.
    pub fn glyph_px(&self, ch: char, px: f32) -> GlyphSlot {
        self.glyph_slot(self.font.glyph_id(ch).0, px)
    }

    /// Shapes `text` at `px` device pixels and returns one slot per glyph in
    /// visual order. Each glyph's `advance`/`offset` come from shaping, so the
    /// caller only walks the pen left-to-right.
    pub fn shape_px(&self, text: &str, px: f32) -> Vec<GlyphSlot> {
        let scale = self.unit_scale(px);
        self.shape(text)
            .iter()
            .map(|glyph| {
                let mut slot = self.glyph_slot(glyph.glyph_id, px);
                slot.advance = glyph.x_advance * scale;
                slot.offset[0] += glyph.x_offset * scale;
                slot.offset[1] += glyph.y_offset * scale;
                slot
            })
            .collect()
    }

    /// Returns the full atlas pixels if new glyphs were rasterized since the
    /// last call.
    pub fn take_dirty_atlas(&self) -> Option<Vec<u8>> {
        let mut atlas = self.atlas.borrow_mut();
        if !atlas.dirty {
            return None;
        }
        atlas.dirty = false;
        Some(atlas.pixels.clone())
    }

    /// Device-pixels-per-font-unit factor, matching `ab_glyph`'s scaling of
    /// `h_advance_unscaled` (so shaped and unshaped advances agree).
    fn unit_scale(&self, px: f32) -> f32 {
        self.font.as_scaled(px_scale(px)).h_scale_factor()
    }

    /// Shapes every bidi run of `text`, in visual order, in font units.
    ///
    /// Results are memoized by the text itself (font units are size-independent
    /// and this string is exactly what layout and paint ask for every frame);
    /// a drag/resize frame would otherwise re-run full HarfBuzz shaping for
    /// every text on screen, which is the difference between a smooth drag and
    /// a stuttering one in debug builds.
    fn shape(&self, text: &str) -> Rc<[ShapedGlyph]> {
        let cached = self.shape_cache.borrow().get(text).cloned();
        if let Some(cached) = cached {
            return cached;
        }
        let shaped: Rc<[ShapedGlyph]> = self.shape_uncached(text).into();
        let mut cache = self.shape_cache.borrow_mut();
        if cache.len() >= SHAPE_CACHE_CAP {
            cache.clear();
        }
        cache.insert(text.into(), shaped.clone());
        shaped
    }

    /// Shapes every bidi run of `text`, in visual order, in font units.
    fn shape_uncached(&self, text: &str) -> Vec<ShapedGlyph> {
        let mut out = Vec::new();
        for (range, rtl) in bidi_runs(text) {
            let run = &text[range];
            if run.is_empty() {
                continue;
            }
            let mut buffer = UnicodeBuffer::new();
            buffer.push_str(run);
            buffer.set_direction(if rtl {
                Direction::RightToLeft
            } else {
                Direction::LeftToRight
            });
            buffer.set_script(run_script(run));

            let glyphs = rustybuzz::shape(&self.face, &[], buffer);
            let infos = glyphs.glyph_infos();
            let positions = glyphs.glyph_positions();
            out.extend(infos.iter().zip(positions).map(|(info, pos)| ShapedGlyph {
                glyph_id: info.glyph_id as u16,
                x_advance: pos.x_advance as f32,
                x_offset: pos.x_offset as f32,
                y_offset: pos.y_offset as f32,
            }));
        }
        out
    }

    /// Atlas slot for a glyph id at `px` device pixels, rasterizing on first use.
    fn glyph_slot(&self, glyph_id: u16, px: f32) -> GlyphSlot {
        let px = px.round().max(1.0);
        let key = (glyph_id as u32, px.to_bits());
        if let Some(slot) = self.cache.borrow().get(&key) {
            return *slot;
        }
        let slot = self.rasterize(glyph_id, px);
        self.cache.borrow_mut().insert(key, slot);
        slot
    }

    fn rasterize(&self, glyph_id: u16, px: f32) -> GlyphSlot {
        let scaled = self.font.as_scaled(px_scale(px));
        let id = GlyphId(glyph_id);
        let advance = scaled.h_advance(id);
        let glyph = Glyph {
            id,
            scale: px_scale(px),
            position: point(0.0, 0.0),
        };

        let Some(outline) = scaled.outline_glyph(glyph) else {
            // Whitespace / control: advance only.
            return GlyphSlot {
                advance,
                ..GlyphSlot::EMPTY
            };
        };
        let bounds = outline.px_bounds();
        let width = bounds.width().ceil().max(0.0) as u32;
        let height = bounds.height().ceil().max(0.0) as u32;
        if width == 0 || height == 0 {
            return GlyphSlot {
                advance,
                ..GlyphSlot::EMPTY
            };
        }

        let mut coverage = vec![0u8; (width * height) as usize];
        outline.draw(|x, y, value| {
            let index = (y * width + x) as usize;
            if index < coverage.len() {
                coverage[index] = (value.clamp(0.0, 1.0) * 255.0).round() as u8;
            }
        });

        let mut atlas = self.atlas.borrow_mut();
        let Some((origin_x, origin_y)) = atlas.alloc(width, height) else {
            // Atlas full: keep the advance so layout stays correct.
            return GlyphSlot {
                advance,
                ..GlyphSlot::EMPTY
            };
        };
        atlas.blit(origin_x, origin_y, width, height, &coverage);

        let uv = [
            origin_x as f32 / ATLAS_WIDTH as f32,
            origin_y as f32 / ATLAS_HEIGHT as f32,
            (origin_x + width) as f32 / ATLAS_WIDTH as f32,
            (origin_y + height) as f32 / ATLAS_HEIGHT as f32,
        ];
        GlyphSlot {
            uv,
            size: [width as f32, height as f32],
            // `px_bounds` is already in y-down screen space relative to the
            // glyph position (baseline); its `min` is the top-left corner.
            offset: [bounds.min.x, bounds.min.y],
            advance,
        }
    }
}

impl std::fmt::Debug for SystemFont {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SystemFont")
            .field("name", &self.name)
            .finish()
    }
}

fn px_scale(font_size: f32) -> PxScale {
    PxScale::from(font_size.max(1.0))
}

/// Splits `text` into bidi runs in visual order, with `(byte range, is_rtl)`.
///
/// Pure and font-independent, so it is unit-testable without a font.
fn bidi_runs(text: &str) -> Vec<(Range<usize>, bool)> {
    if text.is_empty() {
        return Vec::new();
    }
    let bidi = BidiInfo::new(text, None);
    let mut runs = Vec::new();
    for para in &bidi.paragraphs {
        if para.range.is_empty() {
            continue;
        }
        let (levels, visual) = bidi.visual_runs(para, para.range.clone());
        for run in visual {
            if run.is_empty() {
                continue;
            }
            let rtl = levels[run.start].is_rtl();
            runs.push((run, rtl));
        }
    }
    runs
}

/// Best script for `text`: the first character that is not Common/Inherited.
fn run_script(text: &str) -> rustybuzz::Script {
    for ch in text.chars() {
        let script = unicode_script::Script::from(ch);
        if script != unicode_script::Script::Common && script != unicode_script::Script::Inherited {
            return script_from_name(script.short_name());
        }
    }
    rustybuzz::script::LATIN
}

fn script_from_name(name: &str) -> rustybuzz::Script {
    let bytes = name.as_bytes();
    if let [a, b, c, d] = bytes {
        let tag = rustybuzz::ttf_parser::Tag::from_bytes(&[*a, *b, *c, *d]);
        if let Some(script) = rustybuzz::Script::from_iso15924_tag(tag) {
            return script;
        }
    }
    rustybuzz::script::LATIN
}

/// `QUILL_FONT` override plus a short per-OS candidate list.
fn candidate_paths() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(path) = std::env::var("QUILL_FONT") {
        candidates.push(PathBuf::from(path));
    }

    #[cfg(target_os = "macos")]
    {
        candidates.push("/System/Library/Fonts/Supplemental/Arial Unicode.ttf".into());
        candidates.push("/Library/Fonts/Arial Unicode.ttf".into());
        candidates.push("/System/Library/Fonts/Supplemental/Arial.ttf".into());
        candidates.push("/System/Library/Fonts/Helvetica.ttc".into());
    }
    #[cfg(target_os = "linux")]
    {
        candidates.push("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf".into());
        candidates.push("/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf".into());
        candidates.push("/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc".into());
    }
    #[cfg(target_os = "windows")]
    {
        candidates.push("C:/Windows/Fonts/arial.ttf".into());
        candidates.push("C:/Windows/Fonts/msyh.ttc".into());
    }
    candidates
}

/// A simple shelf-packing RGBA8 atlas (white RGB + coverage alpha).
struct Atlas {
    pixels: Vec<u8>,
    pen_x: u32,
    pen_y: u32,
    row_height: u32,
    dirty: bool,
}

impl Atlas {
    fn new() -> Self {
        Self {
            pixels: vec![0u8; (ATLAS_WIDTH * ATLAS_HEIGHT * 4) as usize],
            pen_x: 0,
            pen_y: 0,
            row_height: 0,
            dirty: false,
        }
    }

    fn alloc(&mut self, width: u32, height: u32) -> Option<(u32, u32)> {
        if width == 0 || height == 0 {
            return None;
        }
        if self.pen_x + width + PADDING > ATLAS_WIDTH {
            self.pen_x = 0;
            self.pen_y += self.row_height + PADDING;
            self.row_height = 0;
        }
        if self.pen_y + height + PADDING > ATLAS_HEIGHT {
            return None;
        }
        let origin = (self.pen_x, self.pen_y);
        self.pen_x += width + PADDING;
        self.row_height = self.row_height.max(height);
        Some(origin)
    }

    fn blit(&mut self, x: u32, y: u32, width: u32, height: u32, coverage: &[u8]) {
        for row in 0..height {
            for col in 0..width {
                let alpha = coverage[(row * width + col) as usize];
                if alpha == 0 {
                    continue;
                }
                let index = (((y + row) * ATLAS_WIDTH + (x + col)) * 4) as usize;
                self.pixels[index] = 255;
                self.pixels[index + 1] = 255;
                self.pixels[index + 2] = 255;
                self.pixels[index + 3] = alpha;
            }
        }
        self.dirty = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 临时基准：一次 shape 的成本（拖动帧里每条文本都要走）。
    ///
    /// 默认忽略：时序断言在 CI 上不稳定；`--ignored` 手动跑，用来对比缓存
    /// 前后的单次成本（debug 下尤其明显，shape 一次从 ~300µs 降到 ~1µs）。
    #[test]
    #[ignore]
    fn probe_shape_cost() {
        let Some(font) = SystemFont::load() else {
            eprintln!("no font, skip");
            return;
        };
        let samples = [
            "up ↑↓ · Enter 打开 · Backspace 上级 · R 重扫",
            "00001A30  5a 5a 5a 5a 5a 5a 5a 5a  5a 5a 5a 5a 5a 5a 5a 5a",
            "entry_00042.log",
        ];
        // 先把冷启动成本付掉，再测热路径 —— 拖动帧里的就是热路径。
        for sample in samples {
            let _ = font.shape(sample);
        }
        for sample in samples {
            let start = std::time::Instant::now();
            let n = 200;
            for _ in 0..n {
                let _ = font.shape(sample);
            }
            let per = start.elapsed() / n;
            eprintln!(
                "shape({:>2} chars): {:>10.1?} / call",
                sample.chars().count(),
                per
            );
        }
    }

    /// 缓存命中要给出和直接整形一致的结果 —— 缓存不能改变输出。
    #[test]
    fn the_shape_cache_returns_the_same_glyphs() {
        let Some(font) = SystemFont::load() else {
            eprintln!("no font, skip");
            return;
        };
        let text = "预览 00001A30  5a 5a · 双向 mixed אבג";
        let expected: Vec<(u16, f32)> = font
            .shape_uncached(text)
            .iter()
            .map(|g| (g.glyph_id, g.x_advance))
            .collect();
        assert!(!expected.is_empty());
        let _ = font.shape(text); // 首次写入缓存
        let cached: Vec<(u16, f32)> = font
            .shape(text)
            .iter()
            .map(|g| (g.glyph_id, g.x_advance))
            .collect();
        assert_eq!(cached, expected);
    }

    /// 同一段文本在缓存前后的 slot 完全一致（不只 id/advance，offset 也是）。
    #[test]
    fn shape_px_is_identical_before_and_after_caching() {
        let Some(font) = SystemFont::load() else {
            eprintln!("no font, skip");
            return;
        };
        let text = "entry_00042.log";
        let expected = font.shape_px(text, 16.0);
        let again = font.shape_px(text, 16.0);
        assert_eq!(expected, again);
    }

    /// 缓存过载时整体清空，而不是无界增长。
    #[test]
    fn the_shape_cache_clears_past_its_cap() {
        let Some(font) = SystemFont::load() else {
            eprintln!("no font, skip");
            return;
        };
        for index in 0..(SHAPE_CACHE_CAP + 64) {
            let _ = font.shape(&format!("row-{index:06}"));
        }
        assert!(font.shape_cache.borrow().len() <= SHAPE_CACHE_CAP);
    }

    #[test]
    fn bidi_runs_reorder_rtl_around_latin() {
        // Logical: "abc" then Hebrew "אבג". Visual order keeps "abc" first
        // (LTR base) and the Hebrew run reversed as one block.
        let text = "abc\u{5D0}\u{5D1}\u{5D2}";
        let runs = bidi_runs(text);
        let joined: String = runs.iter().map(|(r, _)| &text[r.clone()]).collect();
        assert_eq!(joined, text, "runs must cover the text");
        assert!(!runs[0].1, "leading Latin run is LTR");
        assert!(runs.last().unwrap().1, "Hebrew run is RTL");
        assert_eq!(
            &text[runs.last().unwrap().0.clone()],
            "\u{5D0}\u{5D1}\u{5D2}"
        );
    }

    #[test]
    fn bidi_runs_pure_ltr_is_one_run() {
        let runs = bidi_runs("hello");
        assert_eq!(runs.len(), 1);
        assert!(!runs[0].1);
    }

    #[test]
    fn script_detection_prefers_strong_characters() {
        assert_eq!(run_script("123abc"), rustybuzz::script::LATIN);
        assert_eq!(run_script("\u{5D0}"), rustybuzz::script::HEBREW);
    }
}
