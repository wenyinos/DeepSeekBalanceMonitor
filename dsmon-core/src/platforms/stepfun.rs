//! StepFun account endpoint.
//!
//! Reports the pay-as-you-go balance only: Step Plan subscriptions have no
//! public quota API, which the porting notes call out.

use std::time::Duration;

use reqwest::StatusCode;
use serde::Deserialize;

use super::http_client;
use crate::catalog;
use crate::model::{deserialize_number, Balance, Balances};

const CHINA: &str = "https://api.stepfun.com/v1/accounts";
const GLOBAL: &str = "https://api.stepfun.ai/v1/accounts";

/// Reads the account balance. `platform` selects the region.
pub fn fetch_balance(platform: &str, api_key: &str, proxy: &str) -> Result<Balances, String> {
    let url = match platform {
        "stepfun_token_cn" => CHINA,
        "stepfun_token_global" => GLOBAL,
        other => return Err(format!("StepFun has no endpoint for {other}")),
    };

    let client = http_client(Duration::from_secs(10), proxy)?;
    let response = client
        .get(url)
        .header("Accept", "application/json")
        .bearer_auth(api_key.trim())
        .send()
        .map_err(|error| format!("StepFun request failed: {error}"))?;

    if response.status() == StatusCode::UNAUTHORIZED {
        return Err("Invalid API key (401 Unauthorized)".to_owned());
    }

    let account: StepFunAccount = response
        .error_for_status()
        .map_err(|error| error.to_string())?
        .json()
        .map_err(|error| format!("StepFun JSON parse failed: {error}"))?;

    let mut balances = Balances::new();
    balances.insert(
        catalog::default_currency(platform).to_owned(),
        Balance {
            total_balance: account.balance,
            granted_balance: account.total_voucher_balance,
            topped_up_balance: account.total_cash_balance,
        },
    );
    Ok(balances)
}

#[derive(Debug, Deserialize)]
struct StepFunAccount {
    #[serde(default, deserialize_with = "deserialize_number")]
    balance: f64,
    #[serde(default, deserialize_with = "deserialize_number")]
    total_cash_balance: f64,
    #[serde(default, deserialize_with = "deserialize_number")]
    total_voucher_balance: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_the_account_fields() {
        let payload = r#"{
            "type": "prepaid",
            "balance": 12.75,
            "total_cash_balance": 10.0,
            "total_voucher_balance": 2.75
        }"#;
        let account: StepFunAccount = serde_json::from_str(payload).unwrap();
        assert_eq!(account.balance, 12.75);
        assert_eq!(account.total_cash_balance, 10.0);
        assert_eq!(account.total_voucher_balance, 2.75);
    }

    #[test]
    fn defaults_missing_amounts_to_zero() {
        let account: StepFunAccount = serde_json::from_str("{}").unwrap();
        assert_eq!(account.balance, 0.0);
        assert_eq!(account.total_cash_balance, 0.0);
    }

    #[test]
    fn unknown_platforms_are_rejected() {
        assert!(fetch_balance("nonsense", "key", "").is_err());
    }
}
