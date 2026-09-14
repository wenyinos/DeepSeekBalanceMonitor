//! MiniMax plan usage, for the Token and Coding plans.
//!
//! The four entries share two hosts and differ by path: the Token Plan and the
//! Coding Plan report the same shape from different endpoints. Only a five-hour
//! and a weekly window exist; there is no monthly one.

use std::time::Duration;

use chrono::Local;

use super::{epoch_to_reset_seconds, http_client, sanitize_message};
use crate::model::PackageQuota;

const HOST_CN: &str = "https://www.minimaxi.com";
const HOST_GLOBAL: &str = "https://www.minimax.io";
const TOKEN_PATH: &str = "/v1/token_plan/remains";
const CODING_PATH: &str = "/v1/api/openplatform/coding_plan/remains";

/// The window of a plan that reports its usage as what is left.
#[derive(Debug, Default, serde::Deserialize)]
struct ApiRemains {
    #[serde(default, deserialize_with = "crate::model::deserialize_number")]
    current_interval_remaining_percent: f64,
    #[serde(default, deserialize_with = "crate::model::deserialize_number")]
    current_weekly_remaining_percent: f64,
    #[serde(
        default,
        deserialize_with = "crate::model::deserialize_optional_number"
    )]
    end_time: Option<f64>,
    #[serde(
        default,
        deserialize_with = "crate::model::deserialize_optional_number"
    )]
    weekly_end_time: Option<f64>,
}

/// Reads the five-hour and weekly windows of one plan.
pub fn fetch_quota(
    platform: &str,
    api_key: &str,
    http_proxy: &str,
) -> Result<PackageQuota, String> {
    let path = if platform.starts_with("minimax_coding") {
        CODING_PATH
    } else {
        TOKEN_PATH
    };
    let host = if platform.ends_with("_cn") {
        HOST_CN
    } else {
        HOST_GLOBAL
    };

    let client = http_client(Duration::from_secs(10), http_proxy)?;
    let response = client
        .get(format!("{host}{path}"))
        .header("Accept", "application/json")
        .header("Authorization", format!("Bearer {}", api_key.trim()))
        .send()
        .map_err(|error| format!("MiniMax request failed: {error}"))?;

    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().unwrap_or_default();
        return Err(format!(
            "MiniMax API error {status}: {}",
            sanitize_message(&text)
        ));
    }

    let body: serde_json::Value = response
        .error_for_status()
        .map_err(|error| error.to_string())?
        .json()
        .map_err(|error| format!("MiniMax JSON parse failed: {error}"))?;

    read_quota(&body, Local::now().timestamp())
}

/// Turns a response body into the two windows, or says why it cannot.
fn read_quota(body: &serde_json::Value, now: i64) -> Result<PackageQuota, String> {
    // The two plans answer with the list in different places.
    let remains = body
        .get("data")
        .and_then(|data| data.get("model_remains"))
        .or_else(|| body.get("model_remains"));

    let Some(entries) = remains.and_then(|value| value.as_array()) else {
        return Err("MiniMax API returned no usage windows.".to_owned());
    };

    // The list holds every model; the account's own allowance is the general
    // one, and the first entry is the fallback when there is no such entry.
    let chosen = entries
        .iter()
        .find(|entry| entry.get("model_name").and_then(|name| name.as_str()) == Some("general"))
        .or_else(|| entries.first())
        .ok_or_else(|| "MiniMax API returned no usage windows.".to_owned())?;

    let remains: ApiRemains = serde_json::from_value(chosen.clone())
        .map_err(|error| format!("MiniMax JSON parse failed: {error}"))?;

    // The percentages are what is left, and the windows are what is used.
    let mut quota = PackageQuota::new();
    quota.insert(
        "5h".to_owned(),
        crate::model::QuotaWindow::from_percent(
            100.0 - remains.current_interval_remaining_percent,
            epoch_to_reset_seconds(remains.end_time, now),
        ),
    );
    quota.insert(
        "weekly".to_owned(),
        crate::model::QuotaWindow::from_percent(
            100.0 - remains.current_weekly_remaining_percent,
            epoch_to_reset_seconds(remains.weekly_end_time, now),
        ),
    );
    Ok(quota)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_remaining_percentages_as_used_ones() {
        let body = serde_json::json!({
            "data": {
                "model_remains": [
                    { "model_name": "general",
                      "current_interval_remaining_percent": 62.0,
                      "current_weekly_remaining_percent": 88.5 }
                ]
            }
        });

        let quota = read_quota(&body, 0).expect("the body parses");
        let five_hour = &quota["5h"];
        assert_eq!(five_hour.usage_percent, 38.0);
        assert_eq!(five_hour.percent_remaining, 62.0);

        let weekly = &quota["weekly"];
        assert_eq!(weekly.usage_percent, 11.5);
    }

    #[test]
    fn the_general_model_wins_and_the_first_entry_is_the_fallback() {
        let with_general = serde_json::json!({
            "model_remains": [
                { "model_name": "abab-6.5", "current_interval_remaining_percent": 10.0 },
                { "model_name": "general", "current_interval_remaining_percent": 40.0 }
            ]
        });
        let quota = read_quota(&with_general, 0).expect("the body parses");
        assert_eq!(quota["5h"].usage_percent, 60.0, "the general entry is used");

        let without_general = serde_json::json!({
            "model_remains": [{ "model_name": "abab-6.5", "current_interval_remaining_percent": 10.0 }]
        });
        let quota = read_quota(&without_general, 0).expect("the body parses");
        assert_eq!(quota["5h"].usage_percent, 90.0, "the first entry is used");
    }

    #[test]
    fn a_body_without_entries_is_an_error() {
        assert!(read_quota(&serde_json::json!({ "data": {} }), 0).is_err());
        assert!(read_quota(&serde_json::json!({ "model_remains": [] }), 0).is_err());
    }

    #[test]
    fn timestamps_are_taken_in_seconds_or_milliseconds() {
        let now = 1_767_225_600;
        let in_an_hour = (now + 3600) as f64;

        let seconds = serde_json::json!({
            "model_remains": [{ "model_name": "general",
                "current_interval_remaining_percent": 0.0,
                "end_time": in_an_hour }]});
        assert_eq!(read_quota(&seconds, now).unwrap()["5h"].reset_in_sec, 3600);

        let millis = serde_json::json!({
            "model_remains": [{ "model_name": "general",
                "current_interval_remaining_percent": 0.0,
                "end_time": in_an_hour * 1000.0 }]});
        assert_eq!(
            read_quota(&millis, now).unwrap()["5h"].reset_in_sec,
            3600,
            "a millisecond epoch is read as milliseconds"
        );
    }

    #[test]
    fn percentages_are_clamped_to_the_window() {
        let body = serde_json::json!({
            "model_remains": [{ "model_name": "general",
                "current_interval_remaining_percent": -5.0,
                "current_weekly_remaining_percent": 130.0 }]});

        let quota = read_quota(&body, 0).unwrap();
        assert_eq!(quota["5h"].usage_percent, 100.0, "105 used clamps to 100");
        assert_eq!(quota["weekly"].usage_percent, 0.0, "-30 used clamps to 0");
    }
}
