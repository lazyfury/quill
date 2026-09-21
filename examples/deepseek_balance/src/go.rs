//! The OpenCode Go usage endpoint: request, response model and key lookup.
//!
//! OpenCode Go's plan exposes its quota as *percent used* per window, not as
//! currency: `GET https://opencode.ai/zen/go/v1/usage` (Bearer key) answers
//! `{ "usage": { "rolling", "weekly", "monthly" } }`, and each window carries a
//! `status`, a `percent` and a `resetsAt` stamp. The view shows the remainder
//! (`100 - percent`) and the time to reset.
//!
//! Like [`crate::api`], the network call is a plain blocking function so the
//! host can run it on a worker thread; the headless UI tests never touch it.

use std::path::PathBuf;
use std::time::Duration;

use deepseek_util::time::parse_rfc3339_utc;
use serde::Deserialize;

use crate::api::{error_detail, non_blank};

/// Usage endpoint used when `OPENCODE_GO_USAGE_URL` is not set.
const DEFAULT_USAGE_URL: &str = "https://opencode.ai/zen/go/v1/usage";

/// The error [`resolve_api_key`] reports when no key is configured anywhere.
pub const MISSING_KEY_MESSAGE: &str =
    "未配置 OpenCode Go 密钥（~/.local/share/opencode/auth.json 或 OPENCODE_GO_API_KEY）";

/// A usage reply: one window per quota period.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct GoUsage {
    pub usage: Usage,
}

/// The three plan windows. Each is optional so a partial reply still parses.
#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
pub struct Usage {
    #[serde(default)]
    pub rolling: Option<UsageWindow>,
    #[serde(default)]
    pub weekly: Option<UsageWindow>,
    #[serde(default)]
    pub monthly: Option<UsageWindow>,
}

/// One quota window: how much is gone, its status and when it resets.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct UsageWindow {
    /// `"ok"` while the window is healthy; anything else means it is spent.
    #[serde(default)]
    pub status: String,
    /// Percent *used*, 0–100.
    #[serde(default)]
    pub percent: f64,
    /// RFC 3339 instant the window resets, as returned by the API.
    #[serde(default, rename = "resetsAt")]
    pub resets_at: Option<String>,
}

/// Which plan window a row is about.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WindowKind {
    /// The rolling five-hour window.
    Rolling,
    /// The week window.
    Weekly,
    /// The billing-cycle window.
    Monthly,
}

impl WindowKind {
    /// Every window, in display order (shortest first).
    pub const ALL: [WindowKind; 3] = [Self::Rolling, Self::Weekly, Self::Monthly];

    /// The row label.
    pub fn label(self) -> &'static str {
        match self {
            Self::Rolling => "5 小时",
            Self::Weekly => "本周",
            Self::Monthly => "本月",
        }
    }

    /// The matching window in `usage`, if the reply carried it.
    pub fn of(self, usage: &Usage) -> Option<&UsageWindow> {
        match self {
            Self::Rolling => usage.rolling.as_ref(),
            Self::Weekly => usage.weekly.as_ref(),
            Self::Monthly => usage.monthly.as_ref(),
        }
    }
}

impl UsageWindow {
    /// Percent left, clamped so a sloppy `percent` cannot paint `剩余 105%` or
    /// a negative.
    pub fn remaining_percent(&self) -> f64 {
        (100.0 - self.percent).clamp(0.0, 100.0)
    }

    /// Whether the window is spent: a non-`ok` status, or nothing left.
    pub fn is_exhausted(&self) -> bool {
        !self.status.eq_ignore_ascii_case("ok") || self.remaining_percent() <= 0.0
    }

    /// The value cell: `剩余 88%` (or `已用尽`).
    pub fn remaining_label(&self) -> String {
        if self.is_exhausted() {
            "已用尽".to_string()
        } else {
            format!("剩余 {:.0}%", self.remaining_percent())
        }
    }

    /// Time until `resetsAt` relative to `now_epoch`, if the stamp parses and is
    /// still ahead. `None` means "no usable reset time", which the view renders
    /// by leaving the cell empty.
    pub fn reset_in(&self, now_epoch: i64) -> Option<Duration> {
        let at = parse_rfc3339_utc(self.resets_at.as_deref()?)?;
        let seconds = at - now_epoch;
        (seconds > 0).then(|| Duration::from_secs(seconds as u64))
    }
}

impl GoUsage {
    /// Parses a response body.
    pub fn parse(body: &str) -> Result<Self, String> {
        serde_json::from_str(body).map_err(|error| format!("解析响应失败: {error}"))
    }

    /// A single line for the menu bar and the badge, e.g. `Go 88%`.
    ///
    /// The rolling window leads, like the panel's first row: it is the one that
    /// throttles day to day. A missing window still gets a placeholder rather
    /// than an empty title.
    pub fn headline(&self) -> String {
        match self.usage.rolling.as_ref() {
            Some(window) if window.is_exhausted() => "Go 已用尽".to_string(),
            Some(window) => format!("Go {:.0}%", window.remaining_percent()),
            None => "Go —".to_string(),
        }
    }
}

/// Usage endpoint, overridable with `OPENCODE_GO_USAGE_URL`.
pub fn endpoint() -> String {
    std::env::var("OPENCODE_GO_USAGE_URL").unwrap_or_else(|_| DEFAULT_USAGE_URL.to_string())
}

/// Candidate locations of OpenCode's `auth.json`, most specific first.
///
/// The key lives under the `opencode-go` entry. `OPENCODE_AUTH_JSON` overrides
/// the path outright (the tests use it); otherwise the XDG data dir is tried,
/// then the two defaults opencode uses on Linux (`~/.local/share`) and macOS
/// (`~/Library/Application Support`).
fn auth_json_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Ok(path) = std::env::var("OPENCODE_AUTH_JSON") {
        paths.push(PathBuf::from(path));
    }
    if let Ok(xdg) = std::env::var("XDG_DATA_HOME") {
        paths.push(PathBuf::from(xdg).join("opencode").join("auth.json"));
    }
    if let Ok(home) = std::env::var("HOME") {
        paths.push(
            PathBuf::from(&home)
                .join(".local")
                .join("share")
                .join("opencode")
                .join("auth.json"),
        );
        paths.push(
            PathBuf::from(&home)
                .join("Library")
                .join("Application Support")
                .join("opencode")
                .join("auth.json"),
        );
    }
    paths
}

/// The `opencode-go` key from the first readable `auth.json`, `None` when no
/// file has one.
pub fn api_key_from_auth_json() -> Option<String> {
    for path in auth_json_paths() {
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
            continue;
        };
        let key = value
            .get("opencode-go")
            .and_then(|entry| entry.get("key"))
            .and_then(|key| key.as_str());
        if let Some(key) = non_blank(key.map(str::to_string)) {
            return Some(key);
        }
    }
    None
}

/// Key from `auth.json` first, then `OPENCODE_GO_API_KEY`, `None` when neither
/// has one.
pub fn api_key() -> Option<String> {
    api_key_from_auth_json().or_else(|| non_blank(std::env::var("OPENCODE_GO_API_KEY").ok()))
}

/// Re-reads `OPENCODE_GO_API_KEY` from the user's login + interactive shell.
///
/// Same reason as [`crate::api::api_key_from_shell`]: a packaged app inherits no
/// shell environment, and a running process cannot see a variable set after it
/// started. `auth.json` is the common case, so this is the last resort.
pub fn api_key_from_shell() -> Option<String> {
    const MARKER: &str = "__QUILL_GO_KEY__";
    let shell = std::env::var("SHELL").ok()?;
    let output = std::process::Command::new(shell)
        .args([
            "-lic",
            &format!("printf %s \"{MARKER}${{OPENCODE_GO_API_KEY}}\""),
        ])
        .stdin(std::process::Stdio::null())
        .output()
        .ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    non_blank(stdout.rsplit(MARKER).next().map(String::from))
}

/// The key to fetch with: `auth.json`, then the process env, then the login
/// shell. Without a key anywhere this is the error the view shows verbatim.
pub fn resolve_api_key() -> Result<String, String> {
    api_key()
        .or_else(api_key_from_shell)
        .ok_or_else(|| MISSING_KEY_MESSAGE.to_string())
}

/// Queries the usage endpoint. Blocking; call it off the UI thread.
pub fn fetch(endpoint: &str, api_key: &str) -> Result<GoUsage, String> {
    let response = ureq::get(endpoint)
        .set("Authorization", &format!("Bearer {api_key}"))
        .set("Accept", "application/json")
        .call();

    match response {
        Ok(response) => {
            let body = response
                .into_string()
                .map_err(|error| format!("读取响应失败: {error}"))?;
            GoUsage::parse(&body)
        }
        Err(ureq::Error::Status(code, response)) => {
            let body = response.into_string().unwrap_or_default();
            // The two failures a Go user actually hits deserve a sentence, not
            // just the number: a rejected key, and a key with no Go plan. The
            // endpoint joins the plan, so "no subscription" arrives as 401/403.
            match code {
                401 => Err("OpenCode Go 密钥被拒绝（401）".to_string()),
                403 => Err("该密钥没有 OpenCode Go 订阅（403）".to_string()),
                _ => Err(format!("HTTP {code}: {}", error_detail(&body))),
            }
        }
        Err(error) => Err(format!("请求失败: {error}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
        "usage": {
            "rolling": { "status": "ok", "percent": 12.5, "resetsAt": "2026-09-21T12:00:00.000Z" },
            "weekly":  { "status": "ok", "percent": 40,   "resetsAt": "2026-09-28T00:00:00Z" },
            "monthly": { "status": "exhausted", "percent": 100, "resetsAt": "2026-10-01T00:00:00Z" }
        }
    }"#;

    #[test]
    fn parses_a_usage_reply() {
        let usage = GoUsage::parse(SAMPLE).expect("valid reply");
        assert_eq!(usage.usage.rolling.as_ref().map(|w| w.percent), Some(12.5));
        assert_eq!(usage.usage.weekly.as_ref().map(|w| w.percent), Some(40.0));
        assert_eq!(usage.usage.monthly.as_ref().map(|w| w.percent), Some(100.0));
    }

    #[test]
    fn reports_a_bad_reply() {
        let error = GoUsage::parse("not json").expect_err("invalid body");
        assert!(error.contains("解析响应失败"), "{error}");
    }

    /// A reply missing a window still parses; that row is simply left empty.
    #[test]
    fn a_partial_reply_parses() {
        let usage = GoUsage::parse(r#"{"usage":{"rolling":{"percent":5}}}"#).expect("valid reply");
        assert!(usage.usage.rolling.is_some());
        assert!(usage.usage.weekly.is_none());
        assert!(usage.usage.monthly.is_none());
    }

    /// `remaining_percent` is the complement of `percent`, clamped so a sloppy
    /// reply cannot paint `剩余 105%` or a negative; a non-`ok` status means
    /// spent even when the arithmetic says otherwise.
    #[test]
    fn remaining_and_exhaustion() {
        let window = |status: &str, percent| UsageWindow {
            status: status.to_string(),
            percent,
            resets_at: None,
        };

        let healthy = window("ok", 12.5);
        assert_eq!(healthy.remaining_percent(), 87.5);
        assert_eq!(healthy.remaining_label(), "剩余 88%");
        assert!(!healthy.is_exhausted());

        assert_eq!(window("ok", 105.0).remaining_percent(), 0.0);
        assert!(window("ok", 105.0).is_exhausted(), "nothing left is spent");
        assert_eq!(window("ok", -5.0).remaining_percent(), 100.0);

        let spent = window("rate_limited", 10.0);
        assert!(spent.is_exhausted());
        assert_eq!(spent.remaining_label(), "已用尽");
    }

    #[test]
    fn window_kinds_pick_their_window() {
        let usage = GoUsage::parse(SAMPLE).expect("valid reply").usage;
        assert_eq!(
            WindowKind::Rolling.of(&usage).map(|w| w.percent),
            Some(12.5)
        );
        assert_eq!(WindowKind::Weekly.of(&usage).map(|w| w.percent), Some(40.0));
        assert_eq!(
            WindowKind::Monthly.of(&usage).map(|w| w.percent),
            Some(100.0)
        );
        assert_eq!(WindowKind::ALL.len(), 3);
    }

    #[test]
    fn the_headline_is_the_rolling_remainder() {
        let usage = GoUsage::parse(SAMPLE).expect("valid reply");
        assert_eq!(usage.headline(), "Go 88%");

        let spent = GoUsage {
            usage: Usage {
                rolling: Some(UsageWindow {
                    status: "rate_limited".to_string(),
                    percent: 10.0,
                    resets_at: None,
                }),
                ..Default::default()
            },
        };
        assert_eq!(spent.headline(), "Go 已用尽");

        let missing = GoUsage {
            usage: Usage::default(),
        };
        assert_eq!(missing.headline(), "Go —");
    }

    /// `reset_in` is the gap to the stamp, and a stamp already past leaves `None`.
    #[test]
    fn reset_in_is_the_gap_to_the_stamp() {
        let window = |resets_at: &str| UsageWindow {
            status: "ok".to_string(),
            percent: 0.0,
            resets_at: Some(resets_at.to_string()),
        };
        let before = parse_rfc3339_utc("2026-09-21T11:00:00Z").unwrap();
        assert_eq!(
            window("2026-09-21T12:00:00Z").reset_in(before),
            Some(Duration::from_secs(3_600))
        );
        let after = parse_rfc3339_utc("2026-09-21T13:00:00Z").unwrap();
        assert_eq!(window("2026-09-21T12:00:00Z").reset_in(after), None);
    }
}
