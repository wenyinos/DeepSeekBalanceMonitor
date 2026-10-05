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

/// How many times the host is asked before the failure is reported.
///
/// It drops a connection now and then — the previous build's logs are full of
/// `UNEXPECTED_EOF` — and the next attempt a second later answers normally.
/// Three tries cost two seconds in the worst case, against a poll interval
/// measured in minutes.
const ATTEMPTS: u32 = 3;
const RETRY_PAUSE: Duration = Duration::from_secs(1);

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

    let mut last = String::new();
    for attempt in 1..=ATTEMPTS {
        match ask(&client, host, path, api_key) {
            Ok(quota) => return Ok(quota),
            Err(error) => {
                let again = attempt < ATTEMPTS && worth_another_try(&error);
                last = error;
                if !again {
                    break;
                }
                std::thread::sleep(RETRY_PAUSE);
            }
        }
    }
    Err(last)
}

/// One request, from the connection to the two windows.
fn ask(
    client: &reqwest::blocking::Client,
    host: &str,
    path: &str,
    api_key: &str,
) -> Result<PackageQuota, String> {
    let response = client
        .get(format!("{host}{path}"))
        .header("Accept", "application/json")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", api_key.trim()))
        // The host is known to end a kept-alive connection without saying so,
        // which is what the retry above is for; not reusing one is the cheaper
        // half of the same fix.
        .header("Connection", "close")
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

/// Whether asking again could plausibly answer differently.
///
/// A dropped connection, a truncated body or a server-side hiccup answers
/// differently on the second try. A key the host rejects will be rejected every
/// time, and two seconds of waiting would only delay the message the user needs
/// to see.
fn worth_another_try(error: &str) -> bool {
    ![" 401 ", " 403 ", " 404 "]
        .iter()
        .any(|status| error.contains(status))
}

const STATUS_URL: &str = "https://status.minimax.io/";

/// The plan's own status page, read the way the earlier build read it: the
/// state word of the LLM components on the front page.
///
/// A page that answers nothing, or answers something this build cannot
/// recognize, yields `unknown`. (The earlier build answered `none` for such a
/// page — the same silent "healthy" its DeepSeek page once reported, which
/// the 1.x line fixed; the rule here is the fixed one.)
pub fn fetch_status(http_proxy: &str) -> String {
    let Ok(client) = http_client(Duration::from_secs(5), http_proxy) else {
        return "unknown".to_owned();
    };
    let fetched = client
        .get(STATUS_URL)
        .header("Accept", "text/html,*/*")
        .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64)")
        .send()
        .and_then(|response| response.error_for_status())
        .and_then(|response| response.text());
    match fetched {
        Ok(html) => parse_status_page(&html),
        Err(error) => {
            let _ = crate::storage::log_line(&format!(
                "The MiniMax status page could not be read ({error}); the status reads unknown."
            ));
            "unknown".to_owned()
        }
    }
}

/// Maps the LLM components' state word to the interface's indicator, folding
/// the vocabulary the earlier build's regex captured. Note that "major
/// outage" reads `major` there, not `critical` — the branch order decided
/// that, and the behavior is kept.
fn parse_status_page(html: &str) -> String {
    let lower = html
        .split('\n')
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();

    if let Some(anchor) = lower.find("large language models") {
        let rest = &lower[anchor..];
        // The word that answers is the first one to appear after the anchor,
        // the way the earlier build's non-greedy regex matched.
        let mut earliest: Option<(usize, &str)> = None;
        for (word, indicator) in [
            ("operational", "none"),
            ("degraded", "minor"),
            ("partial outage", "major"),
            ("major outage", "major"),
            ("under maintenance", "maintenance"),
        ] {
            if let Some(at) = rest.find(word) {
                if earliest.map_or(true, |(position, _)| at < position) {
                    earliest = Some((at, indicator));
                }
            }
        }
        if let Some((_, indicator)) = earliest {
            return indicator.to_owned();
        }
    }

    if lower.contains("all systems operational") {
        return "none".to_owned();
    }

    "unknown".to_owned()
}

/// Turns a response body into the two windows, or says why it cannot.
fn read_quota(body: &serde_json::Value, now: i64) -> Result<PackageQuota, String> {
    // The endpoint answers failures in-band: a `base_resp` status that is not
    // zero carries the reason — a rejected key, a session it wants, a quota it
    // will not show. Without this check such a body only reports "no usage
    // windows", which sends the user looking in the wrong place.
    let status_code = body
        .get("base_resp")
        .and_then(|resp| resp.get("status_code"))
        .and_then(|value| {
            value
                .as_i64()
                .or_else(|| value.as_str().and_then(|text| text.parse().ok()))
        })
        .unwrap_or(0);
    if status_code != 0 {
        let message = body
            .get("base_resp")
            .and_then(|resp| resp.get("status_msg"))
            .and_then(|value| value.as_str())
            .unwrap_or("no reason given");
        return Err(format!("MiniMax API error {status_code}: {message}"));
    }

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
    fn a_rejected_key_is_not_asked_about_again() {
        assert!(worth_another_try(
            "MiniMax request failed: connection closed before message completed"
        ));
        assert!(worth_another_try(
            "MiniMax API error 500 Internal Server Error: "
        ));
        assert!(worth_another_try(
            "MiniMax JSON parse failed: EOF while parsing"
        ));

        // A key the host rejects will be rejected on the second try too, and
        // the two seconds of waiting would only delay the message.
        assert!(!worth_another_try(
            "MiniMax API error 401 Unauthorized: invalid api key"
        ));
        assert!(!worth_another_try("MiniMax API error 403 Forbidden: "));
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

    #[test]
    fn reads_the_llm_status_word() {
        let page = |word: &str| format!("<html>Large Language Models <span>{word}</span></html>");
        assert_eq!(parse_status_page(&page("Operational")), "none");
        assert_eq!(parse_status_page(&page("Degraded")), "minor");
        assert_eq!(parse_status_page(&page("Partial Outage")), "major");
        // The earlier build folded "major outage" into major, not critical.
        assert_eq!(parse_status_page(&page("Major Outage")), "major");
        assert_eq!(parse_status_page(&page("Under Maintenance")), "maintenance");
    }

    #[test]
    fn a_healthy_page_says_so() {
        assert_eq!(
            parse_status_page("<html><body>All Systems Operational</body></html>"),
            "none"
        );
    }

    /// A page this build cannot read says unknown — never a silent "healthy".
    #[test]
    fn an_unreadable_page_is_unknown() {
        assert_eq!(parse_status_page(""), "unknown");
        assert_eq!(parse_status_page("<html>nothing here</html>"), "unknown");
    }

    /// The word nearest the anchor answers, the way the non-greedy regex did.
    #[test]
    fn the_first_word_after_the_anchor_wins() {
        assert_eq!(
            parse_status_page("Large Language Models Degraded ... Major Outage"),
            "minor"
        );
    }

    /// A body carrying an in-band failure must report its reason — that is the
    /// message the user needs, not "no usage windows".
    #[test]
    fn an_in_band_failure_reports_its_reason() {
        let refused = serde_json::json!({
            "base_resp": {
                "status_code": 1004,
                "status_msg": "login fail: Please carry the API secret key",
            }
        });
        let error = read_quota(&refused, 0).expect_err("a failure is an error");
        assert!(error.contains("1004"), "{error}");
        assert!(error.contains("login fail"), "{error}");

        // A string code reads the same way, and code zero still reads windows.
        let refused = serde_json::json!({
            "base_resp": { "status_code": "1004", "status_msg": "cookie is missing" }
        });
        let error = read_quota(&refused, 0).expect_err("a failure is an error");
        assert!(error.contains("cookie is missing"), "{error}");

        let ok = serde_json::json!({
            "base_resp": { "status_code": 0 },
            "model_remains": [{ "model_name": "general",
                "current_interval_remaining_percent": 10.0 }]
        });
        assert!(read_quota(&ok, 0).is_ok());
    }
}
