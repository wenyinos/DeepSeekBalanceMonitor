//! Domain types shared by the API clients, the history store and the interface.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// One currency's balance as reported by the DeepSeek API.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Balance {
    pub total_balance: f64,
    pub granted_balance: f64,
    pub topped_up_balance: f64,
}

/// A single row of the balance history table.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HistoryRecord {
    pub timestamp: String,
    pub currency: String,
    pub total: f64,
    pub topped: f64,
    pub granted: f64,
    pub service_status: String,
}

/// Aggregate over the records of one currency.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HistorySummary {
    pub currency: String,
    pub records: usize,
    pub first_time: String,
    pub last_time: String,
    pub latest_total: f64,
    pub latest_topped: f64,
    pub latest_granted: f64,
    pub min_total: f64,
    pub max_total: f64,
    pub avg_total: f64,
    pub change_total: f64,
}

/// Busy-hour consumption estimate for the preferred currency.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConsumptionRate {
    pub hourly_rate: f64,
    pub busy_hours_left: f64,
    pub currency: String,
}

/// One quota window of the OpenCode Go subscription.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpenCodeGoUsage {
    pub usage_percent: f64,
    pub percent_remaining: f64,
    pub reset_in_sec: i64,
}

/// The three OpenCode Go windows.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct OpenCodeGoQuota {
    pub rolling: Option<OpenCodeGoUsage>,
    pub weekly: Option<OpenCodeGoUsage>,
    pub monthly: Option<OpenCodeGoUsage>,
}

#[derive(Debug, Deserialize)]
pub struct OpenCodeGoApiResponse {
    pub usage: OpenCodeGoApiUsage,
}

#[derive(Debug, Deserialize)]
pub struct OpenCodeGoApiUsage {
    #[serde(default)]
    pub rolling: Option<OpenCodeGoApiWindow>,
    #[serde(default)]
    pub weekly: Option<OpenCodeGoApiWindow>,
    #[serde(default)]
    pub monthly: Option<OpenCodeGoApiWindow>,
}

#[derive(Debug, Deserialize)]
pub struct OpenCodeGoApiWindow {
    #[serde(default, deserialize_with = "deserialize_number")]
    pub percent: f64,
    #[serde(rename = "resetsAt", default)]
    pub resets_at: Option<String>,
}

/// One quota window of the Command Code subscription.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CommandCodeWindow {
    pub used: f64,
    pub cap: f64,
    pub reset_in_sec: i64,
}

/// The Command Code windows: five-hour, weekly and the derived monthly view.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CommandCodeQuota {
    pub five_hour: Option<CommandCodeWindow>,
    pub weekly: Option<CommandCodeWindow>,
    pub monthly: Option<CommandCodeWindow>,
}

#[derive(Debug, Deserialize)]
pub struct CommandCodeApiResponse {
    pub credits: CommandCodeApiCredits,
    #[serde(rename = "windowLimits")]
    pub window_limits: CommandCodeApiLimits,
}

#[derive(Debug, Deserialize)]
pub struct CommandCodeApiCredits {
    #[serde(
        rename = "monthlyCredits",
        default,
        deserialize_with = "deserialize_optional_number"
    )]
    pub monthly_credits: Option<f64>,
}

#[derive(Debug, Deserialize)]
pub struct CommandCodeApiLimits {
    #[serde(rename = "fiveHour", default)]
    pub five_hour: Option<CommandCodeApiWindow>,
    #[serde(default)]
    pub weekly: Option<CommandCodeApiWindow>,
}

#[derive(Debug, Deserialize)]
pub struct CommandCodeApiWindow {
    #[serde(default, deserialize_with = "deserialize_number")]
    pub used: f64,
    #[serde(default, deserialize_with = "deserialize_number")]
    pub cap: f64,
    #[serde(
        rename = "resetAt",
        default,
        deserialize_with = "deserialize_optional_number"
    )]
    pub reset_at: Option<f64>,
}

#[derive(Debug, Deserialize)]
pub struct CommandCodeApiWhoami {
    pub org: Option<CommandCodeApiOrg>,
}

#[derive(Debug, Deserialize)]
pub struct CommandCodeApiOrg {
    #[serde(default)]
    pub id: Option<String>,
}

/// Response of the DeepSeek balance endpoint.
#[derive(Debug, Deserialize)]
pub struct ApiResponse {
    #[serde(default)]
    pub balance_infos: Vec<ApiBalanceInfo>,
}

#[derive(Debug, Deserialize)]
pub struct ApiBalanceInfo {
    #[serde(default = "default_currency")]
    pub currency: String,
    #[serde(default)]
    pub total_balance: String,
    #[serde(default)]
    pub granted_balance: String,
    #[serde(default)]
    pub topped_up_balance: String,
}

fn default_currency() -> String {
    "CNY".to_owned()
}

/// Accepts either a JSON number or a numeric string.
pub fn deserialize_number<'de, D>(deserializer: D) -> Result<f64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::Error;
    match serde_json::Value::deserialize(deserializer)? {
        serde_json::Value::Number(number) => number
            .as_f64()
            .ok_or_else(|| D::Error::custom("expected numeric value")),
        serde_json::Value::String(text) => text
            .parse::<f64>()
            .map_err(|_| D::Error::custom("expected numeric string")),
        other => Err(D::Error::custom(format!(
            "expected number or numeric string, got {other}"
        ))),
    }
}

/// Like [`deserialize_number`], but tolerates null.
pub fn deserialize_optional_number<'de, D>(deserializer: D) -> Result<Option<f64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::Error;
    match serde_json::Value::deserialize(deserializer)? {
        serde_json::Value::Null => Ok(None),
        serde_json::Value::Number(number) => number
            .as_f64()
            .ok_or_else(|| D::Error::custom("expected numeric value"))
            .map(Some),
        serde_json::Value::String(text) => text
            .parse::<f64>()
            .map(Some)
            .map_err(|_| D::Error::custom("expected numeric string")),
        other => Err(D::Error::custom(format!(
            "expected number or numeric string, got {other}"
        ))),
    }
}

/// Balances keyed by currency code.
pub type Balances = BTreeMap<String, Balance>;

/// The balance to show by default: CNY when present, otherwise the first one.
pub fn preferred_balance(balances: &Balances) -> Option<(&String, &Balance)> {
    balances
        .get_key_value("CNY")
        .or_else(|| balances.iter().next())
}

/// Whether the preferred balance sits below the alert threshold.
pub fn is_low_balance(balances: &Balances, threshold: f64) -> bool {
    preferred_balance(balances)
        .map(|(_, balance)| balance.total_balance < threshold)
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_numeric_strings_and_numbers_alike() {
        let payload = r#"{
            "credits": { "monthlyCredits": "150" },
            "windowLimits": {
                "fiveHour": { "used": 12, "cap": "80", "resetAt": 1767225600000.0 },
                "weekly":   { "used": "3.5", "cap": 40 }
            }
        }"#;
        let parsed: CommandCodeApiResponse = serde_json::from_str(payload).unwrap();
        assert_eq!(parsed.credits.monthly_credits, Some(150.0));
        let five_hour = parsed.window_limits.five_hour.unwrap();
        assert_eq!(five_hour.used, 12.0);
        assert_eq!(five_hour.cap, 80.0);
        assert_eq!(five_hour.reset_at, Some(1_767_225_600_000.0));
        let weekly = parsed.window_limits.weekly.unwrap();
        assert_eq!(weekly.used, 3.5);
        assert_eq!(weekly.reset_at, None);
    }

    #[test]
    fn parses_opencode_go_windows() {
        let payload = r#"{
            "usage": {
                "rolling": { "percent": 42.5, "resetsAt": "2026-01-01T00:00:00Z" },
                "weekly":  { "percent": "10" },
                "monthly": null
            }
        }"#;
        let parsed: OpenCodeGoApiResponse = serde_json::from_str(payload).unwrap();
        assert_eq!(parsed.usage.rolling.as_ref().unwrap().percent, 42.5);
        assert_eq!(parsed.usage.weekly.as_ref().unwrap().percent, 10.0);
        assert!(parsed.usage.monthly.is_none());
    }

    #[test]
    fn reads_deepseek_balances_from_strings() {
        let payload = r#"{
            "balance_infos": [
                { "currency": "CNY", "total_balance": "12.34", "granted_balance": "1.00", "topped_up_balance": "11.34" }
            ]
        }"#;
        let parsed: ApiResponse = serde_json::from_str(payload).unwrap();
        let info = &parsed.balance_infos[0];
        assert_eq!(info.currency, "CNY");
        assert_eq!(info.total_balance.parse::<f64>().unwrap(), 12.34);
    }
}
