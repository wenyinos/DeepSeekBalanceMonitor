//! DeepSeek balance endpoint.

use std::time::Duration;

use reqwest::StatusCode;

use super::http_client;
use crate::model::{ApiResponse, Balance, Balances};

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
        balances.insert(
            item.currency,
            Balance {
                total_balance: parse_amount(&item.total_balance),
                granted_balance: parse_amount(&item.granted_balance),
                topped_up_balance: parse_amount(&item.topped_up_balance),
            },
        );
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
}
