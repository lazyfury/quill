//! The DeepSeek balance endpoint: request, response model and env overrides.
//!
//! The network layer is deliberately separate from the UI: [`fetch`] is a plain
//! blocking call, so the window host can run it on a worker thread while the UI
//! keeps painting, and the headless UI tests never touch the network.

use serde::Deserialize;

/// Temporary key used when `DEEPSEEK_API_KEY` is not set.
const DEFAULT_API_KEY: &str = "sk-1b0e98ffb7544540ba26854859af2042";
/// Balance endpoint used when `DEEPSEEK_BALANCE_URL` is not set.
const DEFAULT_BALANCE_URL: &str = "https://api.deepseek.com/user/balance";

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

/// Currency code to symbol, falling back to the code itself.
fn symbol(currency: &str) -> String {
    match currency {
        "CNY" | "RMB" => "¥".to_string(),
        "USD" => "$".to_string(),
        "EUR" => "€".to_string(),
        "HKD" => "HK$".to_string(),
        // An unknown code needs a separator, or `JPY1000` reads as one token.
        other => format!("{other} "),
    }
}

/// Balance endpoint, overridable with `DEEPSEEK_BALANCE_URL`.
pub fn endpoint() -> String {
    std::env::var("DEEPSEEK_BALANCE_URL").unwrap_or_else(|_| DEFAULT_BALANCE_URL.to_string())
}

/// API key, overridable with `DEEPSEEK_API_KEY`.
pub fn api_key() -> String {
    std::env::var("DEEPSEEK_API_KEY").unwrap_or_else(|_| DEFAULT_API_KEY.to_string())
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
            Err(format!("HTTP {code}: {}", body.trim()))
        }
        Err(error) => Err(format!("请求失败: {error}")),
    }
}

/// `HH:MM:SS UTC` timestamp for the "refreshed at" line.
///
/// Pure `std`: the tool has no date crate, and a wall-clock stamp is all the UI
/// needs.
pub fn timestamp() -> String {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0);
    let seconds_of_day = seconds % 86_400;
    format!(
        "{:02}:{:02}:{:02} UTC",
        seconds_of_day / 3600,
        (seconds_of_day % 3600) / 60,
        seconds_of_day % 60
    )
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

    #[test]
    fn timestamp_is_a_clock_stamp() {
        let stamp = timestamp();
        assert_eq!(stamp.len(), "00:00:00 UTC".len(), "{stamp}");
        assert!(stamp.ends_with(" UTC"), "{stamp}");
    }

    #[test]
    fn the_headline_is_symbol_plus_amount() {
        let balance = Balance::parse(SAMPLE).expect("valid reply");
        assert_eq!(balance.headline(), "¥110.00");
    }

    #[test]
    fn the_headline_names_an_unknown_currency() {
        let mut balance = Balance::parse(SAMPLE).expect("valid reply");
        balance.balance_infos[0].currency = "JPY".to_string();
        assert_eq!(balance.headline(), "JPY 110.00");
    }

    #[test]
    fn an_empty_reply_still_has_a_headline() {
        let balance = Balance {
            is_available: false,
            balance_infos: Vec::new(),
        };
        assert_eq!(balance.headline(), "无余额信息");
    }
}
