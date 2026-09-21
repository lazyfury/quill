//! A tiny user settings file: which tab was open when the app last closed.
//!
//! One JSON object in the user's config directory. Everything here is
//! best-effort: a missing or unreadable file is simply "no preference", and a
//! write that fails is dropped, because losing a remembered tab is never worth a
//! crash. The path can be overridden with `DEEPSEEK_BALANCE_SETTINGS`, which the
//! tests use so they never touch the real file.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::ui::Tab;

/// The on-disk shape. `serde(default)` so a future field can be added without
/// invalidating an old file.
#[derive(Debug, Default, Serialize, Deserialize)]
struct Settings {
    #[serde(default)]
    tab: u8,
}

/// The settings file's path: `$DEEPSEEK_BALANCE_SETTINGS`, else
/// `$XDG_CONFIG_HOME/deepseek_balance/settings.json`, else
/// `~/.config/deepseek_balance/settings.json`.
pub fn path() -> Option<PathBuf> {
    if let Some(override_path) = std::env::var_os("DEEPSEEK_BALANCE_SETTINGS") {
        return Some(PathBuf::from(override_path));
    }
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;
    Some(base.join("deepseek_balance").join("settings.json"))
}

/// Reads the remembered tab from `path`, `None` when there is nothing usable.
pub fn load_from(path: &Path) -> Option<Tab> {
    let text = std::fs::read_to_string(path).ok()?;
    let settings: Settings = serde_json::from_str(&text).ok()?;
    Tab::from_index(settings.tab)
}

/// Writes the tab to `path`, creating the directory if needed.
pub fn save_to(path: &Path, tab: Tab) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let settings = Settings { tab: tab.index() };
    let text = serde_json::to_string_pretty(&settings)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
    std::fs::write(path, text)
}

/// [`load_from`] at the configured [`path`].
pub fn load() -> Option<Tab> {
    load_from(&path()?)
}

/// [`save_to`] at the configured [`path`], best-effort.
pub fn save(tab: Tab) {
    if let Some(path) = path() {
        let _ = save_to(&path, tab);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A per-test path under the temp dir, removed afterwards.
    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "deepseek_balance_{name}_{}.json",
            std::process::id()
        ))
    }

    #[test]
    fn a_saved_tab_round_trips() {
        let path = temp_path("round_trip");
        save_to(&path, Tab::Go).expect("write");
        assert_eq!(load_from(&path), Some(Tab::Go));

        save_to(&path, Tab::DeepSeek).expect("write");
        assert_eq!(load_from(&path), Some(Tab::DeepSeek));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_missing_file_is_no_preference() {
        let path = temp_path("missing");
        let _ = std::fs::remove_file(&path);
        assert_eq!(load_from(&path), None);
    }

    #[test]
    fn a_corrupt_file_is_no_preference() {
        let path = temp_path("corrupt");
        std::fs::write(&path, "not json").expect("write");
        assert_eq!(load_from(&path), None);

        // An unknown tab index is dropped too, rather than opening a bad page.
        std::fs::write(&path, r#"{"tab":9}"#).expect("write");
        assert_eq!(load_from(&path), None);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn the_tab_index_round_trips() {
        for tab in [Tab::DeepSeek, Tab::Go] {
            assert_eq!(Tab::from_index(tab.index()), Some(tab));
        }
        assert_eq!(Tab::from_index(2), None);
    }

    /// The override variable wins, which is what keeps the tests off the real
    /// user file.
    #[test]
    fn the_path_honours_the_override() {
        // SAFETY: single-threaded assertion about an env var this test owns.
        std::env::set_var("DEEPSEEK_BALANCE_SETTINGS", "/tmp/quill-test.json");
        assert_eq!(path(), Some(PathBuf::from("/tmp/quill-test.json")));
        std::env::remove_var("DEEPSEEK_BALANCE_SETTINGS");
    }
}
