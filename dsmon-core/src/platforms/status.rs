//! DeepSeek service health, read from the public status page.
//!
//! The page is served by FlashDuty either way, but the two addresses do not
//! answer the same thing. `status.flashcat.cloud/deepseek` hands the browser an
//! application shell and fetches the components afterwards; what its HTML holds
//! is FlashDuty's own `Open API` component, so reading it made every reading
//! `operational` — a status page that could never report a fault. The vendor's
//! own address, `status.deepseek.com`, carries the components in the HTML:
//! `DeepSeek V4.1 Flash API服务(API Service)`, `DeepSeek V4 Pro API服务(API
//! Service)`, the chat and search services, each with its status. (Found
//! 2026-09-15, after the older address had been reporting "none" for a while.)

use std::time::Duration;

use super::http_client;

const STATUS_URL: &str = "https://status.deepseek.com";

/// Fetches and normalises the status of the API component.
///
/// Any failure yields `unknown`; the interface treats that as "no information"
/// rather than as an outage.
pub fn fetch(http_proxy: &str) -> String {
    let Ok(client) = http_client(Duration::from_secs(10), http_proxy) else {
        return "unknown".to_owned();
    };
    fetch_component_status(&client)
        .unwrap_or("unknown")
        .to_owned()
}

fn fetch_component_status(client: &reqwest::blocking::Client) -> Option<&'static str> {
    let html = client
        .get(STATUS_URL)
        .header("Accept", "text/html,*/*")
        .header("User-Agent", "Mozilla/5.0")
        .send()
        .ok()?
        .error_for_status()
        .ok()?
        .text()
        .ok()?;
    Some(parse_component_status(&html))
}

/// Picks the worst status among the components whose name mentions the API.
fn parse_component_status(html: &str) -> &'static str {
    let unescaped = html.replace("\\\"", "\"");
    unescaped
        .split("\"name\"")
        .skip(1)
        .filter_map(|part| {
            let name = json_string_after_key(part, "")?;
            if name.to_ascii_lowercase().contains("api") {
                json_string_after_key(part, "\"status\"").map(normalize)
            } else {
                None
            }
        })
        .max_by_key(|status| rank(status))
        .unwrap_or("none")
}

/// Reads the value of the next JSON string that follows `key`.
fn json_string_after_key<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    let text = if key.is_empty() {
        text
    } else {
        &text[text.find(key)? + key.len()..]
    };
    let start = text[text.find(':')? + 1..].trim_start().strip_prefix('"')?;
    start.split('"').next()
}

/// Folds the status page's vocabulary into six levels.
pub fn normalize(value: &str) -> &'static str {
    match value {
        "none" | "operational" => "none",
        "minor" | "degraded" | "degraded_performance" => "minor",
        "major" | "partial_outage" => "major",
        "critical" | "full_outage" | "major_outage" => "critical",
        "maintenance" | "under_maintenance" => "maintenance",
        _ => "unknown",
    }
}

/// Severity ordering, used to pick the worst component.
pub fn rank(status: &str) -> u8 {
    match status {
        "maintenance" => 1,
        "minor" => 2,
        "major" => 3,
        "critical" => 4,
        _ => 0,
    }
}

/// Whether the status means something is wrong.
pub fn is_degraded(status: &str) -> bool {
    matches!(status, "maintenance" | "minor" | "major" | "critical")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folds_status_vocabulary() {
        assert_eq!(normalize("operational"), "none");
        assert_eq!(normalize("degraded_performance"), "minor");
        assert_eq!(normalize("partial_outage"), "major");
        assert_eq!(normalize("major_outage"), "critical");
        assert_eq!(normalize("under_maintenance"), "maintenance");
        assert_eq!(normalize("something_new"), "unknown");
    }

    #[test]
    fn ranks_severity() {
        assert!(rank("critical") > rank("major"));
        assert!(rank("major") > rank("minor"));
        assert!(rank("minor") > rank("none"));
        assert_eq!(rank("unknown"), 0);
    }

    #[test]
    fn flags_degraded_states() {
        assert!(!is_degraded("none"));
        assert!(!is_degraded("unknown"));
        assert!(is_degraded("maintenance"));
        assert!(is_degraded("critical"));
    }

    #[test]
    fn picks_the_worst_api_component() {
        let html = r#"{"components":[
            {"name":"API","status":"operational"},
            {"name":"Web Chat","status":"major_outage"},
            {"name":"API (beta)","status":"degraded_performance"}
        ]}"#;
        assert_eq!(parse_component_status(html), "minor");

        let escaped = html.replace("\"", "\\\"");
        assert_eq!(parse_component_status(&escaped), "minor");
    }

    /// The names the page really carries: two API components, and two more that
    /// are not APIs — one of which (`Chatservice`) contains the letters by
    /// accident, and one of which is a good deal worse than the APIs are.
    #[test]
    fn reads_the_components_the_deepseek_page_carries() {
        let html = r#"{"components":[
            {"name":"对话服务(Chat Service)","status":"partial_outage"},
            {"name":"DeepSeek V4.1 Flash API服务(API Service)","status":"operational"},
            {"name":"DeepSeek V4 Pro API服务(API Service)","status":"degraded"},
            {"name":"搜索服务(Search Service)","status":"operational"}
        ]}"#;
        assert_eq!(parse_component_status(html), "minor");

        let healthy = html.replace("\"degraded\"", "\"operational\"");
        assert_eq!(parse_component_status(&healthy), "none");
    }

    #[test]
    fn reports_none_when_no_api_component_is_listed() {
        let html = r#"{"components":[{"name":"Dashboard","status":"major_outage"}]}"#;
        assert_eq!(parse_component_status(html), "none");
    }
}
