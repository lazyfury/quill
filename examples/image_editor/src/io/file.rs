//! 文件读写：`路径 <-> PNG 字节`，外加默认命名 / 图层名规则。

use std::path::{Path, PathBuf};

use crate::document::PixelBuffer;

use super::{decode_png, encode_png, IoError};

/// 把像素写成 PNG 文件。
pub fn write_png(path: &Path, pixels: &PixelBuffer) -> Result<(), IoError> {
    let bytes = encode_png(pixels)?;
    std::fs::write(path, bytes).map_err(IoError::File)
}

/// 从 PNG 文件读出像素。
pub fn read_png(path: &Path) -> Result<PixelBuffer, IoError> {
    let bytes = std::fs::read(path).map_err(IoError::File)?;
    decode_png(&bytes)
}

/// 默认导出路径：当前目录 + 文档名（清洗后）`.png`。
///
/// 没有原生文件对话框（winit 不带），所以给一个合理默认值，用户可以在
/// 「文件」面板里改成任意路径。
pub fn default_export_path(document_name: &str) -> PathBuf {
    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(format!("{}.png", sanitize_stem(document_name)))
}

/// 导入图层的名字：用文件名（去掉扩展名）；没有名字就用 `导入`。
pub fn file_label(path: &Path) -> String {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .filter(|stem| !stem.is_empty())
        .unwrap_or("导入")
        .to_string()
}

/// 把文档名清洗成一个安全的文件名主干：去掉目录分隔符 / 控制字符，空名回退
/// `未命名`，并剥掉已有的 `.png`。
fn sanitize_stem(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|ch| match ch {
            '/' | '\\' | ':' | '\0' => '_',
            ch if ch.is_control() => '_',
            ch => ch,
        })
        .collect();
    let cleaned = cleaned.trim();
    let cleaned = cleaned.strip_suffix(".png").unwrap_or(cleaned);
    if cleaned.is_empty() {
        "untitled".to_string()
    } else {
        cleaned.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Color;
    use std::sync::atomic::{AtomicU32, Ordering};

    /// 测试用的唯一临时路径（进程 id + 原子计数，避免并行测试互相覆盖）。
    fn temp_path(tag: &str) -> PathBuf {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "image_editor_{}_{}_{n}.png",
            std::process::id(),
            tag
        ))
    }

    #[test]
    fn pixels_round_trip_through_a_file() {
        let path = temp_path("roundtrip");
        let pixels = PixelBuffer::filled(2, 2, Color::new(4, 5, 6, 7));
        write_png(&path, &pixels).expect("write");
        let read = read_png(&path).expect("read");
        assert_eq!(read.get_pixel(1, 1), Color::new(4, 5, 6, 7));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn reading_a_missing_file_is_an_io_error() {
        let path = temp_path("missing");
        assert!(matches!(read_png(&path), Err(IoError::File(_))));
    }

    #[test]
    fn the_default_path_uses_a_cleaned_document_name() {
        let path = default_export_path("我的 画布/1.png");
        assert_eq!(path.file_name().unwrap(), "我的 画布_1.png");
        assert_eq!(default_export_path("").file_name().unwrap(), "untitled.png");
    }

    #[test]
    fn the_layer_label_comes_from_the_file_stem() {
        assert_eq!(file_label(Path::new("/tmp/photo.png")), "photo");
        assert_eq!(file_label(Path::new("/")), "导入");
    }
}
