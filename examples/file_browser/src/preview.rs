//! 二进制预览：把一个文件的前 [`PREVIEW_LIMIT`] 字节读成 hex dump 的行。
//!
//! 这是 `file_browser` 右栏的数据层，跟目录扫描 [`crate::scan`] 的职责一样：
//! **读盘不发生在主线程上**。视图只说"我想看这个文件"
//! （[`crate::ui::Browser::take_preview_request`]），宿主起一个线程跑
//! [`Preview::read`]，结果送回来。
//!
//! ## 为什么只读前 64 KiB
//!
//! 一个 4 GB 的镜像不该把 4 GB 读进内存，也没人能看完 2.7 亿行 hex。预览是
//! **看文件是什么**（魔数、头、是不是文本），不是看完它 —— 所以只取开头，
//! 并在标题上写清楚"已截断"。真要全看是别的工具的事。
//!
//! ## 行是"算"出来的，不是"存"出来的
//!
//! [`Preview`] 只存字节；一行显示什么由 [`row_cells`] 在列表问的时候算。列表
//! 只会问**即将显示的那几行**，所以 64 KiB（4096 行）跟 1 KiB（64 行）在树上
//! 的代价一样 —— 虚拟化的好处在这里再显一次。

use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

/// 一行放几个字节。16 是 hexdump 的惯例：一行正好能一眼扫完，且偏移量是整
/// 十倍的十六进制。
pub const BYTES_PER_ROW: usize = 16;

/// 最多读这么多字节。
pub const PREVIEW_LIMIT: usize = 64 * 1024;

/// 一次预览的结果。
///
/// `error` 有值时 `bytes` 为空：目录、权限不足、打不开都走这条路，视图把它
/// 显示在右栏标题下面，而不是弹对话框。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Preview {
    /// 被预览的文件；`empty()` 时为空路径。
    pub path: PathBuf,
    /// 文件名（不含目录）。
    pub name: String,
    /// 文件在磁盘上的完整大小（可能远大于 `bytes.len()`）。
    pub size: u64,
    /// 读到的字节，最多 [`PREVIEW_LIMIT`] 个。
    pub bytes: Vec<u8>,
    pub error: Option<String>,
}

impl Preview {
    /// 没有选中文件时的预览：右栏显示一句提示，一行 hex 都没有。
    pub fn empty() -> Self {
        Self {
            path: PathBuf::new(),
            name: String::new(),
            size: 0,
            bytes: Vec::new(),
            error: None,
        }
    }

    /// 一个读失败的预览。
    pub fn failed(path: impl Into<PathBuf>, error: impl Into<String>) -> Self {
        let path = path.into();
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        Self {
            path,
            name,
            size: 0,
            bytes: Vec::new(),
            error: Some(error.into()),
        }
    }

    /// 一个凭空造出来的预览（自检和单测用，不碰磁盘）。
    pub fn fixture(name: &str, bytes: Vec<u8>) -> Self {
        Self {
            path: PathBuf::from(name),
            name: name.to_string(),
            size: bytes.len() as u64,
            bytes,
            error: None,
        }
    }

    /// 读一个文件的前 [`PREVIEW_LIMIT`] 字节。
    ///
    /// 目录直接判失败 —— 它的"内容"是清单，不是字节。
    pub fn read(path: &Path) -> Self {
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();

        let meta = match std::fs::metadata(path) {
            Ok(meta) => meta,
            Err(error) => return Self::failed(path, format!("打不开：{error}")),
        };
        if meta.is_dir() {
            return Self::failed(path, "是一个目录");
        }
        let size = meta.len();

        // `take` 让读盘在 64 KiB 处停住：一个 4 GB 的文件也只读 64 KiB。
        let mut bytes = Vec::new();
        let outcome = File::open(path).and_then(|file| {
            let mut limited = file.take(PREVIEW_LIMIT as u64);
            limited.read_to_end(&mut bytes)
        });
        if let Err(error) = outcome {
            return Self::failed(path, format!("读失败：{error}"));
        }

        Self {
            path: path.to_path_buf(),
            name,
            size,
            bytes,
            error: None,
        }
    }

    /// hex dump 有多少行。
    pub fn rows(&self) -> usize {
        (self.bytes.len() + BYTES_PER_ROW - 1) / BYTES_PER_ROW
    }

    /// 第 `index` 行的字节（最后一行可能不满 [`BYTES_PER_ROW`]）。
    pub fn row(&self, index: usize) -> &[u8] {
        let start = index * BYTES_PER_ROW;
        if start >= self.bytes.len() {
            return &[];
        }
        let end = (start + BYTES_PER_ROW).min(self.bytes.len());
        &self.bytes[start..end]
    }

    /// 字节被截断了（文件比 [`PREVIEW_LIMIT`] 大）。
    pub fn truncated(&self) -> bool {
        self.size > self.bytes.len() as u64
    }

    /// 右栏标题：文件名 + 磁盘上的大小。
    pub fn headline(&self) -> String {
        if self.name.is_empty() {
            return "二进制预览".to_string();
        }
        format!("{} · {}", self.name, crate::scan::format_bytes(self.size))
    }

    /// 标题下面那一行：读了什么、或者为什么没读到。
    pub fn detail(&self) -> String {
        if let Some(error) = self.error.as_deref() {
            return error.to_string();
        }
        if self.name.is_empty() {
            return "选中一个文件，这里显示它的前 64 KiB".to_string();
        }
        let read = crate::scan::format_bytes(self.bytes.len() as u64);
        if self.truncated() {
            return format!(
                "前 {}（共 {}，已截断） · {} 行",
                read,
                crate::scan::format_bytes(self.size),
                self.rows()
            );
        }
        format!("{} · {} 行", read, self.rows())
    }
}

/// 行首的偏移量：8 位十六进制，跟 hexdump 一致。
pub fn format_offset(index: usize) -> String {
    format!("{:08x}", index * BYTES_PER_ROW)
}

/// 一行的十六进制部分。
///
/// 第 8 个字节后多一个空格（hexdump 的分组），**不满一行时补空格** —— 不补的
/// 话最后一行的 ascii 列会往左飘，跟上面几行对不齐。
pub fn format_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(BYTES_PER_ROW * 3 + 1);
    for (index, byte) in bytes.iter().enumerate() {
        if index == BYTES_PER_ROW / 2 {
            out.push(' ');
        }
        out.push_str(&format!("{byte:02x}"));
        out.push(' ');
    }
    // 补到满行的宽度：每个字节占 3 个字符，中间那组多一个。
    let filled = bytes.len() * 3 + usize::from(bytes.len() > BYTES_PER_ROW / 2);
    let full = BYTES_PER_ROW * 3 + 1;
    for _ in filled..full {
        out.push(' ');
    }
    out
}

/// 一行的 ascii 部分：不可打印的字节画成 `.`。
pub fn format_ascii(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|byte| {
            if is_printable(*byte) {
                *byte as char
            } else {
                '.'
            }
        })
        .collect()
}

fn is_printable(byte: u8) -> bool {
    (0x20..=0x7e).contains(&byte)
}

/// 列表要的一行三列：偏移量、十六进制、ascii。
///
/// 只有**即将显示**的行会走到这里，所以这里的分配是每帧几十次，不是几千次。
pub fn row_cells(bytes: &[u8], index: usize) -> Vec<String> {
    if bytes.is_empty() {
        return Vec::new();
    }
    vec![format_offset(index), format_hex(bytes), format_ascii(bytes)]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bytes(text: &str) -> Vec<u8> {
        text.as_bytes().to_vec()
    }

    #[test]
    fn a_row_is_offset_hex_and_ascii() {
        let row = bytes("Hello, world!\n");
        let cells = row_cells(&row, 0);
        assert_eq!(cells[0], "00000000");
        assert_eq!(
            cells[1].trim_end(),
            "48 65 6c 6c 6f 2c 20 77  6f 72 6c 64 21 0a"
        );
        assert_eq!(cells[2], "Hello, world!.");
    }

    #[test]
    fn the_offset_counts_in_hex() {
        assert_eq!(format_offset(0), "00000000");
        assert_eq!(format_offset(1), "00000010");
        assert_eq!(format_offset(16), "00000100");
        assert_eq!(format_offset(0x1234), "00012340");
    }

    #[test]
    fn unprintable_bytes_become_dots() {
        assert_eq!(format_ascii(&[0x00, 0x41, 0x7f, 0x20, 0x7e]), ".A. ~");
    }

    /// 不满一行也要补到满行宽，否则最后一行的 ascii 列会往左飘。
    #[test]
    fn a_short_row_is_padded_to_the_full_width() {
        let full = format_hex(&[0xab; BYTES_PER_ROW]);
        let short = format_hex(&[0xab; 3]);
        assert_eq!(full.len(), short.len(), "补空格后两行等宽");
        assert_eq!(full.len(), BYTES_PER_ROW * 3 + 1);
        assert!(short.starts_with("ab ab ab"));
    }

    #[test]
    fn rows_round_up_and_the_last_one_is_short() {
        let preview = Preview::fixture("bin", vec![7u8; BYTES_PER_ROW * 2 + 5]);
        assert_eq!(preview.rows(), 3);
        assert_eq!(preview.row(0).len(), BYTES_PER_ROW);
        assert_eq!(preview.row(2).len(), 5);
        assert_eq!(preview.row(3).len(), 0, "越界的行是空的");
        assert!(!preview.truncated());
    }

    #[test]
    fn an_empty_preview_has_no_rows() {
        let preview = Preview::empty();
        assert_eq!(preview.rows(), 0);
        assert_eq!(row_cells(&[], 0), Vec::<String>::new());
        assert_eq!(preview.headline(), "二进制预览");
        assert!(preview.detail().contains("选中一个文件"));
    }

    #[test]
    fn a_truncated_preview_says_so() {
        let mut preview = Preview::fixture("big", vec![0u8; PREVIEW_LIMIT]);
        preview.size = 4 * 1024 * 1024 * 1024;
        assert!(preview.truncated());
        assert_eq!(preview.rows(), PREVIEW_LIMIT / BYTES_PER_ROW);
        let detail = preview.detail();
        assert!(detail.contains("已截断"), "{detail}");
        assert!(detail.contains("GB"), "{detail}");
        assert!(detail.contains("64"), "读了 64 KiB：{detail}");
    }

    /// 真读一次盘：一个临时文件。
    #[test]
    fn reading_a_real_file_stops_at_the_limit() {
        let dir = std::env::temp_dir().join("quill-preview-test");
        std::fs::create_dir_all(&dir).expect("mkdir");
        let path = dir.join("sample.bin");
        // 比 PREVIEW_LIMIT 多写一点，验证读取确实在限制处停住。
        std::fs::write(&path, vec![0x5au8; PREVIEW_LIMIT + 512]).expect("write");

        let preview = Preview::read(&path);
        assert_eq!(preview.error, None);
        assert_eq!(preview.name, "sample.bin");
        assert_eq!(preview.bytes.len(), PREVIEW_LIMIT);
        assert!(preview.truncated());
        assert_eq!(preview.row(0), vec![0x5au8; BYTES_PER_ROW]);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_directory_is_not_previewable() {
        let preview = Preview::read(Path::new("/tmp"));
        assert_eq!(preview.error.as_deref(), Some("是一个目录"));
        assert_eq!(preview.rows(), 0);
    }

    #[test]
    fn a_missing_file_reports_why() {
        let preview = Preview::read(Path::new("/definitely/not/here.bin"));
        assert!(preview.error.is_some());
        assert_eq!(preview.rows(), 0);
    }
}
