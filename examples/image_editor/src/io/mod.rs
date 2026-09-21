//! 导入导出（Phase 7）：把 [`PixelBuffer`] 与 PNG 文件互相转换。
//!
//! 这是**唯一**接触文件系统与 `png` 编解码的地方；文档模型
//! （[`crate::document`]）仍然只是纯数据，视图层只调用这一层的公开函数。
//! 分成两个关注点：
//!
//! - [`codec`]：内存里的 `PixelBuffer <-> PNG 字节`，不碰文件，最容易测。
//! - [`file`]：`路径 <-> PNG 字节`，加一点默认命名 / 文件名规则。
//!
//! [`PixelBuffer`]: crate::document::PixelBuffer

mod codec;
mod file;

pub use codec::{decode_png, encode_png};
pub use file::{default_export_path, file_label, read_png, write_png};

use std::fmt;

/// 导入导出失败的统一错误。
#[derive(Debug)]
pub enum IoError {
    /// 读写文件失败（打不开、没权限……）。
    File(std::io::Error),
    /// PNG 解码失败（不是 PNG、数据损坏、不支持的格式）。
    Decode(png::DecodingError),
    /// PNG 编码失败。
    Encode(png::EncodingError),
    /// 数据本身不合法（空图、非 8 位色深等）。
    Invalid(String),
}

impl fmt::Display for IoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::File(error) => write!(f, "文件读写失败：{error}"),
            Self::Decode(error) => write!(f, "PNG 解码失败：{error}"),
            Self::Encode(error) => write!(f, "PNG 编码失败：{error}"),
            Self::Invalid(message) => write!(f, "图像数据不合法：{message}"),
        }
    }
}

impl std::error::Error for IoError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::File(error) => Some(error),
            Self::Decode(error) => Some(error),
            Self::Encode(error) => Some(error),
            Self::Invalid(_) => None,
        }
    }
}

impl From<std::io::Error> for IoError {
    fn from(error: std::io::Error) -> Self {
        Self::File(error)
    }
}
