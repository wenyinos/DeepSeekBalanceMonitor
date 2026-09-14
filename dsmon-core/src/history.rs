//! History aggregation: summaries, CSV export and the busy-hour rate estimate.

use std::collections::BTreeMap;

use chrono::{Datelike, Duration as ChronoDuration, Local, NaiveDate, NaiveDateTime};

use crate::model::{ConsumptionRate, HistoryRecord, HistorySummary, SubscriptionPoint};
use crate::storage;
use crate::time;

/// Groups records by currency and computes the per-currency aggregate.
pub fn summarize_history(records: &[HistoryRecord]) -> Vec<HistorySummary> {
    let mut grouped: BTreeMap<String, Vec<&HistoryRecord>> = BTreeMap::new();
    for record in records {
        grouped
            .entry(record.currency.clone())
            .or_default()
            .push(record);
    }

    grouped
        .into_iter()
        .filter_map(|(currency, items)| {
            let first = items.first()?;
            let latest = items.last()?;
            let min_total = items
                .iter()
                .map(|record| record.total)
                .fold(f64::INFINITY, f64::min);
            let max_total = items
                .iter()
                .map(|record| record.total)
                .fold(f64::NEG_INFINITY, f64::max);
            let avg_total =
                items.iter().map(|record| record.total).sum::<f64>() / items.len() as f64;

            Some(HistorySummary {
                currency,
                records: items.len(),
                first_time: first.timestamp.clone(),
                last_time: latest.timestamp.clone(),
                latest_total: latest.total,
                latest_topped: latest.topped,
                latest_granted: latest.granted,
                min_total,
                max_total,
                avg_total,
                change_total: latest.total - first.total,
            })
        })
        .collect()
}

/// Busy-hour rate over the last `hours`, for the most recently seen currency.
pub fn consumption_rate(
    hours: i64,
    interval_minutes: u64,
) -> Result<Option<ConsumptionRate>, String> {
    let conn = storage::open_db()?;
    let currency = match conn.query_row(
        "SELECT currency FROM balance_history
         GROUP BY currency
         ORDER BY MAX(timestamp) DESC, MAX(total) DESC
         LIMIT 1",
        [],
        |row| row.get::<_, String>(0),
    ) {
        Ok(value) => value,
        Err(rusqlite::Error::QueryReturnedNoRows) => return Ok(None),
        Err(error) => return Err(error.to_string()),
    };

    let cutoff = time::format_local(Local::now() - ChronoDuration::hours(hours.max(1)));
    let mut stmt = conn
        .prepare(
            "SELECT timestamp, currency, total, topped, granted, service_status
             FROM balance_history
             WHERE timestamp >= ?1 AND currency = ?2
             ORDER BY timestamp ASC",
        )
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map(rusqlite::params![cutoff, currency], |row| {
            Ok(HistoryRecord {
                timestamp: row.get(0)?,
                currency: row.get(1)?,
                total: row.get(2)?,
                topped: row.get(3)?,
                granted: row.get(4)?,
                service_status: row.get(5)?,
            })
        })
        .map_err(|error| error.to_string())?;

    let mut records = Vec::new();
    for row in rows {
        records.push(row.map_err(|error| error.to_string())?);
    }
    consumption_rate_from_records(&records, interval_minutes)
}

/// Seven days of data, widening to the retention window when that is empty.
pub fn consumption_rate_with_fallback(
    retention_days: u64,
    interval_minutes: u64,
) -> Result<Option<ConsumptionRate>, String> {
    if let Some(rate) = consumption_rate(7 * 24, interval_minutes)? {
        return Ok(Some(rate));
    }

    let fallback_hours = retention_days
        .max(1)
        .saturating_mul(24)
        .min(i64::MAX as u64) as i64;
    if fallback_hours <= 7 * 24 {
        return Ok(None);
    }
    consumption_rate(fallback_hours, interval_minutes)
}

/// Estimates the burn rate from `topped_up` readings.
///
/// Three rules keep idle periods out of the average:
/// 1. a top-up starts a new interval;
/// 2. a gap longer than the busy threshold slices the interval;
/// 3. a flat run longer than the threshold is discarded.
pub fn consumption_rate_from_records(
    records: &[HistoryRecord],
    interval_minutes: u64,
) -> Result<Option<ConsumptionRate>, String> {
    if records.len() < 2 {
        return Ok(None);
    }

    let currency = records[0].currency.clone();
    let parsed: Vec<(NaiveDateTime, f64)> = records
        .iter()
        .map(|record| {
            let timestamp = NaiveDateTime::parse_from_str(&record.timestamp, "%Y-%m-%d %H:%M:%S")
                .map_err(|error| error.to_string())?;
            Ok((timestamp, record.topped))
        })
        .collect::<Result<Vec<_>, String>>()?;

    // A reading gap above this counts as idle time.
    let threshold_minutes = 30i64.max(2 * interval_minutes as i64);
    let threshold_seconds = (threshold_minutes * 60) as f64;

    let mut intervals: Vec<(f64, NaiveDateTime, f64, NaiveDateTime)> = Vec::new();
    let mut segment_start_value = parsed[0].1;
    let mut segment_start_time = parsed[0].0;
    let mut previous_value = segment_start_value;
    let mut previous_time = segment_start_time;
    let mut equal_run_start: Option<usize> = None;

    for index in 1..parsed.len() {
        let (current_time, current_value) = parsed[index];
        let gap_seconds = (current_time - previous_time).num_seconds() as f64;

        if current_value > previous_value {
            // Rule 1: a top-up closes the interval and starts a new one.
            if let Some(run_start) = equal_run_start {
                let run_seconds = (previous_time - parsed[run_start].0).num_seconds() as f64;
                if run_seconds > threshold_seconds {
                    if parsed[run_start].0 > segment_start_time {
                        intervals.push((
                            segment_start_value,
                            segment_start_time,
                            parsed[run_start].1,
                            parsed[run_start].0,
                        ));
                    }
                    segment_start_value = current_value;
                    segment_start_time = current_time;
                    previous_value = current_value;
                    previous_time = current_time;
                    equal_run_start = None;
                    continue;
                }
                equal_run_start = None;
            }

            if previous_time > segment_start_time {
                intervals.push((
                    segment_start_value,
                    segment_start_time,
                    previous_value,
                    previous_time,
                ));
            }
            segment_start_value = current_value;
            segment_start_time = current_time;
        } else if current_value < previous_value {
            if gap_seconds > threshold_seconds {
                // Rule 2: long idle gap, slice the interval. A pending equal run
                // is resolved first so flat periods are not folded in.
                if let Some(run_start) = equal_run_start {
                    let run_seconds = (previous_time - parsed[run_start].0).num_seconds() as f64;
                    if run_seconds > threshold_seconds {
                        if parsed[run_start].0 > segment_start_time {
                            intervals.push((
                                segment_start_value,
                                segment_start_time,
                                parsed[run_start].1,
                                parsed[run_start].0,
                            ));
                        }
                        segment_start_value = previous_value;
                        segment_start_time = previous_time;
                    }
                    equal_run_start = None;
                }
                if previous_time > segment_start_time {
                    intervals.push((
                        segment_start_value,
                        segment_start_time,
                        previous_value,
                        previous_time,
                    ));
                }
                segment_start_value = current_value;
                segment_start_time = current_time;
            } else if let Some(run_start) = equal_run_start {
                let run_seconds = (previous_time - parsed[run_start].0).num_seconds() as f64;
                if run_seconds > threshold_seconds {
                    // Rule 3: a long flat run is dropped.
                    if parsed[run_start].0 > segment_start_time {
                        intervals.push((
                            segment_start_value,
                            segment_start_time,
                            parsed[run_start].1,
                            parsed[run_start].0,
                        ));
                    }
                    segment_start_value = current_value;
                    segment_start_time = current_time;
                    previous_value = current_value;
                    previous_time = current_time;
                    equal_run_start = None;
                    continue;
                }
                equal_run_start = None;
            }
        } else if equal_run_start.is_none() {
            // Rule 3: track how long the value has been flat.
            equal_run_start = Some(index - 1);
        }

        previous_value = current_value;
        previous_time = current_time;
    }

    // A trailing flat run never got resolved because the value stopped moving.
    if let Some(run_start) = equal_run_start {
        let last = parsed.last().expect("records is not empty");
        let run_seconds = (last.0 - parsed[run_start].0).num_seconds() as f64;
        if run_seconds > threshold_seconds {
            if parsed[run_start].0 > segment_start_time {
                intervals.push((
                    segment_start_value,
                    segment_start_time,
                    parsed[run_start].1,
                    parsed[run_start].0,
                ));
            }
            segment_start_time = last.0;
        }
    }

    if parsed.last().expect("records is not empty").0 > segment_start_time {
        intervals.push((
            segment_start_value,
            segment_start_time,
            previous_value,
            previous_time,
        ));
    }

    // Weighted average of the per-interval hourly rates.
    let mut total_weight = 0.0;
    let mut weighted_sum = 0.0;
    for (start_value, start_time, end_value, end_time) in intervals {
        if end_value >= start_value {
            continue;
        }
        let delta_hours = (end_time - start_time).num_seconds() as f64 / 3600.0;
        if delta_hours < 0.01 {
            continue;
        }
        let hourly_rate = (start_value - end_value) / delta_hours;
        weighted_sum += hourly_rate * delta_hours;
        total_weight += delta_hours;
    }

    if total_weight == 0.0 {
        return Ok(None);
    }
    let average_hourly = weighted_sum / total_weight;
    if average_hourly <= 0.0 {
        return Ok(None);
    }

    let remaining = parsed.last().expect("records is not empty").1;
    Ok(Some(ConsumptionRate {
        hourly_rate: average_hourly,
        busy_hours_left: remaining / average_hourly,
        currency,
    }))
}

/// One day's consumption, derived from consecutive readings.
#[derive(Debug, Clone, PartialEq)]
pub struct DailyUsage {
    /// `YYYY-MM-DD`.
    pub date: String,
    pub used: f64,
}

/// Collapses subscription readings into per-day consumption.
///
/// The last reading of each day stands for that day, and the difference to the
/// previous day is what that day burned. A drop means the allowance was renewed,
/// so the day is skipped rather than recorded as negative usage.
pub fn daily_usage(points: &[SubscriptionPoint]) -> Vec<DailyUsage> {
    let mut last_reading: BTreeMap<String, f64> = BTreeMap::new();
    for point in points {
        // Timestamps are `YYYY-MM-DD HH:MM:SS`; the date is the first ten bytes.
        if let Some(date) = point.timestamp.get(..10) {
            last_reading.insert(date.to_owned(), point.used);
        }
    }

    let mut usage = Vec::new();
    let mut previous: Option<f64> = None;
    for (date, used) in last_reading {
        if let Some(previous) = previous {
            let delta = used - previous;
            if delta >= 0.0 {
                usage.push(DailyUsage { date, used: delta });
            }
        }
        previous = Some(used);
    }
    usage
}

/// Start of the billing cycle containing `today`.
///
/// Billing days are capped at 28 so the date exists in every month; when this
/// month's date has not arrived yet, the cycle began last month.
pub fn cycle_start(today: NaiveDate, billing_day: u8) -> NaiveDate {
    let day = u32::from(billing_day.clamp(1, 28));
    match NaiveDate::from_ymd_opt(today.year(), today.month(), day) {
        Some(date) if date <= today => date,
        _ => {
            let (year, month) = if today.month() == 1 {
                (today.year() - 1, 12)
            } else {
                (today.year(), today.month() - 1)
            };
            NaiveDate::from_ymd_opt(year, month, day).unwrap_or(today)
        }
    }
}

/// Renders records as CSV, matching the column order the old CLI exported.
pub fn history_csv(records: &[HistoryRecord]) -> String {
    let mut lines = vec!["timestamp,currency,total,topped,granted,service_status".to_owned()];
    for record in records {
        lines.push(format!(
            "{},{},{},{},{},{}",
            csv_escape(&record.timestamp),
            csv_escape(&record.currency),
            format_amount(record.total),
            format_amount(record.topped),
            format_amount(record.granted),
            csv_escape(&record.service_status)
        ));
    }
    lines.join("\n") + "\n"
}

fn csv_escape(value: &str) -> String {
    if value.contains([',', '"', '\n']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_owned()
    }
}

/// Two decimals, the format the exports and the interface both use.
pub fn format_amount(value: f64) -> String {
    format!("{value:.2}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(timestamp: &str, topped: f64) -> HistoryRecord {
        HistoryRecord {
            timestamp: timestamp.to_owned(),
            currency: "CNY".to_owned(),
            total: topped,
            topped,
            granted: 0.0,
            service_status: "ok".to_owned(),
        }
    }

    #[test]
    fn summarises_each_currency() {
        let records = vec![
            record("2026-01-01 10:00:00", 100.0),
            record("2026-01-01 11:00:00", 90.0),
        ];
        let summaries = summarize_history(&records);
        assert_eq!(summaries.len(), 1);
        let summary = &summaries[0];
        assert_eq!(summary.currency, "CNY");
        assert_eq!(summary.records, 2);
        assert_eq!(summary.min_total, 90.0);
        assert_eq!(summary.max_total, 100.0);
        assert_eq!(summary.change_total, -10.0);
        assert_eq!(summary.avg_total, 95.0);
    }

    #[test]
    fn needs_two_records_for_a_rate() {
        assert!(consumption_rate_from_records(&[], 10).unwrap().is_none());
        assert!(
            consumption_rate_from_records(&[record("2026-01-01 10:00:00", 10.0)], 10)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn estimates_a_rate_from_steady_consumption() {
        // 10 units burned every 10 minutes over one hour: 60 units/hour.
        let records: Vec<HistoryRecord> = (0..7)
            .map(|step| {
                let minutes = step * 10;
                let timestamp =
                    format!("2026-01-01 {:02}:{:02}:00", 10 + minutes / 60, minutes % 60);
                record(&timestamp, 100.0 - step as f64 * 10.0)
            })
            .collect();

        let rate = consumption_rate_from_records(&records, 10)
            .unwrap()
            .expect("a rate is produced");
        assert_eq!(rate.currency, "CNY");
        assert!(
            (rate.hourly_rate - 60.0).abs() < 1.0,
            "expected about 60/hour, got {}",
            rate.hourly_rate
        );
        assert!(rate.busy_hours_left > 0.0);
    }

    #[test]
    fn a_top_up_starts_a_fresh_interval() {
        // Balance rises mid-series, so the drop after it is measured on its own.
        let records = vec![
            record("2026-01-01 10:00:00", 100.0),
            record("2026-01-01 10:10:00", 90.0),
            record("2026-01-01 10:20:00", 200.0),
            record("2026-01-01 10:30:00", 180.0),
        ];
        let rate = consumption_rate_from_records(&records, 10)
            .unwrap()
            .expect("a rate is produced");
        assert!(rate.hourly_rate > 0.0);
    }

    fn point(timestamp: &str, used: f64) -> SubscriptionPoint {
        SubscriptionPoint {
            timestamp: timestamp.to_owned(),
            used,
            cap: 100.0,
        }
    }

    #[test]
    fn computes_per_day_usage() {
        let points = vec![
            point("2026-01-01 10:00:00", 10.0),
            point("2026-01-01 22:00:00", 12.0),
            point("2026-01-02 09:00:00", 15.0),
            point("2026-01-03 09:00:00", 20.0),
        ];
        let usage = daily_usage(&points);
        // The first day has nothing to compare against; the rest count their
        // increase over the previous day's last reading.
        assert_eq!(usage.len(), 2);
        assert_eq!(usage[0].date, "2026-01-02");
        assert_eq!(usage[0].used, 3.0);
        assert_eq!(usage[1].date, "2026-01-03");
        assert_eq!(usage[1].used, 5.0);
    }

    #[test]
    fn a_renewal_is_not_negative_usage() {
        let points = vec![
            point("2026-01-01 09:00:00", 90.0),
            point("2026-01-02 09:00:00", 2.0),
            point("2026-01-03 09:00:00", 6.0),
        ];
        let usage = daily_usage(&points);
        assert_eq!(usage.len(), 1, "the reset day is skipped");
        assert_eq!(usage[0].date, "2026-01-03");
        assert_eq!(usage[0].used, 4.0);
    }

    #[test]
    fn finds_the_cycle_start() {
        let day = |y, m, d| NaiveDate::from_ymd_opt(y, m, d).unwrap();
        // Billing day 1: the cycle starts this month.
        assert_eq!(cycle_start(day(2026, 3, 15), 1), day(2026, 3, 1));
        // Billing day 20: not reached yet, so the cycle began last month.
        assert_eq!(cycle_start(day(2026, 3, 15), 20), day(2026, 2, 20));
        assert_eq!(cycle_start(day(2026, 3, 25), 20), day(2026, 3, 20));
        // January wraps back into the previous year.
        assert_eq!(cycle_start(day(2026, 1, 5), 20), day(2025, 12, 20));
        // Out-of-range days are clamped rather than rejected; the 28th of March
        // has not arrived on the 15th, so the cycle began back in February.
        assert_eq!(cycle_start(day(2026, 3, 15), 31), day(2026, 2, 28));
    }

    #[test]
    fn csv_carries_a_header_and_escapes_values() {
        let mut item = record("2026-01-01 10:00:00", 12.5);
        item.currency = "C,N".to_owned();
        let csv = history_csv(&[item]);
        let mut lines = csv.lines();
        assert_eq!(
            lines.next().unwrap(),
            "timestamp,currency,total,topped,granted,service_status"
        );
        assert!(lines.next().unwrap().contains("\"C,N\""));
        assert!(csv.ends_with('\n'));
    }
}
