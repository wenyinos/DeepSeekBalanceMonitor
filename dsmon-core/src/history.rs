//! History aggregation: summaries, CSV export and the busy-hour rate estimate.

use std::collections::BTreeMap;

use chrono::{Datelike, Duration as ChronoDuration, Local, NaiveDate, NaiveDateTime};

use crate::model::{ConsumptionRate, HistoryRecord, HistorySummary, SubscriptionPoint, WindowRate};
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
    platform: &str,
    hours: i64,
    interval_minutes: u64,
) -> Result<Option<ConsumptionRate>, String> {
    let conn = storage::open_db()?;
    let currency = match conn.query_row(
        "SELECT currency FROM balance_history
         WHERE platform = ?1
         GROUP BY currency
         ORDER BY MAX(timestamp) DESC, MAX(total) DESC
         LIMIT 1",
        rusqlite::params![platform],
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
             WHERE platform = ?1 AND timestamp >= ?2 AND currency = ?3
             ORDER BY timestamp ASC",
        )
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map(rusqlite::params![platform, cutoff, currency], |row| {
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
    platform: &str,
    retention_days: u64,
    interval_minutes: u64,
) -> Result<Option<ConsumptionRate>, String> {
    if let Some(rate) = consumption_rate(platform, 7 * 24, interval_minutes)? {
        return Ok(Some(rate));
    }

    let fallback_hours = retention_days
        .max(1)
        .saturating_mul(24)
        .min(i64::MAX as u64) as i64;
    if fallback_hours <= 7 * 24 {
        return Ok(None);
    }
    consumption_rate(platform, fallback_hours, interval_minutes)
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

/// One stretch of readings that shared a service status.
#[derive(Debug, Clone, PartialEq)]
pub struct StatusSpan {
    /// The status the readings reported, in the interface's vocabulary.
    pub status: String,
    /// How many readings the stretch covers.
    pub readings: usize,
}

/// The service status over a run of readings, as contiguous stretches.
///
/// The readings are equally spaced by the poll interval, so a stretch's share
/// of the readings is its share of the window — which is what a band drawn from
/// these gives, and why the stretches are not measured in time.
pub fn service_status_spans(records: &[HistoryRecord]) -> Vec<StatusSpan> {
    let mut spans: Vec<StatusSpan> = Vec::new();
    for record in records {
        match spans.last_mut() {
            Some(span) if span.status == record.service_status => span.readings += 1,
            _ => spans.push(StatusSpan {
                status: record.service_status.clone(),
                readings: 1,
            }),
        }
    }
    spans
}

/// The share of the readable readings that were healthy, 0-100.
///
/// Readings that could not be read at all are left out of the figure: they say
/// nothing about the vendor's service, and counting them as downtime would
/// blame the vendor for this program's own trouble. `None` when every reading
/// in the window was unreadable.
pub fn availability_percent(spans: &[StatusSpan]) -> Option<f64> {
    let healthy: usize = spans
        .iter()
        .filter(|span| span.status == "none")
        .map(|span| span.readings)
        .sum();
    let readable: usize = spans
        .iter()
        .filter(|span| span.status != "unknown")
        .map(|span| span.readings)
        .sum();

    (readable > 0).then(|| healthy as f64 / readable as f64 * 100.0)
}

/// The pace of one quota window, from the readings logged inside its cycle.
///
/// The cycle begins at the last drop in the readings — a window's usage falls
/// back when it resets — or at the earliest reading there is, which is all a
/// plan first seen mid-cycle has to offer. The average runs from there to the
/// latest reading, so idle time counts towards it: the clock runs whether the
/// quota is used or not, and the pace is what has to be weighed against it.
pub fn window_rate(points: &[SubscriptionPoint], reset_in_sec: i64) -> Option<WindowRate> {
    let latest = points.last()?;
    let start = points
        .windows(2)
        .rposition(|pair| pair[1].percent() < pair[0].percent())
        .map_or(0, |index| index + 1);
    let first = points.get(start)?;
    if first.timestamp == latest.timestamp {
        return None;
    }

    let hours = (parse_timestamp(&latest.timestamp)? - parse_timestamp(&first.timestamp)?)
        .num_seconds() as f64
        / 3600.0;
    if hours <= 0.0 {
        return None;
    }

    let pace = (latest.percent() - first.percent()) / hours;
    if pace <= 0.0 {
        return None;
    }

    let left = (100.0 - latest.percent()).max(0.0);
    Some(WindowRate {
        percent_per_hour: pace,
        hours_left: Some(left / pace),
        reset_in_sec,
    })
}

/// The store's own timestamp format, which orders as it reads.
fn parse_timestamp(text: &str) -> Option<NaiveDateTime> {
    NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M:%S").ok()
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
/// When this month's billing date has not arrived yet, the cycle began last
/// month.
/// A quota percentage as the interface writes it.
///
/// Most windows arrive as whole percents and stay whole; OpenCode Go's coarse
/// ones are refined into decimals from the money that was spent
/// ([`refined_percent`]), and printing those as whole numbers would throw away
/// exactly what the refinement is for.
pub fn format_percent(value: f64) -> String {
    if (value - value.round()).abs() < 0.005 {
        format!("{value:.0}%")
    } else {
        format!("{value:.2}%")
    }
}

/// Where between two whole percents a coarse window really is.
///
/// The endpoint reports the weekly and monthly windows as whole percents, one
/// step at a time, and the five-hour window as money. A pool is a known amount
/// ($12 per five hours, $30 a week, $60 a month), so one percent of it is worth
/// a fixed sum, and the money spent since the whole percent last moved says how
/// far into the current step the truth has travelled — which is what the
/// previous build showed, and what this one left out.
///
/// Strictly causal: only money actually recorded moves the figure, and only
/// forward (a refund does not unspend it). A stretch with nothing recorded
/// keeps the coarse value, however much time passes, and a step is never
/// refined past its own width — when the spending fills it, the next poll's
/// whole percent says so.
pub fn refined_percent(
    coarse_points: &[SubscriptionPoint],
    spent_points: &[SubscriptionPoint],
    pool: f64,
) -> Option<f64> {
    let current = coarse_points.last()?.used;
    if pool <= 0.0 {
        return Some(current);
    }

    // Where the current whole percent began: the first reading that already
    // showed it, after one that showed something else.
    let step_start = coarse_points
        .windows(2)
        .rev()
        .find(|pair| (pair[0].used - current).abs() > 0.000_001)
        .map(|pair| pair[1].timestamp.as_str())
        .unwrap_or(coarse_points[0].timestamp.as_str());

    // The money spent since then, added up interval by interval. Timestamps
    // are the stored `YYYY-MM-DD HH:MM:SS`, so the comparison is a string one
    // on purpose: in that format it orders the same as time does.
    let spent: f64 = spent_points
        .iter()
        .filter(|point| point.timestamp.as_str() >= step_start)
        .fold((None::<f64>, 0.0), |(previous, total), point| {
            let step = previous.map_or(0.0, |value| (point.used - value).max(0.0));
            (Some(point.used), total + step)
        })
        .1;

    let per_percent = pool / 100.0;
    Some(current + (spent / per_percent).clamp(0.0, 1.0))
}

pub fn cycle_start(today: NaiveDate, billing_day: u8) -> NaiveDate {
    let day = u32::from(billing_day.clamp(1, crate::config::MAX_BILLING_DAY));
    match date_in_month(today.year(), today.month(), day) {
        Some(date) if date <= today => date,
        _ => {
            let (year, month) = if today.month() == 1 {
                (today.year() - 1, 12)
            } else {
                (today.year(), today.month() - 1)
            };
            date_in_month(year, month, day).unwrap_or(today)
        }
    }
}

/// The given day of a month, or that month's last day when it is shorter.
///
/// A billing day of 31 lands on the 30th of April and the 28th of February,
/// rather than being rejected.
fn date_in_month(year: i32, month: u32, day: u32) -> Option<NaiveDate> {
    let next_month = if month == 12 {
        NaiveDate::from_ymd_opt(year + 1, 1, 1)?
    } else {
        NaiveDate::from_ymd_opt(year, month + 1, 1)?
    };
    let last_day = next_month.pred_opt()?.day();
    NaiveDate::from_ymd_opt(year, month, day.min(last_day))
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
    fn reads_the_pace_of_the_current_cycle() {
        // Ten points of usage in twenty-four hours, with half the window left.
        let points = vec![
            point("2026-01-01 00:00:00", 20.0),
            point("2026-01-01 12:00:00", 25.0),
            point("2026-01-02 00:00:00", 30.0),
        ];
        let rate = window_rate(&points, 3600).expect("a pace");
        assert!((rate.percent_per_hour - 10.0 / 24.0).abs() < 0.0001);
        assert!((rate.percent_per_day() - 10.0).abs() < 0.0001);
        assert!((rate.hours_left.expect("hours left") - 168.0).abs() < 0.0001);
        // Seventy points left at ten a day is a week of use, so an hour of
        // window is not the clock that ends it.
        assert!(!rate.runs_out_first());

        // A month of window is: a week of use runs out long before it does.
        let monthly = window_rate(&points, 30 * 24 * 3600).expect("a pace");
        assert!(monthly.runs_out_first());
    }

    #[test]
    fn a_reset_starts_the_cycle_over() {
        // The first two readings belong to a cycle that has ended; only what
        // followed the drop describes the pace now.
        let points = vec![
            point("2026-01-01 00:00:00", 80.0),
            point("2026-01-01 12:00:00", 95.0),
            point("2026-01-02 00:00:00", 2.0),
            point("2026-01-02 06:00:00", 8.0),
        ];
        let rate = window_rate(&points, 0).expect("a pace");
        assert!(
            (rate.percent_per_hour - 1.0).abs() < 0.0001,
            "6 points in 6 hours"
        );
        assert_eq!(rate.reset_in_sec, 0);
        assert!(
            !rate.runs_out_first(),
            "a window with no stated reset cannot run out first"
        );
    }

    #[test]
    fn folds_a_run_of_statuses_into_stretches() {
        let mut records = vec![
            record("2026-01-01 10:00:00", 100.0),
            record("2026-01-01 10:10:00", 99.0),
            record("2026-01-01 10:20:00", 98.0),
        ];
        records[1].service_status = "minor".to_owned();
        records[2].service_status = "unknown".to_owned();

        let spans = service_status_spans(&records);
        assert_eq!(spans.len(), 3);
        assert_eq!(spans[0].status, "ok");
        assert_eq!(spans[0].readings, 1);
        assert_eq!(spans[1].status, "minor");
        assert_eq!(spans[2].status, "unknown");

        // The same status twice in a row is one stretch, not two.
        records[2].service_status = "minor".to_owned();
        let spans = service_status_spans(&records);
        assert_eq!(spans.len(), 2);
        assert_eq!(spans[1].readings, 2);
    }

    #[test]
    fn availability_counts_only_readings_that_were_read() {
        let spans = vec![
            StatusSpan {
                status: "none".to_owned(),
                readings: 9,
            },
            StatusSpan {
                status: "major".to_owned(),
                readings: 1,
            },
            // Ten unreadable readings say nothing about the service, so they
            // neither count as downtime nor leave the window.
            StatusSpan {
                status: "unknown".to_owned(),
                readings: 10,
            },
        ];
        assert_eq!(availability_percent(&spans), Some(90.0));

        let all_unreadable = vec![StatusSpan {
            status: "unknown".to_owned(),
            readings: 4,
        }];
        assert_eq!(availability_percent(&all_unreadable), None);
        assert_eq!(availability_percent(&[]), None);
    }

    #[test]
    fn a_steady_window_has_no_pace_to_report() {
        let points = vec![
            point("2026-01-01 00:00:00", 40.0),
            point("2026-01-01 12:00:00", 40.0),
        ];
        assert_eq!(window_rate(&points, 600), None);
    }

    #[test]
    fn one_reading_is_not_a_pace() {
        assert_eq!(
            window_rate(&[point("2026-01-01 00:00:00", 10.0)], 600),
            None
        );
        assert_eq!(window_rate(&[], 600), None);
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
        // A billing day past the end of a month uses that month's last day.
        assert_eq!(cycle_start(day(2026, 3, 15), 31), day(2026, 2, 28));
        assert_eq!(cycle_start(day(2026, 4, 15), 31), day(2026, 3, 31));
        assert_eq!(cycle_start(day(2026, 5, 15), 31), day(2026, 4, 30));
        // And the date itself is honoured when the month is long enough.
        assert_eq!(cycle_start(day(2026, 1, 31), 31), day(2026, 1, 31));
        assert_eq!(cycle_start(day(2026, 3, 31), 31), day(2026, 3, 31));
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

    /// Points at the given values, one minute apart, in the stored format.
    fn points(values: &[f64]) -> Vec<SubscriptionPoint> {
        values
            .iter()
            .enumerate()
            .map(|(index, used)| SubscriptionPoint {
                timestamp: format!("2026-09-16 10:{index:02}:00"),
                used: *used,
                cap: 0.0,
            })
            .collect()
    }

    #[test]
    fn a_percent_keeps_decimals_only_when_it_has_some() {
        assert_eq!(format_percent(86.0), "86%");
        assert_eq!(format_percent(100.0), "100%");
        assert_eq!(format_percent(0.0), "0%");

        // What the refinement produces, and what the previous build showed.
        assert_eq!(format_percent(70.428), "70.43%");
        assert_eq!(format_percent(41.5), "41.50%");
    }

    #[test]
    fn spending_inside_the_current_step_refines_it() {
        // The weekly window sits at 41% from 10:02 on; $0.15 has been spent
        // since, and a percent of the $30 pool is $0.30 — half a percent.
        let coarse = points(&[40.0, 40.0, 41.0, 41.0, 41.0]);
        let spent = points(&[0.0, 1.0, 1.2, 1.35, 1.35]);

        let refined = refined_percent(&coarse, &spent, 30.0).expect("a figure");
        assert!((refined - 41.5).abs() < 0.000_001, "{refined}");
    }

    #[test]
    fn nothing_spent_keeps_the_coarse_figure() {
        let coarse = points(&[40.0, 41.0, 41.0]);
        let spent = points(&[0.5, 0.5, 0.5]);

        assert_eq!(refined_percent(&coarse, &spent, 30.0), Some(41.0));
    }

    #[test]
    fn a_step_is_never_refined_past_its_own_width() {
        // $30 spent would be a hundred percents; the next poll's whole percent
        // is what says so, not this.
        let coarse = points(&[40.0, 41.0, 41.0]);
        let spent = points(&[0.0, 0.0, 30.0]);

        assert_eq!(refined_percent(&coarse, &spent, 30.0), Some(42.0));
    }

    #[test]
    fn a_month_is_refined_with_its_own_pool() {
        // The monthly pool is $60, so a percent is $0.60.
        let coarse = points(&[70.0, 70.0]);
        let spent = points(&[0.0, 0.30]);

        let refined = refined_percent(&coarse, &spent, 60.0).expect("a figure");
        assert!((refined - 70.5).abs() < 0.000_001, "{refined}");
    }

    #[test]
    fn a_window_with_no_history_has_no_figure() {
        assert_eq!(refined_percent(&[], &[], 30.0), None);
    }
}
