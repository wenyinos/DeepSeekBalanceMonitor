//! Kimi (Moonshot) balance endpoint.
//!
//! Two regions, two hosts: a key issued for one is rejected by the other, so the
//! endpoint follows the platform the user configured.

use std::time::Duration;

use reqwest::StatusCode;
use serde::Deserialize;

use super::http_client;
use crate::catalog;
use crate::model::{deserialize_number, Balance, Balances};

const CHINA: &str = "https://api.moonshot.cn/v1/users/me/balance";
const GLOBAL: &str = "https://api.moonshot.ai/v1/users/me/balance";

/// Reads the account balance. `platform` selects the region.
pub fn fetch_balance(platform: &str, api_key: &str, proxy: &str) -> Result<Balances, String> {
    let url = match platform {
        "kimi_token_cn" => CHINA,
        "kimi_token_global" => GLOBAL,
        other => return Err(format!("Kimi has no endpoint for {other}")),
    };

    let client = http_client(Duration::from_secs(10), proxy)?;
    let response = client
        .get(url)
        .header("Accept", "application/json")
        .bearer_auth(api_key.trim())
        .send()
        .map_err(|error| format!("Kimi request failed: {error}"))?;

    if response.status() == StatusCode::UNAUTHORIZED {
        return Err("Invalid API key (401 Unauthorized); a key is bound to its region".to_owned());
    }

    let payload: KimiResponse = response
        .error_for_status()
        .map_err(|error| error.to_string())?
        .json()
        .map_err(|error| format!("Kimi JSON parse failed: {error}"))?;

    if payload.code != 0 || !payload.status {
        return Err(format!(
            "Kimi API error {}: {}",
            payload.code,
            payload
                .scode
                .or(payload.message)
                .unwrap_or_else(|| "unknown".to_owned()),
        ));
    }

    let data = payload
        .data
        .ok_or_else(|| "Kimi returned no balance data".to_owned())?;

    let mut balances = Balances::new();
    balances.insert(
        catalog::default_currency(platform).to_owned(),
        Balance {
            total_balance: data.available_balance,
            granted_balance: data.voucher_balance,
            topped_up_balance: data.cash_balance,
        },
    );
    Ok(balances)
}

#[derive(Debug, Deserialize)]
struct KimiResponse {
    #[serde(default)]
    code: i64,
    #[serde(default)]
    status: bool,
    #[serde(default)]
    scode: Option<String>,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    data: Option<KimiData>,
}

#[derive(Debug, Deserialize)]
struct KimiData {
    #[serde(default, deserialize_with = "deserialize_number")]
    available_balance: f64,
    #[serde(default, deserialize_with = "deserialize_number")]
    voucher_balance: f64,
    #[serde(default, deserialize_with = "deserialize_number")]
    cash_balance: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_the_balance_fields() {
        let payload = r#"{
            "code": 0,
            "status": true,
            "data": {
                "available_balance": 42.5,
                "voucher_balance": 2.5,
                "cash_balance": 40.0
            }
        }"#;
        let parsed: KimiResponse = serde_json::from_str(payload).unwrap();
        assert_eq!(parsed.code, 0);
        assert!(parsed.status);
        let data = parsed.data.unwrap();
        assert_eq!(data.available_balance, 42.5);
        assert_eq!(data.voucher_balance, 2.5);
        assert_eq!(data.cash_balance, 40.0);
    }

    #[test]
    fn accepts_string_amounts() {
        let payload = r#"{"code":0,"status":true,"data":{"available_balance":"7.25","voucher_balance":"0","cash_balance":"7.25"}}"#;
        let parsed: KimiResponse = serde_json::from_str(payload).unwrap();
        assert_eq!(parsed.data.unwrap().available_balance, 7.25);
    }

    #[test]
    fn unknown_platforms_are_rejected() {
        assert!(fetch_balance("nonsense", "key", "").is_err());
    }
}
