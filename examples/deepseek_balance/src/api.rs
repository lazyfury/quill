//! The DeepSeek balance endpoint: request, response model and env overrides.
//!
//! The network layer is deliberately separate from the UI: [`fetch`] is a plain
//! blocking call, so the window host can run it on a worker thread while the UI
//! keeps painting, and the headless UI tests never touch the network.

use deepseek_util::currency::symbol;
use serde::Deserialize;

/// Balance endpoint used when `DEEPSEEK_BALANCE_URL` is not set.
const DEFAULT_BALANCE_URL: &str = "https://api.deepseek.com/user/balance";

/// The error [`resolve_api_key`] reports when no key is configured anywhere.
pub const MISSING_KEY_MESSAGE: &str =
    "未配置 DEEPSEEK_API_KEY 环境变量（写入 ~/.zshrc 后点刷新即可，无需重启）";

/// A balance reply: availability plus one entry per currency.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Balance {
    pub is_available: bool,
    pub balance_infos: Vec<BalanceInfo>,
}

/// Balance for a single currency, as returned by the API (amounts are strings).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct BalanceInfo {
    pub currency: String,
    pub total_balance: String,
    pub granted_balance: String,
    pub topped_up_balance: String,
}

impl Balance {
    /// Parses a response body.
    pub fn parse(body: &str) -> Result<Self, String> {
        serde_json::from_str(body).map_err(|error| format!("解析响应失败: {error}"))
    }

    /// A single line for the menu bar, e.g. `¥-0.19`.
    ///
    /// Only the first currency is shown: the status bar is a handful of
    /// characters wide, and the panel next to it carries the full breakdown.
    pub fn headline(&self) -> String {
        match self.balance_infos.first() {
            Some(info) => format!("{}{}", symbol(&info.currency), info.total_balance),
            None => "无余额信息".to_string(),
        }
    }
}

/// Balance endpoint, overridable with `DEEPSEEK_BALANCE_URL`.
pub fn endpoint() -> String {
    std::env::var("DEEPSEEK_BALANCE_URL").unwrap_or_else(|_| DEFAULT_BALANCE_URL.to_string())
}

/// API key from `DEEPSEEK_API_KEY`, `None` when it is unset or blank.
pub fn api_key() -> Option<String> {
    non_blank(std::env::var("DEEPSEEK_API_KEY").ok())
}

/// Re-reads the key from the user's login + interactive shell, `None` when it
/// yields nothing.
///
/// Two reasons this exists: a packaged `.app` (Finder / `open`) never inherits
/// shell environment variables, and a running process cannot see changes made
/// to its environment after it started. Asking `$SHELL -lic` for the variable
/// is the only way "set the var, then hit refresh" can pick it up without a
/// restart. The `-i` matters: exports usually live in `~/.zshrc` / `~/.bashrc`,
/// which a plain `zsh -lc` never sources. Interactive setups may print their
/// own noise to stdout, so the value is fetched behind a marker and everything
/// before it is ignored. Unix only; elsewhere the process env is all there is.
pub fn api_key_from_shell() -> Option<String> {
    const MARKER: &str = "__QUILL_KEY__";
    let shell = std::env::var("SHELL").ok()?;
    let output = std::process::Command::new(shell)
        .args([
            "-lic",
            &format!("printf %s \"{MARKER}${{DEEPSEEK_API_KEY}}\""),
        ])
        .stdin(std::process::Stdio::null())
        .output()
        .ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    // `rsplit` lands on the chunk after the *last* marker, i.e. our value.
    non_blank(stdout.rsplit(MARKER).next().map(String::from))
}

/// The key to fetch with: process env first, then the login shell — so a
/// refresh after configuring the variable works without restarting. Without a
/// key anywhere this is the "未配置" error the UI shows verbatim.
pub fn resolve_api_key() -> Result<String, String> {
    api_key()
        .or_else(api_key_from_shell)
        .ok_or_else(|| MISSING_KEY_MESSAGE.to_string())
}

/// Trims surrounding whitespace and drops empty results, so a blank or
/// newline-padded value counts as unconfigured rather than a bad key.
pub(crate) fn non_blank(value: Option<String>) -> Option<String> {
    value
        .map(|key| key.trim().to_string())
        .filter(|key| !key.is_empty())
}

/// Queries the balance endpoint. Blocking; call it off the UI thread.
pub fn fetch(endpoint: &str, api_key: &str) -> Result<Balance, String> {
    let response = ureq::get(endpoint)
        .set("Authorization", &format!("Bearer {api_key}"))
        .set("Accept", "application/json")
        .call();

    match response {
        Ok(response) => {
            let body = response
                .into_string()
                .map_err(|error| format!("读取响应失败: {error}"))?;
            Balance::parse(&body)
        }
        Err(ureq::Error::Status(code, response)) => {
            let body = response.into_string().unwrap_or_default();
            Err(format!("HTTP {code}: {}", error_detail(&body)))
        }
        Err(error) => Err(format!("请求失败: {error}")),
    }
}

/// Longest error detail shown verbatim; longer bodies are cut with `…` so one
/// pathological reply cannot flood the error line (and, before the layout cap,
/// stretch the panel to the width of the widest unbreakable token).
const MAX_ERROR_DETAIL_CHARS: usize = 160;

/// One line of error detail from a response body.
///
/// An API failure (wrong key, no quota) comes back as a JSON object whose
/// `error.message` is the human-readable reason; showing the body verbatim
/// puts a dense JSON line on screen. Anything else is shown trimmed. Either
/// way the result is capped to [`MAX_ERROR_DETAIL_CHARS`].
///
/// Shared with [`crate::go`], whose endpoint reports failures the same way.
pub(crate) fn error_detail(body: &str) -> String {
    let detail = serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|value| {
            value
                .get("error")?
                .get("message")?
                .as_str()
                .map(str::trim)
                .filter(|message| !message.is_empty())
                .map(str::to_string)
        })
        .unwrap_or_else(|| body.trim().to_string());
    truncate_chars(&detail, MAX_ERROR_DETAIL_CHARS)
}

fn truncate_chars(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_string()
    } else {
        format!("{}…", text.chars().take(max).collect::<String>())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
        "is_available": true,
        "balance_infos": [
            {
                "currency": "CNY",
                "total_balance": "110.00",
                "granted_balance": "10.00",
                "topped_up_balance": "100.00"
            }
        ]
    }"#;

    #[test]
    fn parses_a_balance_reply() {
        let balance = Balance::parse(SAMPLE).expect("valid reply");
        assert!(balance.is_available);
        assert_eq!(balance.balance_infos.len(), 1);
        let info = &balance.balance_infos[0];
        assert_eq!(info.currency, "CNY");
        assert_eq!(info.total_balance, "110.00");
        assert_eq!(info.granted_balance, "10.00");
        assert_eq!(info.topped_up_balance, "100.00");
    }

    #[test]
    fn reports_a_bad_reply() {
        let error = Balance::parse("not json").expect_err("invalid body");
        assert!(error.contains("解析响应失败"), "{error}");
    }

    /// The headline is the symbol plus the amount: a known currency (the first
    /// entry only), an unknown code, and an empty reply all have an answer.
    #[test]
    fn the_headline_is_symbol_plus_amount() {
        let balance = Balance::parse(SAMPLE).expect("valid reply");
        assert_eq!(balance.headline(), "¥110.00");

        let mut unknown = balance.clone();
        unknown.balance_infos[0].currency = "JPY".to_string();
        assert_eq!(unknown.headline(), "JPY 110.00");

        let empty = Balance {
            is_available: false,
            balance_infos: Vec::new(),
        };
        assert_eq!(empty.headline(), "无余额信息");
    }

    #[test]
    fn blank_keys_count_as_unconfigured() {
        assert_eq!(non_blank(None), None);
        assert_eq!(non_blank(Some(String::new())), None);
        assert_eq!(non_blank(Some("   ".to_string())), None);
        assert_eq!(
            non_blank(Some("sk-test".to_string())),
            Some("sk-test".into())
        );
        assert_eq!(
            non_blank(Some("  sk-test\n".to_string())),
            Some("sk-test".into())
        );
    }

    #[test]
    fn the_missing_key_message_names_the_variable() {
        assert!(MISSING_KEY_MESSAGE.contains("DEEPSEEK_API_KEY"));
    }

    /// A wrong key returns `{"error":{"message":...}}`; the user sees the
    /// reason, not the JSON envelope.
    #[test]
    fn an_api_error_body_shows_its_message_not_the_json() {
        let body = r#"{"error":{"message":"Authentication Fails (no such user)","type":"authentication_error","code":"invalid_request_error"}}"#;
        assert_eq!(error_detail(body), "Authentication Fails (no such user)");
    }

    #[test]
    fn a_non_json_error_body_shows_itself_trimmed() {
        assert_eq!(error_detail("  gateway timeout  "), "gateway timeout");
    }

    /// JSON without an `error.message` (or with a blank one) falls back to the
    /// body instead of showing nothing.
    #[test]
    fn json_without_an_error_message_falls_back_to_the_body() {
        assert_eq!(error_detail(r#"{"ok":false}"#), r#"{"ok":false}"#);
        assert_eq!(
            error_detail(r#"{"error":{"message":"   "}}"#),
            r#"{"error":{"message":"   "}}"#
        );
    }

    #[test]
    fn an_overlong_detail_is_truncated() {
        let body = "x".repeat(500);
        let detail = error_detail(&body);
        assert_eq!(detail.chars().count(), MAX_ERROR_DETAIL_CHARS + 1);
        assert!(detail.ends_with('…'), "{detail}");
    }
}
