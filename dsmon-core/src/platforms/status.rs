//! DeepSeek service health, read from the FlashDuty status page.
//!
//! The canonical domain is tried first; FlashDuty's backend host serves the
//! same page and is the only one reachable from some networks (the vanity
//! domain's TLS handshake is reset there). The page used before —
//! `status.flashcat.cloud/deepseek` — is FlashDuty's OWN status page: it
//! carries no DeepSeek data at all, yet the parser still answered
//! "operational", so an outage was never reported.
//!
//! The page ships its data as Next.js RSC chunks, and what matters is the
//! worst status among its API-service components in the *active* changes. The
//! normalization table is the shared contract in `docs/INTERFACES.md` §7.4.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use super::http_client;

const STATUS_URLS: [&str; 2] = [
    "https://status.deepseek.com/",
    "https://cn.statuspage.flashduty.com/deepseek",
];

/// Whether the previous attempt got through, so a page that stays unreadable
/// is logged once rather than on every poll.
static REACHED: AtomicBool = AtomicBool::new(true);

/// Fetches the worst status among the API-service components.
///
/// Any failure yields `unknown`; the interface treats that as "no
/// information" rather than as an outage. The failure, and the recovery from
/// one, is written to the log: a silent `unknown` cannot be told apart from a
/// status page that answered nothing.
pub fn fetch(http_proxy: &str) -> String {
    match fetch_worst(http_proxy) {
        Ok(status) => {
            if !REACHED.swap(true, Ordering::Relaxed) {
                let _ = crate::storage::log_line("The status page answers again.");
            }
            status
        }
        Err(error) => {
            if REACHED.swap(false, Ordering::Relaxed) {
                let _ = crate::storage::log_line(&format!(
                    "The status page could not be read ({error}); the service status reads unknown."
                ));
            }
            "unknown".to_owned()
        }
    }
}

/// Tries the canonical domain, then FlashDuty's backend host.
fn fetch_worst(http_proxy: &str) -> Result<String, String> {
    let mut last_error = "no status page answered".to_owned();
    for url in STATUS_URLS {
        let client = http_client(Duration::from_secs(10), http_proxy)?;
        let fetched = client
            .get(url)
            .header("Accept", "text/html,*/*")
            .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64)")
            .send()
            .and_then(|response| response.error_for_status())
            .and_then(|response| response.text());
        match fetched {
            Ok(html) => match parse_status_page(&html) {
                Some(status) => return Ok(status),
                None => last_error = format!("{url} is not DeepSeek's status page"),
            },
            Err(error) => last_error = format!("{url}: {error}"),
        }
    }
    Err(last_error)
}

/// Parses a status page into the interface's indicator.
///
/// Returns `None` when the page cannot be identified as DeepSeek's. A wrong
/// or restructured page must surface as "unknown", never as a silent
/// 服务正常.
fn parse_status_page(html: &str) -> Option<String> {
    let text = decode_rsc_payload(html);

    // Identify the page by its API service components: FlashDuty answers
    // unknown paths with its own status page, which has no such components.
    if !has_api_component_name(&text) {
        return None;
    }

    let raw = extract_json_value(&text, "active_changes")?;
    let changes: Vec<serde_json::Value> = serde_json::from_str(raw).ok()?;

    let mut worst = "operational";
    for change in &changes {
        let change_status = change
            .get("status")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        if is_inactive_change(change_status) {
            continue;
        }
        let Some(components) = change
            .get("affected_components")
            .and_then(|value| value.as_array())
        else {
            continue;
        };
        for component in components {
            let name = component
                .get("name")
                .and_then(|value| value.as_str())
                .unwrap_or("");
            if !is_api_component(name) {
                continue;
            }
            let lower = component
                .get("status")
                .and_then(|value| value.as_str())
                .unwrap_or("")
                .to_lowercase();
            if severity(&lower) > severity(worst) {
                // An unrecognized status word counts as degraded, never as
                // "fine"; known ones keep their own name for the mapping.
                worst = known_status(&lower);
            }
        }
    }

    Some(normalize(worst).to_owned())
}

/// Joins the Next.js RSC chunks into one payload with real (unescaped)
/// quotes.
///
/// The page ships its data as `self.__next_f.push([1,"<escaped JSON>"])`
/// script chunks. Inside them every quote is escaped, so the JSON structure
/// is invisible until the string literals are decoded — decoding chunk by
/// chunk keeps nesting intact.
fn decode_rsc_payload(html: &str) -> String {
    const MARKER: &str = "self.__next_f.push([";
    let mut parts = String::new();
    let mut rest = html;
    while let Some(start) = rest.find(MARKER) {
        rest = &rest[start + MARKER.len()..];
        let Some(quote) = rest.find('"') else {
            break;
        };
        let after = &rest[quote..];
        let bytes = after.as_bytes();
        let mut end = 1;
        while end < bytes.len() {
            match bytes[end] {
                b'\\' => end += 2,
                b'"' => break,
                _ => end += 1,
            }
        }
        if end >= bytes.len() {
            break;
        }
        if let Ok(decoded) = serde_json::from_str::<String>(&after[..=end]) {
            parts.push_str(&decoded);
        }
        rest = &after[end + 1..];
    }
    if parts.is_empty() {
        html.to_owned()
    } else {
        parts
    }
}

/// Returns the raw JSON array/object following `"key":` via bracket
/// matching — string-aware and nesting-aware, so it survives a change whose
/// `affected_components` array is nested inside the changes array. A blunt
/// `\[[^\]]*\]` regex truncated exactly that, `json.loads` raised, and a
/// blanket catch turned a real outage into "状态未知".
fn extract_json_value<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    let needle = format!("\"{key}\"");
    let mut from = 0;
    while let Some(offset) = text[from..].find(&needle) {
        let at = from + offset;
        let after = text[at + needle.len()..].trim_start();
        if let Some(rest) = after.strip_prefix(':') {
            let rest = rest.trim_start();
            if let Some(open @ ('[' | '{')) = rest.chars().next() {
                return walk_to_close(rest, open);
            }
        }
        from = at + needle.len();
    }
    None
}

/// The slice from `text`'s leading bracket to its matching close, counting
/// depth and honouring string literals.
fn walk_to_close(text: &str, open: char) -> Option<&str> {
    let close = match open {
        '[' => ']',
        '{' => '}',
        _ => return None,
    };
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escaped = false;
    for (index, c) in text.char_indices() {
        if in_string {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_string = false;
            }
            continue;
        }
        match c {
            '"' => in_string = true,
            _ if c == open => depth += 1,
            _ if c == close => {
                depth -= 1;
                if depth == 0 {
                    return Some(&text[..index + c.len_utf8()]);
                }
            }
            _ => {}
        }
    }
    None
}

/// Whether any `"name": "..."` field names an API component.
fn has_api_component_name(text: &str) -> bool {
    let needle = "\"name\"";
    let mut from = 0;
    while let Some(offset) = text[from..].find(needle) {
        let at = from + offset;
        let after = text[at + needle.len()..].trim_start();
        if let Some(rest) = after.strip_prefix(':') {
            let rest = rest.trim_start();
            if let Some(rest) = rest.strip_prefix('"') {
                if let Some(end) = rest.find('"') {
                    if is_api_component(&rest[..end]) {
                        return true;
                    }
                }
            }
        }
        from = at + needle.len();
    }
    false
}

/// Only API-type components drive the API status. The page names them
/// "DeepSeek V4 Pro API服务(API Service)"; chat, upload and search services
/// are deliberately ignored.
fn is_api_component(name: &str) -> bool {
    let compact: String = name
        .to_lowercase()
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    compact.contains("api服务") || compact.contains("apiservice")
}

/// A change that is already over must not raise the indicator.
fn is_inactive_change(status: &str) -> bool {
    matches!(
        status.to_lowercase().as_str(),
        "resolved" | "completed" | "scheduled"
    )
}

/// Component status -> the interface's indicator, the normalization table of
/// the shared contract (`docs/INTERFACES.md` §7.4).
fn normalize(status: &str) -> &'static str {
    match status {
        "operational" => "none",
        "degraded" | "degraded_performance" => "minor",
        "partial_outage" => "major",
        "full_outage" | "major_outage" => "critical",
        "under_maintenance" => "maintenance",
        _ => "unknown",
    }
}

/// Severity ranking used to pick the worst affected API component.
fn severity(status: &str) -> i32 {
    match status {
        "operational" => 0,
        "under_maintenance" => 1,
        "degraded" | "degraded_performance" => 2,
        "partial_outage" => 3,
        "full_outage" | "major_outage" => 4,
        // An unrecognized status must never read as "fine".
        _ => 2,
    }
}

/// The static form of a status word; anything unrecognized becomes
/// `degraded`, which the mapping reads as `minor`.
fn known_status(status: &str) -> &'static str {
    match status {
        "operational" => "operational",
        "under_maintenance" => "under_maintenance",
        "degraded" => "degraded",
        "degraded_performance" => "degraded_performance",
        "partial_outage" => "partial_outage",
        "full_outage" => "full_outage",
        "major_outage" => "major_outage",
        _ => "degraded",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A page whose RSC chunks carry one active change, with the nested
    /// `affected_components` array that a regex used to truncate.
    fn page_with(change_status: &str, component: &str, component_status: &str) -> String {
        let payload = serde_json::json!({
            "props": {
                "active_changes": [{
                    "status": change_status,
                    "affected_components": [{
                        "name": component,
                        "status": component_status
                    }]
                }],
            }
        })
        .to_string();
        let escaped = serde_json::to_string(&payload).expect("an escaped literal");
        format!("<html><script>self.__next_f.push([1,{escaped}])</script></html>")
    }

    #[test]
    fn reads_the_worst_api_status() {
        assert_eq!(
            parse_status_page(&page_with(
                "monitoring",
                "DeepSeek V4 Pro API服务(API Service)",
                "degraded_performance"
            )),
            Some("minor".to_owned())
        );
        assert_eq!(
            parse_status_page(&page_with(
                "monitoring",
                "DeepSeek V4 Pro API服务(API Service)",
                "major_outage"
            )),
            Some("critical".to_owned())
        );
        assert_eq!(
            parse_status_page(&page_with(
                "identified",
                "DeepSeek V4.1 Flash API服务(API Service)",
                "partial_outage"
            )),
            Some("major".to_owned())
        );
        assert_eq!(
            parse_status_page(&page_with("monitoring", "API Service", "under_maintenance")),
            Some("maintenance".to_owned())
        );
        assert_eq!(
            parse_status_page(&page_with("monitoring", "API服务", "operational")),
            Some("none".to_owned())
        );
    }

    /// A change that is already over is history, not the state now.
    #[test]
    fn a_resolved_change_does_not_raise_the_indicator() {
        assert_eq!(
            parse_status_page(&page_with(
                "resolved",
                "DeepSeek V4 Pro API服务(API Service)",
                "major_outage"
            )),
            Some("none".to_owned())
        );
    }

    /// Non-API components never drive the API status.
    #[test]
    fn chat_components_are_ignored() {
        assert_eq!(
            parse_status_page(&page_with(
                "monitoring",
                "DeepSeek 网页聊天",
                "major_outage"
            )),
            None,
            "a page without API components is not identified"
        );
    }

    /// The page that carries no DeepSeek data at all (FlashDuty's own status
    /// page) must surface as unknown, never as a silent 服务正常.
    #[test]
    fn a_page_without_api_components_is_not_identified() {
        let html = "<html><script>self.__next_f.push([1,\"{\\\"name\\\":\\\"Open API\\\"}\"])</script></html>";
        assert_eq!(parse_status_page(html), None);
        assert_eq!(parse_status_page(""), None);
        assert_eq!(parse_status_page("<html></html>"), None);
    }

    /// An unrecognized status word is not a clean bill: it counts as degraded.
    #[test]
    fn an_unknown_status_word_reads_as_degraded() {
        assert_eq!(
            parse_status_page(&page_with("monitoring", "API服务", "something_new")),
            Some("minor".to_owned())
        );
    }

    /// The bracket walk is string-aware: a `]` inside a component name must
    /// not cut the payload short.
    #[test]
    fn brackets_inside_strings_do_not_truncate_the_payload() {
        assert_eq!(
            parse_status_page(&page_with("monitoring", "API服务 [beta]", "full_outage")),
            Some("critical".to_owned())
        );
    }

    #[test]
    fn folds_the_vocabulary_into_indicators() {
        assert_eq!(normalize("operational"), "none");
        assert_eq!(normalize("degraded"), "minor");
        assert_eq!(normalize("degraded_performance"), "minor");
        assert_eq!(normalize("partial_outage"), "major");
        assert_eq!(normalize("full_outage"), "critical");
        assert_eq!(normalize("major_outage"), "critical");
        assert_eq!(normalize("under_maintenance"), "maintenance");
        assert_eq!(normalize("something_new"), "unknown");
    }

    #[test]
    fn severity_ranks_unknown_words_as_degraded() {
        assert!(severity("operational") < severity("under_maintenance"));
        assert!(severity("under_maintenance") < severity("degraded"));
        assert!(severity("degraded") < severity("partial_outage"));
        assert!(severity("partial_outage") < severity("major_outage"));
        assert!(severity("full_outage") > severity("partial_outage"));
        assert_eq!(severity("something_new"), severity("degraded"));
    }
}
