//! System font discovery: scan font directories into face metadata.
//!
//! Metadata only — the file bytes are read to parse the `name`/`OS/2`/`cmap`
//! tables and then dropped. A face's bytes are loaded (and leaked) lazily by
//! [`crate::face::FontFace::load`] the first time it is used.

use std::path::{Path, PathBuf};

use draw_core::FontWeight;

/// One discovered face: enough to resolve and load it, without the bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FaceInfo {
    pub(crate) family: String,
    pub(crate) weight: FontWeight,
    pub(crate) file: PathBuf,
    pub(crate) index: u32,
}

/// Scans `dirs` recursively for font files and returns the normal-style faces,
/// sorted deterministically by `(family, weight, file, index)`.
pub(crate) fn scan(dirs: &[PathBuf]) -> Vec<FaceInfo> {
    let mut faces = Vec::new();
    for dir in dirs {
        if !dir.is_dir() {
            continue;
        }
        walk(dir, 0, &mut |path| {
            if !has_font_extension(path) {
                return;
            }
            collect_file(path, &mut faces);
        });
    }
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
    faces
}

/// Per-OS default font directories.
pub(crate) fn system_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    #[cfg(target_os = "macos")]
    {
        dirs.push(PathBuf::from("/System/Library/Fonts"));
        dirs.push(PathBuf::from("/System/Library/Fonts/Supplemental"));
        dirs.push(PathBuf::from("/Library/Fonts"));
        if let Some(home) = home_dir() {
            dirs.push(home.join("Library/Fonts"));
        }
    }
    #[cfg(target_os = "linux")]
    {
        dirs.push(PathBuf::from("/usr/share/fonts"));
        dirs.push(PathBuf::from("/usr/local/share/fonts"));
        if let Some(home) = home_dir() {
            dirs.push(home.join(".fonts"));
            dirs.push(home.join(".local/share/fonts"));
        }
    }
    #[cfg(target_os = "windows")]
    {
        dirs.push(PathBuf::from("C:/Windows/Fonts"));
    }
    dirs
}

/// Additional macOS asset directories that hold optional system fonts
/// (PingFang lives under `AssetsV2/com_apple_MobileAsset_Font*`).
#[cfg(target_os = "macos")]
pub(crate) fn asset_dirs() -> Vec<PathBuf> {
    let root = PathBuf::from("/System/Library/AssetsV2");
    if !root.is_dir() {
        return Vec::new();
    }
    let Ok(entries) = std::fs::read_dir(&root) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_dir()
                && path.file_name().is_some_and(|name| {
                    name.to_string_lossy()
                        .starts_with("com_apple_MobileAsset_Font")
                })
        })
        .collect()
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn asset_dirs() -> Vec<PathBuf> {
    Vec::new()
}

/// Parses every face of `path` and appends its normal-style metadata.
fn collect_file(path: &Path, out: &mut Vec<FaceInfo>) {
    let Ok(bytes) = std::fs::read(path) else {
        return;
    };
    let count = ttf_parser::fonts_in_collection(&bytes).unwrap_or(1);
    for index in 0..count {
        let Ok(face) = ttf_parser::Face::parse(&bytes, index) else {
            continue;
        };
        if face.is_italic() || face.is_oblique() {
            continue;
        }
        let Some(family) = family_name(&face) else {
            continue;
        };
        out.push(FaceInfo {
            family,
            weight: FontWeight::new(face.weight().to_number()),
            file: path.to_path_buf(),
            index,
        });
    }
}

/// Best English family name from a face's `name` table (id 1, else 16).
pub(crate) fn family_name(face: &ttf_parser::Face<'_>) -> Option<String> {
    let names = face.names();
    for wanted in [1u16, 16] {
        let mut fallback = None;
        for name in names {
            if name.name_id != wanted {
                continue;
            }
            let Some(text) = name.to_string() else {
                continue;
            };
            if name.language() == ttf_parser::Language::English_UnitedStates {
                return Some(text);
            }
            fallback.get_or_insert(text);
        }
        if fallback.is_some() {
            return fallback;
        }
    }
    None
}

fn has_font_extension(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_ascii_lowercase())
            .as_deref(),
        Some("ttf" | "otf" | "ttc" | "otc")
    )
}

/// Depth-limited recursive walk (fonts are shallow; assets are not).
fn walk(dir: &Path, depth: usize, visit: &mut impl FnMut(&Path)) {
    const MAX_DEPTH: usize = 6;
    if depth > MAX_DEPTH {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().map(|n| n.to_string_lossy().into_owned()) else {
            continue;
        };
        if name.starts_with('.') {
            continue;
        }
        if path.is_dir() {
            walk(&path, depth + 1, visit);
        } else {
            visit(&path);
        }
    }
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn font_extension_is_recognized() {
        assert!(has_font_extension(Path::new("/x/Foo.ttf")));
        assert!(has_font_extension(Path::new("/x/Foo.TTC")));
        assert!(!has_font_extension(Path::new("/x/Foo.txt")));
        assert!(!has_font_extension(Path::new("/x/Foo")));
    }

    #[test]
    fn scan_missing_dir_is_empty() {
        assert!(scan(&[PathBuf::from("/definitely/not/here")]).is_empty());
    }
}
