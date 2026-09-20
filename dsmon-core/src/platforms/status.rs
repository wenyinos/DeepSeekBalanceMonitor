//! DeepSeek service health, read from the status page's RSS feed.
//!
//! The page is a Next.js shell, and what its HTML carries is the history of the
//! events — each with the components it touched *at the time*, most of them
//! long over. Read as the current state, an event the vendor had resolved
//! answered for a service that was fine, and the answer it gave was the worst
//! one anywhere in that history: the 2.1.2 build reported a full outage for an
//! account it could still query. The feed at `/feed.rss` — the same FlashDuty
//! system, in a format that does not change shape — states the one thing worth
//! knowing, which is the status of the most recent event.
//!
//! The feed carries no severity. An event being worked on reads
//! `investigating`, `identified` or `monitoring`; a closed one reads
//! `resolved`. An open event is therefore reported as `minor` — something is
//! wrong and the vendor is on it — rather than guessing at one of the four
//! levels the interface knows from a feed that does not name one.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use super::http_client;

const FEED_URL: &str = "https://status.deepseek.com/feed.rss";

/// Whether the previous attempt got through, so a page that stays unreadable is
/// logged once rather than on every poll.
static REACHED: AtomicBool = AtomicBool::new(true);

/// Fetches the status of the most recent event.
///
/// Any failure yields `unknown`; the interface treats that as "no information"
/// rather than as an outage. The failure, and the recovery from one, is written
/// to the log: a silent `unknown` cannot be told apart from a status page that
/// answered nothing.
pub fn fetch(http_proxy: &str) -> String {
    let Ok(client) = http_client(Duration::from_secs(10), http_proxy) else {
        return "unknown".to_owned();
    };
    match fetch_feed(&client) {
        Ok(feed) => {
            if !REACHED.swap(true, Ordering::Relaxed) {
                let _ = crate::storage::log_line("The status page answers again.");
            }
            latest_status(&feed).to_owned()
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

fn fetch_feed(client: &reqwest::blocking::Client) -> Result<String, String> {
    client
        .get(FEED_URL)
        .header("Accept", "application/rss+xml, application/xml, */*")
        .header("User-Agent", "Mozilla/5.0")
        .send()
        .map_err(|error| error.to_string())?
        .error_for_status()
        .map_err(|error| error.to_string())?
        .text()
        .map_err(|error| error.to_string())
}

/// The status of the newest event, which the feed lists first.
fn latest_status(feed: &str) -> &'static str {
    first_item(feed)
        .and_then(status_of)
        .map(normalize)
        .unwrap_or("unknown")
}

/// The text between the first `<item>` and its closing tag.
fn first_item(feed: &str) -> Option<&str> {
    let start = feed.find("<item>")? + "<item>".len();
    let end = start + feed[start..].find("</item>")?;
    Some(&feed[start..end])
}

/// The value of the description's `Status:` field, HTML-escaped as it arrives.
fn status_of(item: &str) -> Option<&str> {
    const MARKER: &str = "Status:&lt;/strong&gt;";
    let rest = item[item.find(MARKER)? + MARKER.len()..].trim_start();
    let end = rest.find(|c: char| !c.is_ascii_alphanumeric())?;
    Some(&rest[..end])
}

/// Folds the feed's vocabulary into the levels the interface shows.
fn normalize(value: &str) -> &'static str {
    match value {
        "resolved" | "postmortem" => "none",
        "investigating" | "identified" | "monitoring" => "minor",
        _ => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The feed as it arrives: newest event first, its status inside the
    /// description, HTML-escaped.
    const FEED: &str = r#"<?xml version="1.0" encoding="UTF-8"?><rss version="2.0"><channel>
        <title>DeepSeek</title>
        <item>
            <title>DeepSeek 网页/API 性能下降（DeepSeek Web/API Degraded Performance</title>
            <link>https://status.deepseek.com/incidents/6985724403287</link>
            <description>&lt;p&gt;&lt;strong&gt;Status:&lt;/strong&gt; resolved&lt;/p&gt;&lt;p&gt;本次问题已解决，服务已恢复。&lt;/p&gt;</description>
            <guid>urn:flashduty:change:6985724403287</guid>
            <pubDate>Tue, 15 Sep 2026 19:30:49 +0800</pubDate>
        </item>
        <item>
            <title>搜索服务异常（Search service error）</title>
            <description>&lt;p&gt;&lt;strong&gt;Status:&lt;/strong&gt; resolved&lt;/p&gt;</description>
            <pubDate>Fri, 14 Aug 2026 14:34:38 +0800</pubDate>
        </item>
    </channel></rss>"#;

    #[test]
    fn reads_the_status_of_the_newest_event() {
        assert_eq!(latest_status(FEED), "none");
    }

    /// The feed lists the newest event first; an older one still open is
    /// history, not the state now.
    #[test]
    fn the_newest_event_is_the_one_that_answers() {
        let feed = r#"<rss><channel>
            <item><description>&lt;strong&gt;Status:&lt;/strong&gt; resolved&lt;/p&gt;</description></item>
            <item><description>&lt;strong&gt;Status:&lt;/strong&gt; monitoring&lt;/p&gt;</description></item>
        </channel></rss>"#;
        assert_eq!(latest_status(feed), "none");
    }

    /// An event that is still being worked on is not good news. Reporting it as
    /// healthy is the failure this whole module exists to avoid.
    #[test]
    fn an_open_event_is_not_reported_as_healthy() {
        let open = FEED.replace(
            "Status:&lt;/strong&gt; resolved",
            "Status:&lt;/strong&gt; investigating",
        );
        assert_eq!(latest_status(&open), "minor");
    }

    #[test]
    fn folds_the_feed_vocabulary() {
        assert_eq!(normalize("resolved"), "none");
        assert_eq!(normalize("postmortem"), "none");
        assert_eq!(normalize("investigating"), "minor");
        assert_eq!(normalize("identified"), "minor");
        assert_eq!(normalize("monitoring"), "minor");
        // A word this build does not know is neither a fault nor a clean bill.
        assert_eq!(normalize("something_new"), "unknown");
    }

    #[test]
    fn a_feed_it_cannot_read_is_unknown() {
        assert_eq!(latest_status(""), "unknown");
        assert_eq!(latest_status("<rss><channel></channel></rss>"), "unknown");
        assert_eq!(
            latest_status("<item><title>no status here</title></item>"),
            "unknown"
        );
    }
}
