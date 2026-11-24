//! Huawei Cloud CodeArts usage.
//!
//! The CodeArts gateway is not a Bearer-token API: every request is signed with
//! Huawei's `SDK-HMAC-SHA256` scheme using a permanent AK/SK pair. This client
//! implements only the signed `GET /snap-manager/v1/statistics/plugin` call —
//! the same endpoint the CodeArts CLI and the opencode-codearts proxy read —
//! so the application does not need that proxy, its environment variables or
//! its web login.
//!
//! The endpoint reports credit buckets (total, basic, on-demand, bonus). Each
//! becomes a quota window whose `used`/`cap` are credits, which is what the
//! subscription card draws.

use std::collections::BTreeMap;
use std::time::Duration;

use chrono::Utc;
use ring::digest::{digest, SHA256};
use ring::hmac;
use serde_json::Value;

use super::{http_client, sanitize_message};
use crate::model::{PackageQuota, QuotaWindow};

const HOST: &str = "snap-access.cn-north-4.myhuaweicloud.com";
const PATH: &str = "/snap-manager/v1/statistics/plugin";
const URL: &str =
    "https://snap-access.cn-north-4.myhuaweicloud.com/snap-manager/v1/statistics/plugin";

/// Metric name in the answer → window name in the quota map.
const METRICS: [(&str, &str); 4] = [
    ("usageTotalPackageCredit", "total"),
    ("usageBasicPackageCredit", "basic"),
    ("usageOnDemandPackageCredit", "on_demand"),
    ("usageBonusPackageCredit", "bonus"),
];

/// Splits the stored secret into the AK and SK the signature needs.
///
/// The settings page stores them as two lines; `AK:SK` is accepted as well so
/// a value pasted from an existing tool still works. Both halves are required.
pub fn split_credentials(raw: &str) -> Result<(String, String), String> {
    let text = raw.trim();
    let pair = text.split_once('\n').or_else(|| text.split_once(':'));
    let Some((ak, sk)) = pair else {
        return Err("CodeArts needs an AK and an SK (two lines, or AK:SK).".to_owned());
    };
    let (ak, sk) = (ak.trim(), sk.trim());
    if ak.is_empty() || sk.is_empty() {
        return Err("CodeArts needs both an AK and an SK.".to_owned());
    }
    Ok((ak.to_owned(), sk.to_owned()))
}

/// Reads the account's credit buckets.
///
/// `credentials` is the stored AK/SK pair (two lines). The request is signed
/// here; the HTTP client only carries it, so the configured proxy still works.
pub fn fetch_quota(credentials: &str, http_proxy: &str) -> Result<PackageQuota, String> {
    let (ak, sk) = split_credentials(credentials)?;
    let client = http_client(Duration::from_secs(15), http_proxy)?;

    let date = Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
    let signed = signed_headers(&ak, &sk, &date, &sha256_hex(b""));

    let mut request = client.get(URL);
    for (name, value) in &signed {
        // reqwest sets `host` itself; sending it again is not needed and some
        // proxies dislike the duplicate.
        if *name == "host" {
            continue;
        }
        request = request.header(*name, value.as_str());
    }

    let response = request
        .send()
        .map_err(|error| format!("CodeArts request failed: {error}"))?;
    let status = response.status();
    let text = response
        .text()
        .map_err(|error| format!("CodeArts response could not be read: {error}"))?;

    if !status.is_success() {
        let message = business_error(&text).unwrap_or_else(|| sanitize_message(&text));
        return Err(format!("CodeArts API error {status}: {message}"));
    }

    let value: Value = serde_json::from_str(&text)
        .map_err(|error| format!("CodeArts JSON parse failed: {error}"))?;
    usage_from_value(&value)
}

/// Signs one GET. The returned map carries exactly the headers whose values
/// were hashed, plus the `authorization` header.
fn signed_headers(
    ak: &str,
    sk: &str,
    date: &str,
    payload_hash: &str,
) -> BTreeMap<&'static str, String> {
    let mut headers = BTreeMap::new();
    headers.insert("agent-type", "PromptCenter".to_owned());
    headers.insert("content-type", "application/json".to_owned());
    headers.insert("host", HOST.to_owned());
    headers.insert("x-language", "zh-cn".to_owned());
    headers.insert("x-sdk-content-sha256", payload_hash.to_owned());
    headers.insert("x-sdk-date", date.to_owned());

    let signed_names = headers.keys().copied().collect::<Vec<_>>().join(";");
    let canonical = canonical_request("GET", &format!("{PATH}/"), "", &headers, payload_hash);
    let canonical_hash = sha256_hex(canonical.as_bytes());
    let string_to_sign = format!("SDK-HMAC-SHA256\n{date}\n{canonical_hash}");
    let signature = hmac_hex(sk.as_bytes(), string_to_sign.as_bytes());

    headers.insert(
        "authorization",
        format!("SDK-HMAC-SHA256 Access={ak},SignedHeaders={signed_names},Signature={signature}"),
    );
    headers
}

/// The layout the Huawei SDK hashes: method, URI, query, headers, the blank
/// line that ends them, the signed-header list, and the payload hash.
fn canonical_request(
    method: &str,
    uri: &str,
    query: &str,
    headers: &BTreeMap<&'static str, String>,
    payload_hash: &str,
) -> String {
    let lines = headers
        .iter()
        .map(|(name, value)| format!("{name}:{value}"))
        .collect::<Vec<_>>()
        .join("\n");
    let names = headers.keys().copied().collect::<Vec<_>>().join(";");
    format!("{method}\n{uri}\n{query}\n{lines}\n\n{names}\n{payload_hash}")
}

fn sha256_hex(data: &[u8]) -> String {
    hex(digest(&SHA256, data).as_ref())
}

fn hmac_hex(key: &[u8], data: &[u8]) -> String {
    let key = hmac::Key::new(hmac::HMAC_SHA256, key);
    hex(hmac::sign(&key, data).as_ref())
}

fn hex(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        text.push_str(&format!("{byte:02x}"));
    }
    text
}

/// The gateway puts business failures in `code`/`message` with HTTP 200, and
/// sometimes in `msg`.
fn business_error(text: &str) -> Option<String> {
    let value: Value = serde_json::from_str(text).ok()?;
    let code = value.get("code")?.as_i64()?;
    if code == 0 {
        return None;
    }
    let message = value
        .get("message")
        .or_else(|| value.get("msg"))
        .and_then(Value::as_str)
        .unwrap_or("request rejected");
    Some(format!("{message} (code {code})"))
}

/// Maps the statistics answer to the quota map.
///
/// The envelope is `{code, message, data}`; a plain data object is tolerated
/// too. Empty buckets are dropped: a plan that has no bonus credits should not
/// draw an empty bar.
pub fn usage_from_value(value: &Value) -> Result<PackageQuota, String> {
    let root = value.get("data").unwrap_or(value);

    let code = value.get("code").and_then(Value::as_i64).unwrap_or(0);
    if code != 0 {
        let message = value
            .get("message")
            .or_else(|| value.get("msg"))
            .and_then(Value::as_str)
            .unwrap_or("request rejected");
        return Err(format!("CodeArts API error: {message} (code {code})"));
    }

    let metrics = root
        .get("metrics")
        .and_then(Value::as_array)
        .ok_or_else(|| "CodeArts usage response has no metrics.".to_owned())?;

    let mut quota = PackageQuota::new();
    for metric in metrics {
        let Some(name) = metric.get("name").and_then(Value::as_str) else {
            continue;
        };
        let Some((_, window)) = METRICS.iter().find(|(metric, _)| *metric == name) else {
            continue;
        };
        if metric.get("show").and_then(Value::as_bool) == Some(false) {
            continue;
        }

        let limit = metric
            .get("package_credit_amount")
            .map(number)
            .unwrap_or(0.0);
        if limit <= 0.0 {
            continue;
        }
        let remain = metric.get("package_credit_remain").map(number);
        let used = metric
            .get("package_credit_used")
            .map(number)
            .unwrap_or_else(|| (limit - remain.unwrap_or(0.0)).max(0.0));
        let used = used.clamp(0.0, limit);

        quota.insert((*window).to_owned(), QuotaWindow::from_pool(used, limit, 0));
    }

    if quota.is_empty() {
        return Err("CodeArts returned no credit buckets for this account.".to_owned());
    }
    Ok(quota)
}

fn number(value: &Value) -> f64 {
    match value {
        Value::Number(number) => number.as_f64().unwrap_or(0.0),
        Value::String(text) => text.trim().parse().unwrap_or(0.0),
        _ => 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn hmac_matches_rfc_4231_case_2() {
        assert_eq!(
            hmac_hex(b"Jefe", b"what do ya want for nothing?"),
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
    }

    #[test]
    fn sha256_of_empty_payload_is_the_known_value() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn canonical_request_follows_the_sdk_layout() {
        let mut headers = BTreeMap::new();
        headers.insert("content-type", "application/json".to_owned());
        headers.insert("host", HOST.to_owned());
        headers.insert("x-sdk-date", "20261122T120000Z".to_owned());
        assert_eq!(
            canonical_request("GET", "/v1/model/builtin/", "", &headers, "abc"),
            [
                "GET",
                "/v1/model/builtin/",
                "",
                "content-type:application/json",
                "host:snap-access.cn-north-4.myhuaweicloud.com",
                "x-sdk-date:20261122T120000Z",
                "",
                "content-type;host;x-sdk-date",
                "abc",
            ]
            .join("\n")
        );
    }

    #[test]
    fn signing_is_deterministic_and_names_the_signed_headers() {
        let first = signed_headers("AKID", "SECRET", "20261122T120000Z", "abc");
        let second = signed_headers("AKID", "SECRET", "20261122T120000Z", "abc");
        assert_eq!(first.get("authorization"), second.get("authorization"));
        let authorization = first.get("authorization").unwrap();
        assert!(authorization.starts_with("SDK-HMAC-SHA256 Access=AKID,SignedHeaders="));
        assert!(authorization.contains("x-sdk-date"));
        assert!(authorization.contains("Signature="));
    }

    #[test]
    fn splits_two_lines_and_a_colon_pair() {
        assert_eq!(
            split_credentials("AK\nSK").unwrap(),
            ("AK".to_owned(), "SK".to_owned())
        );
        assert_eq!(
            split_credentials(" AK : SK ").unwrap(),
            ("AK".to_owned(), "SK".to_owned())
        );
        assert!(split_credentials("AK").is_err());
        assert!(split_credentials("\nSK").is_err());
    }

    fn statistics() -> Value {
        json!({
            "package": {"package_name_cn": "体验版"},
            "metrics": [
                {
                    "name": "usageTotalPackageCredit",
                    "package_credit_amount": 14600.0,
                    "package_credit_used": 137.87,
                    "package_credit_remain": 14462.13,
                    "show": true
                },
                {
                    "name": "usageBasicPackageCredit",
                    "package_credit_amount": 500.0,
                    "package_credit_used": 0,
                    "package_credit_remain": 500.0,
                    "show": true
                },
                {
                    "name": "usageOnDemandPackageCredit",
                    "package_credit_amount": 0,
                    "package_credit_used": 0,
                    "package_credit_remain": 0,
                    "show": true
                },
                {
                    "name": "usageTokenChatMessages",
                    "package_credit_amount": 0,
                    "package_credit_used": 0,
                    "package_credit_remain": 0,
                    "show": true
                }
            ]
        })
    }

    #[test]
    fn maps_the_credit_buckets() {
        let quota = usage_from_value(&statistics()).unwrap();
        assert_eq!(quota.len(), 2);
        let total = quota.get("total").unwrap();
        assert!((total.usage_percent - 0.944).abs() < 0.01);
        assert_eq!(total.cap, Some(14600.0));
        assert_eq!(total.used, Some(137.87));
        assert_eq!(quota.get("basic").unwrap().cap, Some(500.0));
        assert!(!quota.contains_key("on_demand"));
    }

    #[test]
    fn a_business_error_is_reported() {
        let value = json!({"code": 4003, "message": "用户禁用"});
        assert_eq!(
            usage_from_value(&value).unwrap_err(),
            "CodeArts API error: 用户禁用 (code 4003)"
        );
    }

    #[test]
    fn an_account_without_credits_is_an_error() {
        let value = json!({"metrics": []});
        assert!(usage_from_value(&value).is_err());
    }
}
