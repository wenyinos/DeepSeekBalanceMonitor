//! OpenCode Go usage endpoint.

use std::time::Duration;

use chrono::{DateTime, Local};

use super::{http_client, sanitize_message};
use crate::model::{
    OpenCodeGoApiResponse, OpenCodeGoApiWindow, OpenCodeGoQuota, OpenCodeGoUsage,
};

const USAGE_URL: &str = "https://opencode.ai/zen/go/v1/usage";

/// Reads the rolling, weekly and monthly usage windows.
pub fn fetch_quota(api_key: &str, http_proxy: &str) -> Result<OpenCodeGoQuota, String> {
    let client = http_client(Duration::from_secs(10), http_proxy)?;
    let response = client
        .get(USAGE_URL)
        .header("Accept", "application/json")
        .header("Authorization", format!("Bearer {}", api_key.trim()))
        .send()
        .map_err(|error| format!("OpenCode Go request failed: {error}"))?;

    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().unwrap_or_default();
        let message = parse_api_error(&text).unwrap_or_else(|| sanitize_message(&text));
        return Err(format!("OpenCode Go API error {status}: {message}"));
    }

    let payload: OpenCodeGoApiResponse = response
        .error_for_status()
        .map_err(|error| error.to_string())?
        .json()
        .map_err(|error| format!("OpenCode Go JSON parse failed: {error}"))?;

    let now = Local::now().timestamp();
    let quota = OpenCodeGoQuota {
        rolling: payload.usage.rolling.map(|window| window_to_usage(window, now)),
        weekly: payload.usage.weekly.map(|window| window_to_usage(window, now)),
        monthly: payload.usage.monthly.map(|window| window_to_usage(window, now)),
    };

    if quota.rolling.is_none() && quota.weekly.is_none() && quota.monthly.is_none() {
        return Err("OpenCode Go API returned no usage windows.".to_owned());
    }
    Ok(quota)
}

fn window_to_usage(window: OpenCodeGoApiWindow, now: i64) -> OpenCodeGoUsage {
    let usage_percent = window.percent.clamp(0.0, 100.0);
    let reset_in_sec = window
        .resets_at
        .as_deref()
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
        .map(|reset_at| (reset_at.timestamp() - now).max(0))
        .unwrap_or(0);

    OpenCodeGoUsage {
        usage_percent,
        percent_remaining: (100.0 - usage_percent).max(0.0),
        reset_in_sec,
    }
}

/// The endpoint reports failures as `{"error": {"message": ...}}`.
fn parse_api_error(text: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(text).ok()?;
    value
        .get("error")?
        .get("message")?
        .as_str()
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_a_window_with_a_reset_time() {
        let window = OpenCodeGoApiWindow {
            percent: 42.5,
            resets_at: Some("2026-01-01T00:00:00Z".to_owned()),
        };
        let reset_at = DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
            .unwrap()
            .timestamp();
        let usage = window_to_usage(window, reset_at - 3600);
        assert_eq!(usage.usage_percent, 42.5);
        assert_eq!(usage.percent_remaining, 57.5);
        assert_eq!(usage.reset_in_sec, 3600);
    }

    #[test]
    fn clamps_and_tolerates_a_missing_reset_time() {
        let usage = window_to_usage(
            OpenCodeGoApiWindow {
                percent: 150.0,
                resets_at: None,
            },
            0,
        );
        assert_eq!(usage.usage_percent, 100.0);
        assert_eq!(usage.percent_remaining, 0.0);
        assert_eq!(usage.reset_in_sec, 0);
    }

    #[test]
    fn reads_the_error_message() {
        let text = r#"{"error":{"message":"Invalid token"}}"#;
        assert_eq!(parse_api_error(text).as_deref(), Some("Invalid token"));
        assert!(parse_api_error("not json").is_none());
    }
}
