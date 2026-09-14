//! GLM Coding Plan usage.
//!
//! One endpoint per region reports a list of limits. The list carries no window
//! names, so the windows are read from it by position: the first two entries of
//! `TOKENS_LIMIT` are the five-hour and weekly windows, and `TIME_LIMIT` is the
//! monthly one — which counts tool calls rather than tokens, unlike the others.

use std::time::Duration;

use chrono::Local;

use super::{epoch_to_reset_seconds, http_client, sanitize_message};
use crate::model::PackageQuota;

const URL_CN: &str = "https://open.bigmodel.cn/api/monitor/usage/quota/limit";
const URL_GLOBAL: &str = "https://api.z.ai/api/monitor/usage/quota/limit";

/// Reads the three windows of a Coding Plan.
pub fn fetch_quota(
    platform: &str,
    api_key: &str,
    http_proxy: &str,
) -> Result<PackageQuota, String> {
    let url = if platform.ends_with("_cn") {
        URL_CN
    } else {
        URL_GLOBAL
    };

    let client = http_client(Duration::from_secs(10), http_proxy)?;
    let key = api_key.trim();
    let mut response = send(&client, url, &format!("Bearer {key}"))?;

    // Some keys are rejected with a bearer prefix and accepted without it.
    if response.status() == reqwest::StatusCode::UNAUTHORIZED {
        response = send(&client, url, key)?;
    }

    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().unwrap_or_default();
        return Err(format!(
            "GLM API error {status}: {}",
            sanitize_message(&text)
        ));
    }

    let body: serde_json::Value = response
        .error_for_status()
        .map_err(|error| error.to_string())?
        .json()
        .map_err(|error| format!("GLM JSON parse failed: {error}"))?;

    read_quota(&body, Local::now().timestamp())
}

fn send(
    client: &reqwest::blocking::Client,
    url: &str,
    authorization: &str,
) -> Result<reqwest::blocking::Response, String> {
    client
        .get(url)
        .header("Accept", "application/json")
        .header("Authorization", authorization)
        .send()
        .map_err(|error| format!("GLM request failed: {error}"))
}

/// One entry of the limits list.
#[derive(Debug, Default, serde::Deserialize)]
struct ApiLimit {
    #[serde(rename = "type", default)]
    kind: String,
    #[serde(default, deserialize_with = "crate::model::deserialize_number")]
    percentage: f64,
    #[serde(
        rename = "nextResetTime",
        default,
        deserialize_with = "crate::model::deserialize_optional_number"
    )]
    next_reset_time: Option<f64>,
}

/// Turns a response body into the windows, or says why it cannot.
fn read_quota(body: &serde_json::Value, now: i64) -> Result<PackageQuota, String> {
    // The endpoint answers with a code and a flag rather than a status alone.
    let code = body
        .get("code")
        .and_then(|code| code.as_i64())
        .unwrap_or(-1);
    let success = body
        .get("success")
        .and_then(|success| success.as_bool())
        .unwrap_or(false);
    if !matches!(code, 0 | 200) && !success {
        let message = body
            .get("msg")
            .or_else(|| body.get("message"))
            .and_then(|message| message.as_str())
            .unwrap_or("no reason given");
        return Err(format!("GLM API error {code}: {message}"));
    }

    let limits: Vec<ApiLimit> = body
        .get("data")
        .and_then(|data| data.get("limits"))
        .and_then(|limits| serde_json::from_value(limits.clone()).ok())
        .unwrap_or_default();
    if limits.is_empty() {
        return Err("GLM API returned no usage windows.".to_owned());
    }

    let mut quota = PackageQuota::new();

    // The token windows come in order, five-hour first.
    let mut token_windows = limits.iter().filter(|limit| limit.kind == "TOKENS_LIMIT");
    for name in ["5h", "weekly"] {
        if let Some(limit) = token_windows.next() {
            quota.insert(name.to_owned(), window_of(limit, now));
        }
    }

    // The monthly window counts tool calls, and stands alone.
    if let Some(limit) = limits.iter().find(|limit| limit.kind == "TIME_LIMIT") {
        quota.insert("monthly".to_owned(), window_of(limit, now));
    }

    if quota.is_empty() {
        return Err("GLM API returned no usage windows.".to_owned());
    }
    Ok(quota)
}

fn window_of(limit: &ApiLimit, now: i64) -> crate::model::QuotaWindow {
    crate::model::QuotaWindow::from_percent(
        limit.percentage,
        epoch_to_reset_seconds(limit.next_reset_time, now),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(limits: serde_json::Value) -> serde_json::Value {
        serde_json::json!({ "code": 0, "success": true, "data": { "limits": limits } })
    }

    #[test]
    fn the_token_windows_are_read_in_order_and_the_time_window_is_monthly() {
        let body = body(serde_json::json!([
            { "type": "TOKENS_LIMIT", "percentage": 12.0 },
            { "type": "TOKENS_LIMIT", "percentage": 34.0 },
            { "type": "TIME_LIMIT", "percentage": 56.0 }
        ]));

        let quota = read_quota(&body, 0).expect("the body parses");
        assert_eq!(quota["5h"].usage_percent, 12.0);
        assert_eq!(quota["weekly"].usage_percent, 34.0);
        assert_eq!(quota["monthly"].usage_percent, 56.0);
        assert_eq!(quota["5h"].percent_remaining, 88.0);
    }

    #[test]
    fn a_plan_without_a_time_limit_still_reads_its_token_windows() {
        let body = body(serde_json::json!([{ "type": "TOKENS_LIMIT", "percentage": 5.0 }]));

        let quota = read_quota(&body, 0).expect("the body parses");
        assert_eq!(quota.len(), 1);
        assert!(quota.contains_key("5h"));
        assert!(!quota.contains_key("weekly"), "no second entry, no window");
    }

    #[test]
    fn a_millisecond_reset_time_is_read_as_milliseconds() {
        let now = 1_767_225_600;
        let body = body(serde_json::json!([
            { "type": "TOKENS_LIMIT", "percentage": 1.0,
              "nextResetTime": ((now + 7200) as f64) * 1000.0 }
        ]));

        assert_eq!(read_quota(&body, now).unwrap()["5h"].reset_in_sec, 7200);
    }

    #[test]
    fn a_failed_response_is_an_error_rather_than_an_empty_reading() {
        let refused =
            serde_json::json!({ "code": 401, "success": false, "msg": "invalid api key" });
        let error = read_quota(&refused, 0).expect_err("a failure is an error");
        assert!(error.contains("invalid api key"), "{error}");

        // A code of 0 counts even when the flag is missing.
        let ok = serde_json::json!({ "code": 200, "data": { "limits": [
            { "type": "TOKENS_LIMIT", "percentage": 3.0 }] } });
        assert!(read_quota(&ok, 0).is_ok());
    }

    #[test]
    fn an_empty_list_is_an_error() {
        assert!(read_quota(&body(serde_json::json!([])), 0).is_err());
        assert!(read_quota(&serde_json::json!({ "code": 0, "success": true }), 0).is_err());
    }
}
