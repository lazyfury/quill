//! The [`FontServer`]: discovery, family/weight resolution, shaping with
//! per-character fallback, and glyph rasterization into a shared atlas.

use std::cell::{Cell, OnceCell, RefCell};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::PathBuf;
use std::rc::Rc;

use draw_core::FontWeight;

use crate::atlas::{Atlas, ATLAS_HEIGHT, ATLAS_WIDTH};
use crate::bitmap;
use crate::discovery::{self, FaceInfo};
use crate::face::{FontFace, GlyphSlot};
use crate::shaping::bidi_runs;

/// Which kind of font to use for `DrawText`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontMode {
    /// Real system fonts: proportional advances, CJK, fallback, antialiased.
    System,
    /// The built-in fixed-size pixel font (`font8x8`).
    Pixel,
}

/// Pixel-font glyph cell size as a fraction of `font_size`.
pub const PIXEL_GLYPH_RATIO: f32 = 0.75;

/// Rounded glyph cell size in logical pixels for `font_size`.
fn pixel_cell(font_size: f32) -> f32 {
    (font_size * PIXEL_GLYPH_RATIO).round().max(1.0)
}

/// A concrete font face: a file plus the face index inside a collection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FaceRef {
    pub file: PathBuf,
    pub index: u32,
}

impl FaceRef {
    pub fn new(file: impl Into<PathBuf>, index: u32) -> Self {
        Self {
            file: file.into(),
            index,
        }
    }
}

/// Font configuration for a [`FontServer`].
#[derive(Debug, Clone, PartialEq)]
pub struct FontConfig {
    pub mode: FontMode,
    /// Rasterize system glyphs at `font_size * scale` (crisp on HiDPI).
    ///
    /// Ignored in [`FontMode::Pixel`].
    pub device_pixel_rasterization: bool,
    /// Preferred default family. `None` uses a short per-OS list
    /// ([`FontServer::default_family`]).
    pub default_family: Option<String>,
    /// Use this face as the default **without scanning the system**.
    ///
    /// The scan is deferred until something actually needs another face (a
    /// `families()` picker, a family miss, or an uncovered character). Use it
    /// when the application ships or knows its font file.
    pub default_face: Option<FaceRef>,
}

impl Default for FontConfig {
    fn default() -> Self {
        Self {
            mode: FontMode::System,
            device_pixel_rasterization: true,
            default_family: None,
            default_face: None,
        }
    }
}

/// A discovered family and the weights it ships.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FontFamilyInfo {
    pub name: String,
    /// Distinct `usWeightClass` values, ascending.
    pub weights: Vec<u16>,
}

/// A font request: a family name (or the default) and a desired weight.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FontRequest {
    pub family: Option<String>,
    pub weight: FontWeight,
}

impl FontRequest {
    pub fn new(family: impl Into<String>, weight: u16) -> Self {
        Self {
            family: Some(family.into()),
            weight: FontWeight::new(weight),
        }
    }

    /// The default family at `weight`.
    pub fn default_at(weight: u16) -> Self {
        Self {
            family: None,
            weight: FontWeight::new(weight),
        }
    }
}

/// A resolved face handle (an index into the server's face list).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FontId(pub usize);

/// The font service: discovered faces, lazy loading, shaping and the atlas.
pub struct FontServer {
    mode: FontMode,
    /// Faces known so far: a seed (an explicit [`FaceRef`]) plus, once
    /// discovery runs, the scanned system faces appended to it.
    faces: RefCell<Vec<FaceInfo>>,
    /// Whether the system scan has run (or is not needed, e.g. pixel mode).
    discovered: Cell<bool>,
    /// The family list, built (and discovery triggered) on first access.
    families: OnceCell<Vec<FontFamilyInfo>>,
    default_family: OnceCell<String>,
    /// Lazily loaded faces, parallel to `faces` (`None` until first use).
    loaded: RefCell<Vec<Option<Rc<FontFace>>>>,
    /// Resolved primary face per weight value.
    primary_cache: RefCell<HashMap<u16, usize>>,
    /// Per-character chosen face (`(char, weight) -> face index`).
    coverage_cache: RefCell<HashMap<(char, u16), usize>>,
    atlas: RefCell<Atlas>,
    scale: Cell<f32>,
    device_pixels: Cell<bool>,
}

impl FontServer {
    /// Loads the server with the default [`FontConfig`].
    pub fn load() -> Self {
        Self::load_with(FontConfig::default())
    }

    /// Loads the server described by `config`.
    ///
    /// An explicit [`FontConfig::default_face`] seeds the server **without
    /// scanning the system**; the scan is deferred to the first picker /
    /// family miss / fallback. With no seed the system is scanned now, and if
    /// no font can be found `System` mode falls back to the pixel bitmap.
    pub fn load_with(config: FontConfig) -> Self {
        if config.mode == FontMode::Pixel {
            return Self::bitmap_server(config);
        }

        // A known face avoids the scan entirely until another face is needed.
        if let Some(face) = config.default_face.as_ref() {
            let faces = seed_from_face(face);
            if !faces.is_empty() {
                return Self::system_server(config, faces, false);
            }
        }

        // No usable seed: discover now so the default family (or the pixel
        // fallback) is known before the first frame.
        let mut faces = discovery::scan(&discovery::system_dirs());
        faces.extend(discovery::scan(&discovery::asset_dirs()));
        faces.sort_by(|a, b| {
            a.family
                .cmp(&b.family)
                .then(a.weight.cmp(&b.weight))
                .then(a.file.cmp(&b.file))
                .then(a.index.cmp(&b.index))
        });
        faces.dedup_by(|a, b| {
            a.family == b.family && a.weight == b.weight && a.file == b.file && a.index == b.index
        });
        if faces.is_empty() {
            return Self::bitmap_server(config);
        }
        Self::system_server(config, faces, true)
    }

    /// Builds a `System` server. `discovered` is false when only a seed face is
    /// known and the system scan is still pending.
    fn system_server(config: FontConfig, faces: Vec<FaceInfo>, discovered: bool) -> Self {
        let default_family =
            pick_default(&build_families(&faces), config.default_family.as_deref());
        let loaded = RefCell::new(vec![None; faces.len()]);
        let server = Self {
            mode: FontMode::System,
            faces: RefCell::new(faces),
            discovered: Cell::new(discovered),
            families: OnceCell::new(),
            default_family: OnceCell::new(),
            loaded,
            primary_cache: RefCell::new(HashMap::new()),
            coverage_cache: RefCell::new(HashMap::new()),
            atlas: RefCell::new(Atlas::new()),
            scale: Cell::new(1.0),
            device_pixels: Cell::new(config.device_pixel_rasterization),
        };
        let _ = server.default_family.set(default_family);
        server
    }

    fn bitmap_server(config: FontConfig) -> Self {
        Self {
            mode: FontMode::Pixel,
            faces: RefCell::new(Vec::new()),
            discovered: Cell::new(true),
            families: OnceCell::new(),
            default_family: OnceCell::new(),
            loaded: RefCell::new(Vec::new()),
            primary_cache: RefCell::new(HashMap::new()),
            coverage_cache: RefCell::new(HashMap::new()),
            atlas: RefCell::new(Atlas::new()),
            scale: Cell::new(1.0),
            device_pixels: Cell::new(config.device_pixel_rasterization),
        }
    }

    /// The built-in fixed pixel bitmap font, with no system discovery.
    pub fn bitmap() -> Self {
        Self::bitmap_server(FontConfig {
            mode: FontMode::Pixel,
            device_pixel_rasterization: false,
            default_family: None,
            default_face: None,
        })
    }

    /// Runs the deferred system scan once and appends any new faces.
    fn ensure_discovered(&self) {
        if self.mode != FontMode::System || self.discovered.get() {
            return;
        }
        // Set first so a nested call cannot re-enter the scan.
        self.discovered.set(true);
        let mut scan = discovery::scan(&discovery::system_dirs());
        scan.extend(discovery::scan(&discovery::asset_dirs()));
        self.append_faces(scan);
        // New faces can change the nearest weight for a family.
        self.primary_cache.borrow_mut().clear();
        self.coverage_cache.borrow_mut().clear();
    }

    /// Appends scanned faces that are not already known (by file + index).
    fn append_faces(&self, scan: Vec<FaceInfo>) {
        let mut faces = self.faces.borrow_mut();
        let mut loaded = self.loaded.borrow_mut();
        for info in scan {
            if faces
                .iter()
                .any(|f| f.file == info.file && f.index == info.index)
            {
                continue;
            }
            faces.push(info);
            loaded.push(None);
        }
    }

    // -- discovery ---------------------------------------------------------

    /// The discovered families and their weights, for an application font
    /// picker. The first call runs the deferred system scan.
    pub fn families(&self) -> &[FontFamilyInfo] {
        self.families.get_or_init(|| {
            self.ensure_discovered();
            build_families(&self.faces.borrow())
        })
    }

    /// The default family name (empty in pixel mode).
    pub fn default_family(&self) -> &str {
        self.default_family.get_or_init(|| {
            self.ensure_discovered();
            pick_default(&build_families(&self.faces.borrow()), None)
        })
    }

    /// Resolves `request` to a concrete face: nearest weight in the requested
    /// family, falling back to the default family. Never fails while any face
    /// is loaded.
    ///
    /// A family that is not among the seed faces triggers the deferred system
    /// scan once.
    pub fn resolve(&self, request: &FontRequest) -> FontId {
        if let Some(id) = self.try_resolve(request) {
            return id;
        }
        self.ensure_discovered();
        self.try_resolve(request).unwrap_or(FontId(0))
    }

    fn try_resolve(&self, request: &FontRequest) -> Option<FontId> {
        let default = self.default_family();
        let wanted = request
            .family
            .as_deref()
            .filter(|name| !name.is_empty())
            .unwrap_or(default);
        let faces = self.faces.borrow();
        if faces.is_empty() {
            return None;
        }
        if let Some(index) = nearest_in(&faces, wanted, request.weight) {
            return Some(FontId(index));
        }
        if !wanted.eq_ignore_ascii_case(default) {
            return nearest_in(&faces, default, request.weight).map(FontId);
        }
        None
    }

    /// The primary face index for `weight` (the default family).
    fn primary(&self, weight: FontWeight) -> usize {
        if let Some(index) = self.primary_cache.borrow().get(&weight.value()) {
            return *index;
        }
        let index = self.resolve(&FontRequest::default_at(weight.value())).0;
        self.primary_cache
            .borrow_mut()
            .insert(weight.value(), index);
        index
    }

    /// The face index that covers `ch`, preferring the primary and then the
    /// (deterministic) face order. An uncovered character triggers the
    /// deferred scan once.
    fn face_for(&self, ch: char, primary: usize, weight: FontWeight) -> usize {
        let key = (ch, weight.value());
        if let Some(index) = self.coverage_cache.borrow().get(&key) {
            return *index;
        }
        if !self.face_covers(primary, ch) && !self.discovered.get() {
            self.ensure_discovered();
        }
        let index = if self.face_covers(primary, ch) {
            primary
        } else {
            self.fallback_face(ch).unwrap_or(primary)
        };
        self.coverage_cache.borrow_mut().insert(key, index);
        index
    }

    /// The first face (in face order) that covers `ch`.
    fn fallback_face(&self, ch: char) -> Option<usize> {
        let count = self.faces.borrow().len();
        (0..count).find(|&index| self.face_covers(index, ch))
    }

    fn face_covers(&self, index: usize, ch: char) -> bool {
        self.face_at(index).is_some_and(|face| face.covers(ch))
    }

    fn face_at(&self, index: usize) -> Option<Rc<FontFace>> {
        if let Some(face) = self.loaded.borrow().get(index).and_then(Option::clone) {
            return Some(face);
        }
        let info = self.faces.borrow().get(index).cloned()?;
        let face = Rc::new(FontFace::load(&info.file, info.index)?);
        self.loaded.borrow_mut()[index] = Some(face.clone());
        Some(face)
    }

    /// Splits `text` into `(face index, sub-text, rtl)` shaping runs, applying
    /// bidi reordering and per-character font fallback.
    fn runs<'a>(&self, text: &'a str, weight: FontWeight) -> Vec<(usize, &'a str, bool)> {
        let primary = self.primary(weight);
        let mut out = Vec::new();
        for (range, rtl) in bidi_runs(text) {
            let run = &text[range];
            let mut start = 0usize;
            let mut current: Option<usize> = None;
            for (offset, ch) in run.char_indices() {
                let index = self.face_for(ch, primary, weight);
                match current {
                    Some(cur) if cur == index => {}
                    Some(cur) => {
                        out.push((cur, &run[start..offset], rtl));
                        start = offset;
                        current = Some(index);
                    }
                    None => {
                        current = Some(index);
                        start = offset;
                    }
                }
            }
            if let Some(cur) = current {
                out.push((cur, &run[start..], rtl));
            }
        }
        out
    }

    // -- metrics -----------------------------------------------------------

    /// Rasterization `(pixels, divisor)`: convert logical size to atlas pixels
    /// and back.
    fn raster_params(&self, font_size: f32) -> (f32, f32) {
        if self.mode == FontMode::System && self.device_pixels.get() {
            let scale = self.scale.get().max(1e-3);
            ((font_size * scale).round().max(1.0), scale)
        } else if self.mode == FontMode::System {
            (font_size.round().max(1.0), 1.0)
        } else {
            (font_size, 1.0)
        }
    }

    pub fn advance(&self, ch: char, font_size: f32, weight: FontWeight) -> f32 {
        if self.mode == FontMode::Pixel {
            return pixel_cell(font_size);
        }
        let (px, divisor) = self.raster_params(font_size);
        let primary = self.primary(weight);
        let index = self.face_for(ch, primary, weight);
        self.face_at(index)
            .map(|face| face.advance_px(ch, px) / divisor)
            .unwrap_or(0.0)
    }

    pub fn line_height(&self, font_size: f32) -> f32 {
        if self.mode == FontMode::Pixel {
            return font_size;
        }
        let (px, divisor) = self.raster_params(font_size);
        self.face_at(self.primary(FontWeight::NORMAL))
            .map(|face| face.line_height_px(px) / divisor)
            .unwrap_or(font_size)
    }

    pub fn ascent(&self, font_size: f32) -> f32 {
        if self.mode == FontMode::Pixel {
            return font_size * 0.5 + pixel_cell(font_size) * 0.5;
        }
        let (px, divisor) = self.raster_params(font_size);
        self.face_at(self.primary(FontWeight::NORMAL))
            .map(|face| face.ascent_px(px) / divisor)
            .unwrap_or(font_size * 0.8)
    }

    /// Shaped advance width of `text` (logical units), with fallback.
    pub fn advance_run(&self, text: &str, font_size: f32, weight: FontWeight) -> f32 {
        if self.mode == FontMode::Pixel {
            return text.chars().count() as f32 * pixel_cell(font_size);
        }
        let (px, divisor) = self.raster_params(font_size);
        self.runs(text, weight)
            .into_iter()
            .map(|(index, sub, rtl)| {
                self.face_at(index)
                    .map(|face| face.advance_run_px(sub, rtl, px) / divisor)
                    .unwrap_or(0.0)
            })
            .sum()
    }

    /// Shapes `text` at `font_size` into glyphs in visual order (logical
    /// units), applying bidi reordering, per-character fallback and
    /// rasterizing each glyph into the shared atlas.
    pub fn shape(&self, text: &str, font_size: f32, weight: FontWeight) -> Vec<GlyphSlot> {
        if self.mode == FontMode::Pixel {
            return text
                .chars()
                .map(|ch| self.bitmap_glyph(ch, font_size))
                .collect();
        }
        let (px, divisor) = self.raster_params(font_size);
        let mut slots = Vec::new();
        for (index, sub, rtl) in self.runs(text, weight) {
            let Some(face) = self.face_at(index) else {
                continue;
            };
            for mut slot in face.shape_run_px(sub, rtl, px, &self.atlas) {
                slot.advance /= divisor;
                slot.size = [slot.size[0] / divisor, slot.size[1] / divisor];
                slot.offset = [slot.offset[0] / divisor, slot.offset[1] / divisor];
                slots.push(slot);
            }
        }
        slots
    }

    fn bitmap_glyph(&self, ch: char, font_size: f32) -> GlyphSlot {
        let cell = pixel_cell(font_size);
        GlyphSlot {
            uv: bitmap::glyph_uv(ch),
            size: [cell, cell],
            offset: [0.0, -cell],
            advance: cell,
        }
    }

    // -- atlas / config ----------------------------------------------------

    /// `(width, height)` of the atlas texture this server needs.
    pub fn atlas_size(&self) -> (u32, u32) {
        if self.mode == FontMode::System {
            (ATLAS_WIDTH, ATLAS_HEIGHT)
        } else {
            (bitmap::ATLAS_WIDTH, bitmap::ATLAS_HEIGHT)
        }
    }

    /// Initial atlas contents (system fonts start empty).
    pub fn initial_atlas(&self) -> Vec<u8> {
        if self.mode == FontMode::System {
            vec![0u8; (ATLAS_WIDTH * ATLAS_HEIGHT * 4) as usize]
        } else {
            bitmap::build_atlas()
        }
    }

    /// Full atlas pixels if new glyphs were rasterized since the last call.
    pub fn take_dirty_atlas(&self) -> Option<Vec<u8>> {
        if self.mode == FontMode::System {
            self.atlas.borrow_mut().take_dirty()
        } else {
            None
        }
    }

    pub fn set_scale(&self, scale: f32) {
        self.scale.set(scale.max(0.0));
    }

    pub fn set_device_pixel_rasterization(&self, enabled: bool) {
        self.device_pixels.set(enabled);
    }

    /// Whether real system fonts are in use.
    pub fn is_system(&self) -> bool {
        self.mode == FontMode::System
    }

    /// The mode this server resolves to (`System` may have fallen back to
    /// `Pixel`).
    pub fn mode(&self) -> FontMode {
        self.mode
    }

    /// The default family name, or `None` in pixel mode.
    pub fn name(&self) -> Option<&str> {
        if self.mode == FontMode::System {
            Some(self.default_family())
        } else {
            None
        }
    }
}

/// A cloneable handle to a [`FontServer`]'s metrics.
///
/// Hosts wrap this in a `draw_ui::TextMeasurer` so layout measures text with
/// the exact metrics the backend renders with.
#[derive(Clone)]
pub struct FontMetrics {
    server: Rc<FontServer>,
}

impl FontMetrics {
    pub fn new(server: Rc<FontServer>) -> Self {
        Self { server }
    }

    pub fn advance(&self, ch: char, font_size: f32) -> f32 {
        self.server.advance(ch, font_size, FontWeight::NORMAL)
    }

    pub fn advance_weighted(&self, ch: char, font_size: f32, weight: FontWeight) -> f32 {
        self.server.advance(ch, font_size, weight)
    }

    pub fn line_height(&self, font_size: f32) -> f32 {
        self.server.line_height(font_size)
    }

    pub fn ascent(&self, font_size: f32) -> f32 {
        self.server.ascent(font_size)
    }

    /// Shaped advance width of `text`, matching what the backend draws.
    pub fn measure_run(&self, text: &str, font_size: f32) -> f32 {
        self.server.advance_run(text, font_size, FontWeight::NORMAL)
    }

    /// Shaped advance width of `text` at `weight`, with fallback.
    pub fn measure_run_weighted(&self, text: &str, font_size: f32, weight: FontWeight) -> f32 {
        self.server.advance_run(text, font_size, weight)
    }

    pub fn is_system(&self) -> bool {
        self.server.is_system()
    }

    pub fn name(&self) -> Option<&str> {
        self.server.name()
    }

    /// The discovered families (for a font picker).
    pub fn families(&self) -> &[FontFamilyInfo] {
        self.server.families()
    }
}

impl std::fmt::Debug for FontServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let faces = self.faces.borrow().len();
        f.debug_struct("FontServer")
            .field("mode", &self.mode)
            .field("faces", &faces)
            .field("discovered", &self.discovered.get())
            .field("default", &self.default_family.get())
            .finish()
    }
}

impl std::fmt::Debug for FontMetrics {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FontMetrics")
            .field("system", &self.is_system())
            .field("name", &self.name())
            .finish()
    }
}

fn build_families(faces: &[FaceInfo]) -> Vec<FontFamilyInfo> {
    let mut map: BTreeMap<String, BTreeSet<u16>> = BTreeMap::new();
    for face in faces {
        map.entry(face.family.clone())
            .or_default()
            .insert(face.weight.value());
    }
    map.into_iter()
        .map(|(name, weights)| FontFamilyInfo {
            name,
            weights: weights.into_iter().collect(),
        })
        .collect()
}

fn pick_default(families: &[FontFamilyInfo], requested: Option<&str>) -> String {
    if let Some(name) = requested {
        if let Some(family) = families.iter().find(|f| f.name.eq_ignore_ascii_case(name)) {
            return family.name.clone();
        }
    }
    for preferred in preferred_families() {
        if let Some(family) = families
            .iter()
            .find(|f| f.name.eq_ignore_ascii_case(preferred))
        {
            return family.name.clone();
        }
    }
    families.first().map(|f| f.name.clone()).unwrap_or_default()
}

fn preferred_families() -> &'static [&'static str] {
    #[cfg(target_os = "macos")]
    {
        &[
            "PingFang SC",
            "Helvetica Neue",
            "Arial Unicode MS",
            "Arial",
            "Helvetica",
        ]
    }
    #[cfg(target_os = "linux")]
    {
        &["Noto Sans", "DejaVu Sans", "Liberation Sans"]
    }
    #[cfg(target_os = "windows")]
    {
        &["Segoe UI", "Microsoft YaHei", "Arial"]
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        &[]
    }
}

/// The nearest face index in `family` to `weight` (tie -> heavier).
fn nearest_in(faces: &[FaceInfo], family: &str, weight: FontWeight) -> Option<usize> {
    let mut best: Option<(usize, u16)> = None;
    for (index, info) in faces.iter().enumerate() {
        if !info.family.eq_ignore_ascii_case(family) {
            continue;
        }
        let distance = info.weight.value().abs_diff(weight.value());
        match best {
            None => best = Some((index, distance)),
            Some((best_index, best_distance)) => {
                let heavier = info.weight.value() > faces[best_index].weight.value();
                if distance < best_distance || (distance == best_distance && heavier) {
                    best = Some((index, distance));
                }
            }
        }
    }
    best.map(|(index, _)| index)
}

/// Reads a [`FaceRef`] into a single seed face (empty if it cannot be read).
fn seed_from_face(face_ref: &FaceRef) -> Vec<FaceInfo> {
    let Some(mapping) = discovery::map_file(&face_ref.file) else {
        return Vec::new();
    };
    let bytes: &[u8] = &mapping;
    let Ok(face) = ttf_parser::Face::parse(bytes, face_ref.index) else {
        return Vec::new();
    };
    let Some(family) = discovery::family_name(&face) else {
        return Vec::new();
    };
    vec![FaceInfo {
        family,
        weight: FontWeight::new(face.weight().to_number()),
        file: face_ref.file.clone(),
        index: face_ref.index,
    }]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn system_server() -> Option<FontServer> {
        let server = FontServer::load();
        server.is_system().then_some(server)
    }

    #[test]
    fn pixel_mode_ignores_system_fonts() {
        let server = FontServer::bitmap();
        assert_eq!(server.mode(), FontMode::Pixel);
        assert!(!server.is_system());
        assert!(server.families().is_empty());
        let slots = server.shape("Hi 中", 16.0, FontWeight::NORMAL);
        assert_eq!(slots.len(), 4, "one slot per character");
    }

    #[test]
    fn discovery_lists_families_and_weights() {
        let Some(server) = system_server() else {
            return;
        };
        assert!(!server.families().is_empty(), "no families discovered");
        assert!(!server.default_family().is_empty());
        let family = &server.families()[0];
        assert!(!family.name.is_empty());
        assert!(!family.weights.is_empty());
    }

    #[test]
    fn resolve_picks_the_nearest_weight() {
        let Some(server) = system_server() else {
            return;
        };
        let Some(family) = server
            .families()
            .iter()
            .find(|f| f.weights.contains(&400) && f.weights.contains(&700))
        else {
            return; // no family ships both weights here
        };
        let regular = server.resolve(&FontRequest::new(&family.name, 400));
        let bold = server.resolve(&FontRequest::new(&family.name, 700));
        assert_ne!(regular, bold);
        assert_eq!(server.faces.borrow()[regular.0].weight.value(), 400);
        assert_eq!(server.faces.borrow()[bold.0].weight.value(), 700);
    }

    #[test]
    fn an_unknown_family_falls_back_to_the_default() {
        let Some(server) = system_server() else {
            return;
        };
        let unknown = server.resolve(&FontRequest::new("No Such Font 12345", 400));
        let default = server.resolve(&FontRequest::default_at(400));
        assert_eq!(unknown, default);
    }

    #[test]
    fn shaping_covers_cjk_and_latin_in_one_line() {
        let Some(server) = system_server() else {
            return;
        };
        let slots = server.shape("Hello 中文", 24.0, FontWeight::NORMAL);
        assert!(!slots.is_empty());
        // At least one glyph rasterized to real ink (fallback reached a face
        // with a CJK outline).
        assert!(
            slots
                .iter()
                .any(|slot| slot.size[0] > 0.0 && slot.size[1] > 0.0),
            "no glyph rasterized"
        );
        assert!(server.take_dirty_atlas().is_some());
    }

    #[test]
    fn face_metadata_matches_the_discovered_entry() {
        let Some(server) = system_server() else {
            return;
        };
        let index = server.primary(FontWeight::NORMAL);
        let Some(face) = server.face_at(index) else {
            return;
        };
        let info = &server.faces.borrow()[index];
        assert_eq!(face.family(), info.family);
        assert_eq!(face.weight(), info.weight);
        assert_eq!(face.file(), info.file);
        assert_eq!(face.index(), info.index);
        assert!(face.covers('A'));
    }

    #[test]
    fn a_default_face_seeds_without_a_family_scan() {
        // Find a real system face, then seed a fresh server with just it.
        let Some(probe) = system_server() else {
            return;
        };
        let info = probe.faces.borrow()[probe.primary(FontWeight::NORMAL)].clone();
        let server = FontServer::load_with(FontConfig {
            default_face: Some(FaceRef::new(info.file.clone(), info.index)),
            ..FontConfig::default()
        });
        assert!(server.is_system());
        assert_eq!(server.default_family(), info.family);
        // Shaping with the seed works without ever building the family list.
        assert!(!server.shape("A", 16.0, FontWeight::NORMAL).is_empty());
    }
}
