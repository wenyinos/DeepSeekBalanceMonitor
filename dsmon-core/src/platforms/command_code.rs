//! Command Code usage endpoint.

use std::time::Duration;

use chrono::Local;

use super::{epoch_to_reset_seconds, http_client, sanitize_message, urlencode};
use crate::model::{
    CommandCodeApiResponse, CommandCodeApiWhoami, CommandCodeApiWindow, PackageQuota, QuotaWindow,
};

const API_BASE: &str = "https://api.commandcode.ai/";

/// Reads the five-hour and weekly windows plus the derived monthly view.
pub fn fetch_quota(api_key: &str, http_proxy: &str) -> Result<PackageQuota, String> {
    let client = http_client(Duration::from_secs(10), http_proxy)?;
    let organization_id = fetch_org_id(&client, api_key)?;
    let org_query = organization_id
        .map(|id| format!("?orgId={}", urlencode(&id)))
        .unwrap_or_default();

    let response = client
        .get(format!("{API_BASE}alpha/billing/credits{org_query}"))
        .header("Accept", "application/json")
        .header("Authorization", format!("Bearer {}", api_key.trim()))
        .send()
        .map_err(|error| format!("Command Code request failed: {error}"))?;

    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().unwrap_or_default();
        return Err(format!(
            "Command Code API error {status}: {}",
            sanitize_message(&text)
        ));
    }

    let payload: CommandCodeApiResponse = response
        .error_for_status()
        .map_err(|error| error.to_string())?
        .json()
        .map_err(|error| format!("Command Code JSON parse failed: {error}"))?;

    let now = Local::now().timestamp();
    let monthly_cap = payload
        .window_limits
        .five_hour
        .as_ref()
        .zip(payload.window_limits.weekly.as_ref())
        .and_then(|(five_hour, weekly)| monthly_cap(five_hour.cap.max(0.0), weekly.cap.max(0.0)));
    let monthly = monthly_window(monthly_cap, payload.credits.monthly_credits);

    let mut quota = PackageQuota::new();
    for (name, window) in [
        ("5h", payload.window_limits.five_hour),
        ("weekly", payload.window_limits.weekly),
    ] {
        if let Some(window) = window {
            quota.insert(name.to_owned(), window_to_window(window, now));
        }
    }
    if let Some(monthly) = monthly {
        quota.insert("monthly".to_owned(), monthly);
    }

    if quota.is_empty() {
        return Err("Command Code API returned no usage windows.".to_owned());
    }
    Ok(quota)
}

/// The credits endpoint wants an organization id; the account may not have one.
fn fetch_org_id(
    client: &reqwest::blocking::Client,
    api_key: &str,
) -> Result<Option<String>, String> {
    let response = client
        .get(format!("{API_BASE}alpha/whoami"))
        .header("Accept", "application/json")
        .header("Authorization", format!("Bearer {}", api_key.trim()))
        .send()
        .map_err(|error| format!("Command Code request failed: {error}"))?;

    if !response.status().is_success() {
        return Ok(None);
    }

    let payload: CommandCodeApiWhoami = response
        .error_for_status()
        .map_err(|error| error.to_string())?
        .json()
        .map_err(|error| format!("Command Code JSON parse failed: {error}"))?;
    Ok(payload.org.and_then(|org| org.id))
}

fn window_to_window(window: CommandCodeApiWindow, now: i64) -> QuotaWindow {
    QuotaWindow::from_pool(
        window.used,
        window.cap,
        epoch_to_reset_seconds(window.reset_at, now),
    )
}

/// Plan credit pools keyed by the plan's rolling window caps, per
/// <https://commandcode.ai/docs/resources/usage-limits>.
/// Every plan has a unique (5h, weekly) pair.
fn monthly_cap(five_hour_cap: f64, weekly_cap: f64) -> Option<f64> {
    match (five_hour_cap.round() as i64, weekly_cap.round() as i64) {
        (3, 6) => Some(10.0),     // Go
        (14, 35) => Some(70.0),   // GOAT
        (16, 40) => Some(80.0),   // Pro
        (45, 90) => Some(150.0),  // Max 10x
        (90, 180) => Some(300.0), // Max 20x
        (12, 24) => Some(40.0),   // Team Pro
        // Plans without a rolling window (top-up only accounts) get no estimate.
        _ => None,
    }
}

/// Derives the monthly window from the plan pool and the remaining credits.
fn monthly_window(monthly_cap: Option<f64>, monthly_credits: Option<f64>) -> Option<QuotaWindow> {
    monthly_cap
        .zip(monthly_credits)
        .map(|(cap, remaining)| QuotaWindow::from_pool(cap - remaining, cap, 0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_plans_to_monthly_pools() {
        assert_eq!(monthly_cap(3.0, 6.0), Some(10.0));
        assert_eq!(monthly_cap(90.0, 180.0), Some(300.0));
        assert_eq!(monthly_cap(1.0, 2.0), None, "unknown plans get no estimate");
    }

    #[test]
    fn derives_the_monthly_window_from_remaining_credits() {
        let window = monthly_window(Some(80.0), Some(30.0)).expect("window is derived");
        assert_eq!(window.cap, Some(80.0));
        assert_eq!(window.used, Some(50.0));
        assert_eq!(window.reset_in_sec, 0);

        assert!(monthly_window(Some(80.0), None).is_none());
        assert!(monthly_window(None, Some(30.0)).is_none());
    }

    #[test]
    fn normalises_millisecond_and_second_epochs() {
        assert_eq!(
            epoch_to_reset_seconds(Some(1_767_225_600.0), 1_767_225_600),
            0
        );
        assert_eq!(
            epoch_to_reset_seconds(Some(1_767_225_600_000.0), 1_767_225_600),
            0
        );
        assert_eq!(
            epoch_to_reset_seconds(Some(1_767_225_600.0 + 3600.0), 1_767_225_600),
            3600
        );
        assert_eq!(epoch_to_reset_seconds(None, 0), 0);
    }

    #[test]
    fn clamps_negative_windows() {
        let window = window_to_window(
            CommandCodeApiWindow {
                used: -5.0,
                cap: -1.0,
                reset_at: None,
            },
            0,
        );
        assert_eq!(window.used, Some(0.0));
        assert_eq!(window.cap, Some(0.0));
        assert_eq!(window.usage_percent, 0.0);
    }

    /// The monthly view is money spent out of a pool, so it records those
    /// rather than a share of a hundred.
    #[test]
    fn the_monthly_pool_is_recorded_in_money() {
        let window = monthly_window(Some(80.0), Some(30.0)).expect("window is derived");
        assert_eq!(window.as_recorded_usage(), (50.0, 80.0));
        assert_eq!(window.usage_percent, 62.5);

        let share = QuotaWindow::from_percent(25.0, 0);
        assert_eq!(share.as_recorded_usage(), (25.0, 100.0));
    }
}
