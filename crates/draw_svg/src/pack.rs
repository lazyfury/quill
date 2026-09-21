//! Loading an icon pack from a directory of `.svg` files.
//!
//! A pack is a directory tree (e.g. the `icons/` folder of
//! [lucide-static](https://www.npmjs.com/package/lucide-static)); each `.svg`
//! file is indexed by its file stem, so `IconPack::load("brush")` finds
//! `.../brush.svg`. Parsing is on demand and the caller decides whether to
//! cache; nothing here touches a backend or a UI.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::{SvgDocument, SvgError};

/// An index of SVG files by name, rooted at a directory.
#[derive(Debug, Clone)]
pub struct IconPack {
    root: PathBuf,
    icons: BTreeMap<String, PathBuf>,
}

impl IconPack {
    /// Walks `dir` recursively and indexes every `.svg` by its file stem.
    pub fn open(dir: impl AsRef<Path>) -> std::io::Result<Self> {
        let root = dir.as_ref().to_path_buf();
        let mut icons = BTreeMap::new();
        collect(&root, &mut icons)?;
        Ok(Self { root, icons })
    }

    /// The directory the pack was opened from.
    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn len(&self) -> usize {
        self.icons.len()
    }

    pub fn is_empty(&self) -> bool {
        self.icons.is_empty()
    }

    /// Icon names (file stems), sorted.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.icons.keys().map(|name| name.as_str())
    }

    pub fn path(&self, name: &str) -> Option<&Path> {
        self.icons.get(name).map(|path| path.as_path())
    }

    pub fn contains(&self, name: &str) -> bool {
        self.icons.contains_key(name)
    }

    /// Reads and parses an icon. `None` means the name is not in the pack; a
    /// `Some(Err(..))` means the file was found but could not be read/parsed.
    pub fn load(&self, name: &str) -> Option<Result<SvgDocument, SvgError>> {
        let path = self.icons.get(name)?;
        Some(load_file(path))
    }
}

fn load_file(path: &Path) -> Result<SvgDocument, SvgError> {
    let text = fs::read_to_string(path)
        .map_err(|error| SvgError::Io(format!("{}: {error}", path.display())))?;
    SvgDocument::parse(&text)
}

fn collect(dir: &Path, out: &mut BTreeMap<String, PathBuf>) -> std::io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect(&path, out)?;
            continue;
        }
        let is_svg = path
            .extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| extension.eq_ignore_ascii_case("svg"))
            .unwrap_or(false);
        if !is_svg {
            continue;
        }
        if let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) {
            out.entry(stem.to_string()).or_insert(path);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A unique scratch directory for one test.
    fn scratch(label: &str) -> PathBuf {
        let unique = format!(
            "draw_svg_{label}_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or(0)
        );
        std::env::temp_dir().join(unique)
    }

    const ICON: &str =
        r#"<svg viewBox="0 0 24 24" stroke="currentColor"><path d="M0 0 L10 10" /></svg>"#;

    #[test]
    fn a_pack_indexes_svgs_by_file_stem() {
        let dir = scratch("pack");
        fs::create_dir_all(dir.join("nested")).unwrap();
        fs::write(dir.join("brush.svg"), ICON).unwrap();
        fs::write(dir.join("nested").join("eraser.svg"), ICON).unwrap();
        fs::write(dir.join("notes.txt"), "ignore me").unwrap();

        let pack = IconPack::open(&dir).unwrap();
        assert_eq!(pack.len(), 2);
        assert!(pack.contains("brush"));
        assert!(pack.contains("eraser"), "nested files are indexed too");
        assert!(!pack.contains("notes"));
        assert!(!pack.contains("missing"));
        assert_eq!(pack.names().collect::<Vec<_>>(), vec!["brush", "eraser"]);

        let document = pack.load("brush").unwrap().unwrap();
        assert_eq!(document.shapes.len(), 1);
        assert!(pack.load("missing").is_none());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn opening_a_missing_directory_is_an_io_error() {
        let dir = scratch("missing");
        assert!(IconPack::open(&dir).is_err());
    }

    /// Parses every icon in a real pack. Ignored by default; point it at a pack
    /// with `DRAW_SVG_ICON_DIR`:
    ///
    /// ```text
    /// DRAW_SVG_ICON_DIR=/path/to/lucide/icons \
    ///   cargo test -p draw_svg -- --ignored --nocapture every_icon
    /// ```
    #[test]
    #[ignore = "needs a real icon pack (set DRAW_SVG_ICON_DIR)"]
    fn every_icon_in_a_real_pack_parses() {
        use draw_core::{Color, Rect, Size, Vec2};
        use draw_render::PaintContext;

        let Ok(dir) = std::env::var("DRAW_SVG_ICON_DIR") else {
            return;
        };
        let pack = IconPack::open(dir).expect("open the icon pack");
        let target = Rect::from_min_size(Vec2::ZERO, Size::splat(24.0));
        let mut failures = Vec::new();
        let mut shapes = 0usize;
        let mut commands = 0usize;
        for name in pack.names() {
            match pack.load(name) {
                Some(Ok(document)) => {
                    shapes += document.shapes.len();
                    let mut ctx = PaintContext::new();
                    document.draw(&mut ctx, target, Color::BLACK);
                    let count = ctx.into_draw_list().len();
                    if count == 0 {
                        failures.push(format!("{name}: parsed but drew nothing"));
                    }
                    commands += count;
                }
                Some(Err(error)) => failures.push(format!("{name}: {error}")),
                None => {}
            }
        }
        eprintln!(
            "parsed {} icons into {} shapes / {} commands",
            pack.len(),
            shapes,
            commands
        );
        assert!(
            pack.len() > 1000,
            "expected a full pack, got {}",
            pack.len()
        );
        assert!(
            failures.is_empty(),
            "{} icons failed: {:?}",
            failures.len(),
            &failures[..failures.len().min(10)]
        );
    }
}
