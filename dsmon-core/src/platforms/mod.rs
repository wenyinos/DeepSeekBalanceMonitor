//! Per-platform API clients and the HTTP plumbing they share.

pub mod command_code;
pub mod deepseek;
pub mod glm;
pub mod kimi;
pub mod minimax;
pub mod opencode_go;
pub mod openrouter;
pub mod status;
pub mod stepfun;

use std::time::Duration;

use reqwest::Proxy;

use crate::config::AppConfig;

/// Builds a blocking HTTP client with the given timeout and optional proxy.
pub fn http_client(
    timeout: Duration,
    http_proxy: &str,
) -> Result<reqwest::blocking::Client, String> {
    install_tls_provider();
    let mut builder = reqwest::blocking::Client::builder().timeout(timeout);
    let proxy = http_proxy.trim();
    if !proxy.is_empty() {
        builder = builder.proxy(Proxy::all(proxy).map_err(|error| error.to_string())?);
    }
    builder.build().map_err(|error| error.to_string())
}

/// Installs the TLS provider every client in this crate is built with.
///
/// DeepSeek's status page sits behind an edge that resets the connection of any
/// handshake whose first key exchange group is not the post-quantum hybrid
/// `X25519MLKEM768` — a plain `X25519` hello, and even one offering the hybrid
/// further down the list, is answered with a reset before the server says
/// anything. reqwest's own provider is ring, which has no post-quantum group at
/// all, so the status page could not be read from this program and every
/// reading came back `unknown` (found 2026-09-20). `prefer-post-quantum` puts
/// the hybrid first, which is what browsers send and what that edge accepts.
fn install_tls_provider() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        // Only fails if a provider was already installed, which is the same
        // outcome this wants.
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    });
}

/// The proxy to use, honouring the configuration switch.
pub fn effective_proxy(config: &AppConfig) -> &str {
    if config.proxy_enabled {
        config.http_proxy.trim()
    } else {
        ""
    }
}

/// Collapses whitespace and caps the length of an upstream error message.
pub fn sanitize_message(text: &str) -> String {
    let cleaned: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if cleaned.is_empty() {
        "unknown".to_owned()
    } else {
        cleaned.chars().take(120).collect()
    }
}

/// Renders a countdown as `2d 3h 4m`, dropping units that are zero.
pub fn format_reset_seconds(seconds: i64) -> String {
    if seconds <= 0 {
        return "now".to_owned();
    }
    let days = seconds / 86_400;
    let hours = (seconds % 86_400) / 3600;
    let minutes = (seconds % 3600) / 60;

    let mut parts = Vec::new();
    if days > 0 {
        parts.push(format!("{days}d"));
    }
    if hours > 0 {
        parts.push(format!("{hours}h"));
    }
    if minutes > 0 {
        parts.push(format!("{minutes}m"));
    }
    if parts.is_empty() {
        parts.push(format!("{}s", seconds % 60));
    }
    parts.join(" ")
}

/// Percent-encodes a query parameter value.
pub fn urlencode(value: &str) -> String {
    value
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (byte as char).to_string()
            }
            _ => format!("%{byte:02X}"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_countdowns() {
        assert_eq!(format_reset_seconds(0), "now");
        assert_eq!(format_reset_seconds(-5), "now");
        assert_eq!(format_reset_seconds(45), "45s");
        assert_eq!(format_reset_seconds(3_600), "1h");
        assert_eq!(format_reset_seconds(90_061), "1d 1h 1m");
    }

    #[test]
    fn sanitizes_error_messages() {
        assert_eq!(sanitize_message("  "), "unknown");
        assert_eq!(sanitize_message("bad\n  request"), "bad request");
        assert_eq!(sanitize_message(&"x".repeat(500)).chars().count(), 120);
    }

    #[test]
    fn percent_encodes_query_values() {
        assert_eq!(urlencode("org-1"), "org-1");
        assert_eq!(urlencode("a b&c"), "a%20b%26c");
    }

    #[test]
    fn proxy_is_used_only_when_enabled() {
        let mut config = AppConfig::default();
        config.http_proxy = "http://127.0.0.1:7890".to_owned();
        assert_eq!(effective_proxy(&config), "");
        config.proxy_enabled = true;
        assert_eq!(effective_proxy(&config), "http://127.0.0.1:7890");
    }
}

/// Seconds until an epoch, accepting both second and millisecond values: the
/// platforms are not consistent about which they send.
pub(crate) fn epoch_to_reset_seconds(epoch: Option<f64>, now: i64) -> i64 {
    epoch
        .map(|value| {
            let seconds = if value >= 100_000_000_000.0 {
                (value / 1000.0) as i64
            } else {
                value as i64
            };
            (seconds - now).max(0)
        })
        .unwrap_or(0)
}
