//! 二进制 / 文本预览：把一个文件的前 [`PREVIEW_LIMIT`] 字节变成行。
//!
//! 这是 `file_browser` 右栏的数据层，跟目录扫描 [`crate::scan`] 的职责一样：
//! **读盘不发生在主线程上**。视图只说"我想看这个文件"
//! （[`crate::ui::Browser::take_preview_request`]），宿主起一个线程跑
//! [`Preview::read`]，结果送回来。
//!
//! ## 两种看法，一份字节
//!
//! [`PreviewMode`] 选的是**怎么看**这 64 KiB，不是读什么：
//!
//! - [`PreviewMode::Binary`]：hexdump，16 字节一行，偏移量 / 十六进制 / ascii。
//! - [`PreviewMode::Text`]：按换行切，行号 / 这一行的内容。
//!
//! 切换只换行的算法，**不重新读盘**，所以是瞬时的。
//!
//! ## 已知取舍
//!
//! 文本模式看不出缩进：`draw_ui` 的换行是按词排的，会把**前导空白折叠掉**
//! （`draw_ui::layout::text::wrap_hard_line`）。要保住缩进得让那一列的
//! `TextOptions.wrap` 关掉 —— 但 `List` 的列目前没有这个开关，所以记在
//! `docs/plan.md` 里，没为它动组件 API。
//!
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

/// 文本模式一行最多显示这么多字符。
///
/// 一个压缩过的 JS 就是一"行"，把 64 KiB 全画出来既看不出东西又慢，所以截断
/// 并在末尾点一个省略号 —— 文本模式是看结构，不是看全文。
pub const MAX_LINE_CHARS: usize = 400;

/// 右栏显示字节的两种方式。
///
/// 同一份 [`Preview::bytes`]，两种看法。切换不需要重新读盘，所以模式是视图的
/// 状态，不是请求的一部分。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum PreviewMode {
    /// hexdump：偏移量 / 十六进制 / ascii。看魔数、看结构。
    #[default]
    Binary,
    /// 文本：行号 / 内容。看配置文件、日志、代码。
    Text,
}

impl PreviewMode {
    /// 右栏两个切换按钮上写的字。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Binary => "二进制",
            Self::Text => "文本",
        }
    }

    /// 另一个模式（`T` 键和点击按钮都走它）。
    pub fn toggled(&self) -> Self {
        match self {
            Self::Binary => Self::Text,
            Self::Text => Self::Binary,
        }
    }
}

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
    /// 每一行文本在 `bytes` 里的起点。
    ///
    /// 读进来就算好一次，之后按行取是 O(1) —— 文本模式的列表只问"即将显示"
    /// 的那几行，不能每次都从头扫一遍 64 KiB。
    line_starts: Vec<usize>,
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
            line_starts: Vec::new(),
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
            line_starts: Vec::new(),
            error: Some(error.into()),
        }
    }

    /// 一个凭空造出来的预览（自检和单测用，不碰磁盘）。
    pub fn fixture(name: &str, bytes: Vec<u8>) -> Self {
        let line_starts = line_starts(&bytes);
        Self {
            path: PathBuf::from(name),
            name: name.to_string(),
            size: bytes.len() as u64,
            bytes,
            line_starts,
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

        let line_starts = line_starts(&bytes);
        Self {
            path: path.to_path_buf(),
            name,
            size,
            bytes,
            line_starts,
            error: None,
        }
    }

    /// hex dump 有多少行（[`PreviewMode::Binary`] 的行数）。
    pub fn rows(&self) -> usize {
        (self.bytes.len() + BYTES_PER_ROW - 1) / BYTES_PER_ROW
    }

    /// 文本模式有多少行（[`PreviewMode::Text`] 的行数）。
    pub fn text_rows(&self) -> usize {
        self.line_starts.len()
    }

    /// 当前模式下有多少行。列表只认这个数。
    pub fn rows_in(&self, mode: PreviewMode) -> usize {
        match mode {
            PreviewMode::Binary => self.rows(),
            PreviewMode::Text => self.text_rows(),
        }
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

    /// 文本模式的第 `index` 行：不含换行符（`\r\n` 的两个字符都去掉）。
    pub fn text_line(&self, index: usize) -> &[u8] {
        let Some(start) = self.line_starts.get(index).copied() else {
            return &[];
        };
        let end = self
            .line_starts
            .get(index + 1)
            .copied()
            .unwrap_or(self.bytes.len());
        let mut end = end;
        // 行尾的换行符属于"分隔"，不属于这一行 —— 留着会多画一个 `·`。
        while end > start && (self.bytes[end - 1] == b'\n' || self.bytes[end - 1] == b'\r') {
            end -= 1;
        }
        &self.bytes[start..end]
    }

    /// 当前模式下第 `index` 行的列内容。列表的 `source` 闭包问的就是这个。
    pub fn row_cells_in(&self, mode: PreviewMode, index: usize) -> Vec<String> {
        match mode {
            PreviewMode::Binary => self.binary_row_cells(index),
            PreviewMode::Text => self.text_row_cells(index),
        }
    }

    /// 二进制模式的一行：偏移量 / 十六进制 / ascii。
    pub fn binary_row_cells(&self, index: usize) -> Vec<String> {
        let row = self.row(index);
        if row.is_empty() {
            return Vec::new();
        }
        vec![format_offset(index), format_hex(row), format_ascii(row)]
    }

    /// 文本模式的一行：行号 / 内容。
    pub fn text_row_cells(&self, index: usize) -> Vec<String> {
        if index >= self.text_rows() {
            return Vec::new();
        }
        vec![
            format_line_number(index),
            format_text_line(self.text_line(index)),
        ]
    }

    /// 字节被截断了（文件比 [`PREVIEW_LIMIT`] 大）。
    pub fn truncated(&self) -> bool {
        self.size > self.bytes.len() as u64
    }

    /// 读到的字节里有 NUL。
    ///
    /// 文本模式看这种文件只会是乱码，副标题里说一句，别让人以为是预览坏了。
    pub fn looks_binary(&self) -> bool {
        self.bytes.iter().any(|byte| *byte == 0)
    }

    /// 右栏标题：文件名 + 磁盘上的大小。
    pub fn headline(&self) -> String {
        if self.name.is_empty() {
            return "预览".to_string();
        }
        format!("{} · {}", self.name, crate::scan::format_bytes(self.size))
    }

    /// 标题下面那一行：读了什么、怎么在读、或者为什么没读到。
    pub fn detail(&self, mode: PreviewMode) -> String {
        if let Some(error) = self.error.as_deref() {
            return error.to_string();
        }
        if self.name.is_empty() {
            return "选中一个文件，这里显示它的前 64 KiB".to_string();
        }
        let mut out = format!(
            "{} · {} · {} 行",
            mode.label(),
            crate::scan::format_bytes(self.bytes.len() as u64),
            self.rows_in(mode)
        );
        if self.truncated() {
            out.push_str(&format!(
                "（共 {}，已截断）",
                crate::scan::format_bytes(self.size)
            ));
        }
        if mode == PreviewMode::Text && self.looks_binary() {
            out.push_str(" · 看着像二进制");
        }
        out
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

/// 文本模式的行号，从 1 开始。
///
/// **不补前导空格**：`draw_ui` 的换行会把前导空白折叠掉（按词排版），补了也
/// 白补。对齐靠列的定宽，不靠空格。
pub fn format_line_number(index: usize) -> String {
    format!("{}", index + 1)
}

/// 文本模式的一行内容。
///
/// 制表符摊成四个空格（不然后面的列会跟着内容跳），控制字符（含 `\r`）画成
/// `·` —— 它们没有字形，留着会让排版错位。超长行截到 [`MAX_LINE_CHARS`]。
pub fn format_text_line(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    let mut out = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '\t' => out.push_str("    "),
            character if character.is_control() => out.push('·'),
            character => out.push(character),
        }
    }
    if out.chars().count() > MAX_LINE_CHARS {
        let kept: String = out.chars().take(MAX_LINE_CHARS).collect();
        return format!("{kept}…");
    }
    out
}

/// 每一行文本的起点。
///
/// 末尾那个换行不产生一个空行：`"a\n"` 是一行，不是两行 —— 不然每个以换行
/// 结尾的 Unix 文件末尾都会多出一行空白。
fn line_starts(bytes: &[u8]) -> Vec<usize> {
    if bytes.is_empty() {
        return Vec::new();
    }
    let mut starts = vec![0usize];
    for (index, byte) in bytes.iter().enumerate() {
        if *byte == b'\n' && index + 1 < bytes.len() {
            starts.push(index + 1);
        }
    }
    starts
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bytes(text: &str) -> Vec<u8> {
        text.as_bytes().to_vec()
    }

    #[test]
    fn a_row_is_offset_hex_and_ascii() {
        let preview = Preview::fixture("hi", bytes("Hello, world!\n"));
        let cells = preview.binary_row_cells(0);
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
        assert_eq!(preview.text_rows(), 0);
        assert_eq!(preview.binary_row_cells(0), Vec::<String>::new());
        assert_eq!(preview.text_row_cells(0), Vec::<String>::new());
        assert_eq!(preview.headline(), "预览");
        assert!(preview.detail(PreviewMode::Binary).contains("选中一个文件"));
    }

    #[test]
    fn a_truncated_preview_says_so() {
        let mut preview = Preview::fixture("big", vec![0u8; PREVIEW_LIMIT]);
        preview.size = 4 * 1024 * 1024 * 1024;
        assert!(preview.truncated());
        assert_eq!(preview.rows(), PREVIEW_LIMIT / BYTES_PER_ROW);
        let detail = preview.detail(PreviewMode::Binary);
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
        assert_eq!(preview.text_rows(), 0);
    }

    // -- 文本模式 --------------------------------------------------------

    #[test]
    fn text_mode_splits_on_newlines() {
        let preview = Preview::fixture("a.txt", bytes("one\ntwo\nthree\n"));
        assert_eq!(preview.text_rows(), 3, "末尾的换行不产生一个空行");
        let cells = preview.text_row_cells(1);
        assert_eq!(cells[0], "2");
        assert_eq!(cells[1], "two");
        assert_eq!(preview.rows_in(PreviewMode::Text), 3);
        // 同一份字节，二进制模式只有一行。
        assert_eq!(preview.rows_in(PreviewMode::Binary), 1);
    }

    #[test]
    fn a_trailing_newline_does_not_make_an_empty_last_line() {
        assert_eq!(Preview::fixture("a", bytes("only\n")).text_rows(), 1);
        assert_eq!(Preview::fixture("a", bytes("one\ntwo")).text_rows(), 2);
        assert_eq!(Preview::fixture("a", Vec::new()).text_rows(), 0);
    }

    /// `\r\n` 的文件不该每行末尾多一个点。
    #[test]
    fn windows_line_endings_are_trimmed() {
        let preview = Preview::fixture("a.txt", bytes("one\r\ntwo\r\n"));
        assert_eq!(preview.text_rows(), 2);
        assert_eq!(preview.text_line(0), b"one");
        assert_eq!(preview.text_row_cells(0)[1], "one");
    }

    /// 没有换行符就是一行 —— 一个压缩过的 JS 就是这种，所以有长度上限。
    #[test]
    fn a_file_without_newlines_is_one_long_line() {
        let preview = Preview::fixture("min.js", vec![b'x'; MAX_LINE_CHARS + 50]);
        assert_eq!(preview.text_rows(), 1);
        let line = &preview.text_row_cells(0)[1];
        assert!(line.ends_with('…'), "超长行要截住：{}", line.len());
        assert!(line.chars().count() <= MAX_LINE_CHARS + 1);
    }

    #[test]
    fn tabs_and_control_characters_are_renderable() {
        assert_eq!(format_text_line(b"a\tb"), "a    b");
        assert_eq!(format_text_line(b"a\x00b"), "a·b");
        assert_eq!(format_text_line(b"a\r"), "a·");
    }

    /// 文本模式看二进制文件只会是乱码 —— 副标题得说清楚，别让人以为预览坏了。
    #[test]
    fn text_mode_warns_about_binary_files() {
        let preview = Preview::fixture("blob.bin", vec![0x41, 0x00, 0x42]);
        assert!(preview.looks_binary());
        let detail = preview.detail(PreviewMode::Text);
        assert!(detail.contains("看着像二进制"), "{detail}");
        assert!(
            !preview.detail(PreviewMode::Binary).contains("看着像二进制"),
            "二进制模式不用提醒"
        );
    }

    #[test]
    fn the_mode_toggles_between_the_two() {
        assert_eq!(PreviewMode::Binary.toggled(), PreviewMode::Text);
        assert_eq!(PreviewMode::Text.toggled(), PreviewMode::Binary);
        assert_eq!(PreviewMode::default(), PreviewMode::Binary);
        assert_eq!(PreviewMode::Binary.label(), "二进制");
        assert_eq!(PreviewMode::Text.label(), "文本");
    }
}
