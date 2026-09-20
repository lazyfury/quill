//! 目录扫描：把磁盘上的一个目录读成一份可排序、可格式化的清单。
//!
//! 这是 `file_browser` 的数据层，跟 UI 完全分开 —— 视图只认识 [`Listing`]，
//! 不认识 `std::fs`。窗口宿主在**工作线程**上跑 [`scan`]，主线程一帧都不等
//! 磁盘（跟 `deepseek_balance` 里网络请求的形状一样）。
//!
//! 时间戳一律按**上海时区**显示（UTC+8，中国无夏令时，固定偏移即可，不需要
//! tz 数据库）。

use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

/// 上海时区相对 UTC 的固定偏移（秒）。中国没有夏令时。
const SHANGHAI_OFFSET: i64 = 8 * 3600;

/// 清单里的一行。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// 文件名（不含路径）。
    pub name: String,
    pub is_dir: bool,
    /// 字节数；目录为 `0`（递归统计代价太高，也不该阻塞 UI）。
    pub size: u64,
    /// 修改时间，Unix 秒。拿不到时为 `None`。
    pub modified: Option<u64>,
}

/// 一次扫描的结果。
///
/// 读不到目录时 `entries` 为空、`error` 有值 —— 视图把错误显示在状态行里，
/// 而不是弹一个对话框。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Listing {
    pub path: PathBuf,
    pub entries: Vec<Entry>,
    pub error: Option<String>,
}

impl Listing {
    /// 一个读失败的清单。
    pub fn failed(path: impl Into<PathBuf>, error: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            entries: Vec::new(),
            error: Some(error.into()),
        }
    }

    /// 一个凭空造出来的清单（自检用，不碰磁盘）。
    pub fn fixture(path: impl Into<PathBuf>, entries: Vec<Entry>) -> Self {
        Self {
            path: path.into(),
            entries,
            error: None,
        }
    }

    /// 清单里的条目数。
    pub fn len(&self) -> usize {
        self.entries.len()
    }
}

/// 读一个目录。
///
/// `hidden` 为 `false` 时跳过点开头的文件（`.git`、`.DS_Store`…）。目录排在
/// 文件前面，各自按名字排序。
pub fn scan(path: &Path, hidden: bool) -> Listing {
    let Ok(dir) = fs::read_dir(path) else {
        return Listing::failed(path, read_error(path));
    };

    let mut entries = Vec::new();
    for child in dir.flatten() {
        let name = child.file_name().to_string_lossy().into_owned();
        if !hidden && name.starts_with('.') {
            continue;
        }
        // 权限不足 / 已被删除的条目跳过，不让一个坏条目毁掉整份清单。
        let Ok(meta) = child.metadata() else {
            continue;
        };
        let is_dir = meta.is_dir();
        let modified = meta
            .modified()
            .ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map(|duration| duration.as_secs());
        entries.push(Entry {
            name,
            is_dir,
            size: if is_dir { 0 } else { meta.len() },
            modified,
        });
    }
    entries.sort_by(order);

    Listing {
        path: path.to_path_buf(),
        entries,
        error: None,
    }
}

/// 目录在前，然后按名字（大小写不敏感，同形时再按字典序定胜负）。
fn order(a: &Entry, b: &Entry) -> std::cmp::Ordering {
    b.is_dir
        .cmp(&a.is_dir)
        .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        .then_with(|| a.name.cmp(&b.name))
}

fn read_error(path: &Path) -> String {
    match fs::metadata(path) {
        Ok(meta) if meta.is_dir() => format!("读不到目录：{}（权限不足）", path.display()),
        Ok(_) => format!("不是目录：{}", path.display()),
        Err(_) => format!("目录不存在：{}", path.display()),
    }
}

/// `path` 的上一级；已经是根时返回 `None`。
pub fn parent(path: &Path) -> Option<PathBuf> {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .map(|parent| parent.to_path_buf())
}

/// 目录名后面挂一个 `/`，文件则只显示名字 —— 列表里区分两者的最省事办法。
pub fn display_name(entry: &Entry) -> String {
    if entry.is_dir {
        format!("{}/", entry.name)
    } else {
        entry.name.clone()
    }
}

/// 字节数 -> `1.2 KB` 这样的人类尺寸；目录显示为破折号。
pub fn format_size(entry: &Entry) -> String {
    if entry.is_dir {
        return "—".to_string();
    }
    format_bytes(entry.size)
}

/// 字节数 -> `1.2 KB`。
pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{} {}", bytes, UNITS[unit])
    } else if value < 10.0 {
        format!("{:.1} {}", value, UNITS[unit])
    } else {
        format!("{} {}", value.round() as u64, UNITS[unit])
    }
}

/// Unix 秒 -> `2026-09-20 14:49`（上海时区）；拿不到时间时是破折号。
pub fn format_time(entry: &Entry) -> String {
    match entry.modified {
        Some(seconds) => format_timestamp(seconds),
        None => "—".to_string(),
    }
}

/// Unix 秒 -> `YYYY-MM-DD HH:MM`（UTC+8，固定偏移）。
pub fn format_timestamp(seconds: u64) -> String {
    let shifted = seconds as i64 + SHANGHAI_OFFSET;
    let days = shifted.div_euclid(86_400);
    let rest = shifted.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    let hour = rest / 3600;
    let minute = rest % 3600 / 60;
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}",
        year, month, day, hour, minute
    )
}

/// Howard Hinnant 的 days -> 年月日算法，避免为了显示时间拉进一个日期库。
fn civil_from_days(days: i64) -> (i64, u64, u64) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let day = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let month = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
                                                       // 1、2 月属于上一年的"年"计数。
    let year = if month <= 2 { year + 1 } else { year };
    (year, month as u64, day as u64)
}

/// 把路径压成短一些的显示形式：家目录写成 `~`。
pub fn display_path(path: &Path) -> String {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    if let Some(home) = home {
        if let Ok(rest) = path.strip_prefix(&home) {
            if rest.as_os_str().is_empty() {
                return "~".to_string();
            }
            return format!("~/{}", rest.display());
        }
    }
    path.display().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(name: &str) -> Entry {
        Entry {
            name: name.to_string(),
            is_dir: true,
            size: 0,
            modified: None,
        }
    }

    fn file(name: &str, size: u64) -> Entry {
        Entry {
            name: name.to_string(),
            is_dir: false,
            size,
            modified: None,
        }
    }

    #[test]
    fn directories_sort_before_files() {
        let mut entries = vec![file("zeta", 1), dir("alpha"), file("beta", 1)];
        entries.sort_by(order);
        assert_eq!(entries[0].name, "alpha");
        assert_eq!(entries[1].name, "beta");
        assert_eq!(entries[2].name, "zeta");
    }

    /// 大小写不敏感：macOS 的 Finder 也是这个顺序，跟"大写一律在前"比起来
    /// 更接近人找文件的直觉。
    #[test]
    fn names_sort_case_insensitively() {
        let mut entries = vec![file("Zebra", 1), file("apple", 1), file("Banana", 1)];
        entries.sort_by(order);
        let names: Vec<&str> = entries.iter().map(|entry| entry.name.as_str()).collect();
        assert_eq!(names, vec!["apple", "Banana", "Zebra"]);
    }

    #[test]
    fn ties_break_deterministically() {
        let mut entries = vec![file("abc", 1), file("ABC", 1)];
        entries.sort_by(order);
        let names: Vec<&str> = entries.iter().map(|entry| entry.name.as_str()).collect();
        assert_eq!(names, vec!["ABC", "abc"]);
    }

    #[test]
    fn byte_sizes_are_plain() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(1023), "1023 B");
    }

    #[test]
    fn larger_sizes_pick_a_unit() {
        assert_eq!(format_bytes(1024), "1.0 KB");
        assert_eq!(format_bytes(1536), "1.5 KB");
        assert_eq!(format_bytes(1024 * 1024), "1.0 MB");
        // 10 以下保留一位小数，10 以上取整 —— 文件浏览器里最省空间也最好扫。
        assert_eq!(format_bytes(3 * 1024 * 1024 * 1024), "3.0 GB");
        assert_eq!(format_bytes(12 * 1024 * 1024), "12 MB");
    }

    /// 目录不显示尺寸：递归统计一个 `node_modules` 要几秒钟，不该拦在 UI 上。
    #[test]
    fn directories_have_no_size() {
        assert_eq!(format_size(&dir("src")), "—");
    }

    #[test]
    fn directory_names_carry_a_slash() {
        assert_eq!(display_name(&dir("src")), "src/");
        assert_eq!(display_name(&file("main.rs", 10)), "main.rs");
    }

    // -- 时间戳 ----------------------------------------------------------
    //
    // 一律上海时区（UTC+8）。这几个值用 `TZ=Asia/Shanghai date -r <秒>` 核对过。

    #[test]
    fn the_epoch_is_new_years_eve_in_shanghai() {
        // Unix 0 = 1970-01-01 00:00 UTC = 1970-01-01 08:00 CST。
        assert_eq!(format_timestamp(0), "1970-01-01 08:00");
    }

    #[test]
    fn timestamps_are_shanghai_not_utc() {
        // 2026-09-20 06:49 UTC = 2026-09-20 14:49 CST。
        let seconds = 1_789_886_988;
        assert_eq!(format_timestamp(seconds), "2026-09-20 14:49");
    }

    #[test]
    fn midnight_rolls_over_in_shanghai() {
        // 2026-09-19 16:30 UTC = 2026-09-20 00:30 CST（跨日）。
        let seconds = 1_789_835_448;
        assert_eq!(format_timestamp(seconds), "2026-09-20 00:30");
    }

    #[test]
    fn a_missing_time_is_a_dash() {
        assert_eq!(format_time(&file("x", 1)), "—");
    }

    // -- 磁盘 ------------------------------------------------------------

    /// 扫一个临时目录：真实的文件系统，但内容完全可控（也完全可删）。
    fn scratch(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("quill_file_browser_{name}"));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("create scratch dir");
        path
    }

    #[test]
    fn a_real_directory_is_read_and_sorted() {
        let root = scratch("read");
        fs::create_dir(root.join("beta")).unwrap();
        fs::write(root.join("alpha.txt"), "hello").unwrap();
        fs::write(root.join(".hidden"), "x").unwrap();

        let listing = scan(&root, false);
        assert!(listing.error.is_none());
        let names: Vec<String> = listing.entries.iter().map(display_name).collect();
        assert_eq!(names, vec!["beta/", "alpha.txt"], "目录在前，点文件被跳过");

        let with_hidden = scan(&root, true);
        assert_eq!(with_hidden.len(), 3, "--all 把点文件也带回来");

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_missing_directory_reports_why() {
        let path = std::env::temp_dir().join("quill_file_browser_missing");
        let _ = fs::remove_dir_all(&path);
        let listing = scan(&path, false);
        assert!(listing.entries.is_empty());
        assert!(listing.error.is_some(), "读不到要说原因");
        assert!(listing.error.unwrap().contains("不存在"));
    }

    #[test]
    fn a_file_is_not_a_directory() {
        let root = scratch("plainfile");
        let path = root.join("file.txt");
        fs::write(&path, "x").unwrap();
        let listing = scan(&path, false);
        assert!(listing.error.unwrap().contains("不是目录"));
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn the_root_has_no_parent() {
        assert_eq!(parent(Path::new("/")), None);
        assert_eq!(parent(Path::new("/usr/local")), Some(PathBuf::from("/usr")));
    }

    /// 家目录压成 `~`：路径列通常只有 300px 宽，省下来的十几个字符很值钱。
    #[test]
    fn the_home_directory_shrinks_to_a_tilde() {
        let home = std::env::var("HOME").expect("HOME is set");
        assert_eq!(display_path(Path::new(&home)), "~");
        assert_eq!(
            display_path(&PathBuf::from(&home).join("Documents")),
            "~/Documents"
        );
    }
}
