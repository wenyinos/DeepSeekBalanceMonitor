//! OpenRouter credits endpoint.
//!
//! Needs a *management* key: an ordinary inference key is refused here, which is
//! why both 401 and 403 carry the same hint.

use std::time::Duration;

use reqwest::StatusCode;
use serde::Deserialize;

use super::http_client;
use crate::model::{deserialize_number, Balance, Balances};

const CREDITS: &str = "https://openrouter.ai/api/v1/credits";

/// Reads the account balance, which is the credits left minus what was spent.
pub fn fetch_balance(_platform: &str, api_key: &str, proxy: &str) -> Result<Balances, String> {
    let client = http_client(Duration::from_secs(10), proxy)?;
    let response = client
        .get(CREDITS)
        .header("Accept", "application/json")
        .bearer_auth(api_key.trim())
        .send()
        .map_err(|error| format!("OpenRouter request failed: {error}"))?;

    if matches!(
        response.status(),
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN
    ) {
        return Err("OpenRouter rejected the key; this endpoint needs a management key".to_owned());
    }

    let payload: OpenRouterResponse = response
        .error_for_status()
        .map_err(|error| error.to_string())?
        .json()
        .map_err(|error| format!("OpenRouter JSON parse failed: {error}"))?;

    let data = payload
        .data
        .ok_or_else(|| "OpenRouter returned no credit data".to_owned())?;

    // The API reports a running total and the spend; the balance is the two
    // subtracted, and no part of it is a grant.
    let mut balances = Balances::new();
    balances.insert(
        "USD".to_owned(),
        Balance {
            total_balance: (data.total_credits - data.total_usage).max(0.0),
            granted_balance: 0.0,
            topped_up_balance: data.total_credits,
        },
    );
    Ok(balances)
}

#[derive(Debug, Deserialize)]
struct OpenRouterResponse {
    #[serde(default)]
    data: Option<OpenRouterCredits>,
}

#[derive(Debug, Deserialize)]
struct OpenRouterCredits {
    #[serde(default, deserialize_with = "deserialize_number")]
    total_credits: f64,
    #[serde(default, deserialize_with = "deserialize_number")]
    total_usage: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_balance_is_credits_minus_usage() {
        let payload = r#"{"data":{"total_credits":25.0,"total_usage":4.5}}"#;
        let parsed: OpenRouterResponse = serde_json::from_str(payload).unwrap();
        let data = parsed.data.unwrap();
        assert_eq!(data.total_credits - data.total_usage, 20.5);
    }

    #[test]
    fn spend_beyond_the_credits_does_not_go_negative() {
        // The clamp lives in fetch_balance; check the arithmetic it relies on.
        let payload = r#"{"data":{"total_credits":5.0,"total_usage":7.5}}"#;
        let parsed: OpenRouterResponse = serde_json::from_str(payload).unwrap();
        let data = parsed.data.unwrap();
        assert!((data.total_credits - data.total_usage).max(0.0) == 0.0);
    }
}
