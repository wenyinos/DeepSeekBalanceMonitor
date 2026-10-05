//! DeepSeek balance endpoint.

use std::time::Duration;

use reqwest::StatusCode;

use super::http_client;
use crate::model::{ApiBalanceInfo, ApiResponse, Balance, Balances};

const BALANCE_URL: &str = "https://api.deepseek.com/user/balance";

/// Reads every currency balance the account reports.
pub fn fetch_balance(api_key: &str, http_proxy: &str) -> Result<Balances, String> {
    let client = http_client(Duration::from_secs(15), http_proxy)?;
    let response = client
        .get(BALANCE_URL)
        .header("Accept", "application/json")
        .bearer_auth(clean_api_key(api_key))
        .send()
        .map_err(|error| error.to_string())?;

    if response.status() == StatusCode::UNAUTHORIZED {
        return Err("Invalid API key (401 Unauthorized)".to_owned());
    }

    let payload: ApiResponse = response
        .error_for_status()
        .map_err(|error| error.to_string())?
        .json()
        .map_err(|error| error.to_string())?;

    if payload.balance_infos.is_empty() {
        return Err("No balance information in response".to_owned());
    }

    let mut balances = Balances::new();
    for item in payload.balance_infos {
        balances.insert(item.currency.clone(), reported_balance(&item));
    }
    Ok(balances)
}

/// Keys pasted from a terminal occasionally carry stray non-ASCII bytes, which
/// are illegal in an HTTP header. Dropping them keeps the request valid.
fn clean_api_key(api_key: &str) -> String {
    api_key.trim().chars().filter(char::is_ascii).collect()
}

/// The API reports amounts as strings; anything unparsable counts as zero.
fn parse_amount(value: &str) -> f64 {
    value.trim().parse::<f64>().unwrap_or(0.0)
}

/// One API entry in the app's balance model.
///
/// A negative bucket is NOT usable balance: clamp each bucket at 0 and derive
/// the total from the clamped buckets (`topped -0.10 + granted 6.00` => 6.00,
/// not the raw sum 5.90). The earlier build settled on the same reading.
fn reported_balance(item: &ApiBalanceInfo) -> Balance {
    let granted = parse_amount(&item.granted_balance).max(0.0);
    let topped = parse_amount(&item.topped_up_balance).max(0.0);
    Balance {
        total_balance: topped + granted,
        granted_balance: granted,
        topped_up_balance: topped,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn amounts_parse_from_strings() {
        assert_eq!(parse_amount("12.34"), 12.34);
        assert_eq!(parse_amount(" 0 "), 0.0);
        assert_eq!(parse_amount("n/a"), 0.0);
    }

    #[test]
    fn keys_are_stripped_of_non_ascii() {
        assert_eq!(clean_api_key("  sk-abc  "), "sk-abc");
        assert_eq!(clean_api_key("sk-ab\u{4e2d}c"), "sk-abc");
    }

    /// The reported case: topped -0.10 with granted 6.00 is 6.00 usable, not
    /// the raw sum 5.90.
    #[test]
    fn negative_buckets_are_clamped_and_the_total_rebuilt() {
        let item = ApiBalanceInfo {
            currency: "CNY".to_owned(),
            total_balance: "5.90".to_owned(),
            granted_balance: "6.00".to_owned(),
            topped_up_balance: "-0.10".to_owned(),
        };

        let balance = reported_balance(&item);
        assert_eq!(balance.total_balance, 6.0);
        assert_eq!(balance.topped_up_balance, 0.0);
        assert_eq!(balance.granted_balance, 6.0);
    }
}
