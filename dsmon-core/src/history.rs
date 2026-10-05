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

/// Estimates the burn rate from `total` readings.
///
/// Three rules keep idle periods out of the average:
/// 1. a top-up starts a new interval;
/// 2. a gap longer than the busy threshold slices the interval;
/// 3. a flat run longer than the threshold is discarded.
///
/// The rate is read from `total` (total_balance = topped_up + granted), NOT
/// from `topped`: consumption can be drawn from the granted bucket, so a
/// topped-only series stays flat — or negative — while the real usable
/// balance drops, and the rate then reads zero. A granted arrival is an
/// increase and gets sliced as a top-up, same as a recharge.
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
            Ok((timestamp, record.total))
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
    //
    // Minimum length for a rate sample: half the poll interval, never under a
    // minute. Shorter intervals come from extra or manual checks, not from a
    // real measurement span — extrapolating a small balance delta over
    // seconds yields an absurd hourly rate, and when such an interval is the
    // only one carrying a drop it owns the whole weighted average. (The old
    // floor was 0.01 h — 36 seconds — which let a single 46-second interval
    // with a 0.08 drop report 6.26/h by itself.)
    let min_sample_hours = (60.0_f64).max(0.5 * interval_minutes as f64 * 60.0) / 3600.0;
    let mut total_weight = 0.0;
    let mut weighted_sum = 0.0;
    for (start_value, start_time, end_value, end_time) in intervals {
        if end_value >= start_value {
            continue;
        }
        let delta_hours = (end_time - start_time).num_seconds() as f64 / 3600.0;
        if delta_hours < min_sample_hours {
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

    // Clamp the remaining-quota base at 0: a non-positive balance (account in
    // arrears, e.g. before a grant arrives) would otherwise yield a negative
    // "hours left" — meaningless as a figure.
    let remaining = parsed.last().expect("records is not empty").1.max(0.0);
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

/// What one day has spent of a quota window: the day's positive rises added
/// up, in percentage points — the package counterpart of the balance day
/// figure, aggregated the same way. A drop (a renewal) never subtracts; the
/// points handed in are already the day's own readings.
pub fn window_day_spend(points: &[SubscriptionPoint]) -> f64 {
    let mut spent = 0.0;
    for pair in points.windows(2) {
        let rise = pair[1].percent() - pair[0].percent();
        if rise > 0.0 {
            spent += rise;
        }
    }
    spent
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

/// Where between two whole percents a coarse window really is, refined by the
/// money the five-hour window spent — the model the earlier build settled on
/// after measuring it.
///
/// The endpoint reports the weekly and monthly windows as whole percents that
/// are ROUNDED (the true usage lies within obs±0.5), and the five-hour window
/// as percents of its own pool. The pools are known — $12 per five hours, $30
/// a week, $60 a month — so the money the five-hour window burned between two
/// polls is a fixed share of the target window, and it places the estimate
/// inside the band the rounded observation allows. The band is trimmed by
/// 0.06 so a display that keeps one or two decimals never rounds across a
/// half boundary.
///
/// `readings` are `(five-hour used, target used)` pairs, one per poll,
/// oldest first. Strictly causal: only real spending moves the estimate, only
/// forward, and a window reset (the target reading drops) re-anchors it at
/// the rounded observation.
pub fn refined_percent(readings: &[(f64, f64)], five_pool: f64, target_pool: f64) -> Option<f64> {
    /// Keeps a refined value off the exact half boundary.
    const DISPLAY_MARGIN: f64 = 0.06;

    let first = readings.first()?;
    if target_pool <= 0.0 {
        return Some(first.1);
    }

    // The continuous usage estimate. Each point moves only inside its own
    // observation's band, which is what keeps every refined figure consistent
    // with the rounded integer the endpoint reported.
    let mut estimate = first.1;
    for pair in readings.windows(2) {
        let (five_before, obs_before) = pair[0];
        let (five_now, obs) = pair[1];

        if obs < obs_before {
            // A reset: the window fell back to a new cycle, so re-anchor at
            // the rounded observation.
            estimate = obs;
            continue;
        }

        // Only real five-hour spend in this interval advances the estimate —
        // never smear a rate over every row, which overshoots the observed
        // total. The known pool ratio converts points to money exactly.
        if five_now > five_before && five_pool > 0.0 {
            let spent = (five_now - five_before) / 100.0 * five_pool;
            estimate += spent * 100.0 / target_pool;
        }
        // Round semantics: the estimate stays inside (obs-0.5, obs+0.5). The
        // lower bound rescues it when the coarse observation stepped up (the
        // endpoint saw a new rounded integer, so usage must be inside its
        // band); the upper bound keeps the line from claiming more
        // consumption than the observed integer allows.
        estimate = estimate.clamp(obs - 0.5 + DISPLAY_MARGIN, obs + 0.5 - DISPLAY_MARGIN);
    }

    // Two decimals, the format the interface shows.
    let remaining = ((100.0 - estimate).clamp(0.0, 100.0) * 100.0).round() / 100.0;
    Some(100.0 - remaining)
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

    /// A record whose total differs from its topped bucket, for the cases
    /// where the granted balance moves.
    fn record_of(timestamp: &str, total: f64, topped: f64) -> HistoryRecord {
        HistoryRecord {
            total,
            ..record(timestamp, topped)
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

    /// The rate must be read from `total`: consumption drawn from the granted
    /// bucket leaves a topped-only series flat, and the rate would read zero.
    #[test]
    fn the_rate_reads_total_so_granted_balance_counts() {
        // Topped sits at 10 all along; the granted bucket is drawn down by
        // 0.6 an hour.
        let records: Vec<HistoryRecord> = (0..7)
            .map(|step| {
                let minutes = step * 10;
                let timestamp =
                    format!("2026-01-01 {:02}:{:02}:00", 10 + minutes / 60, minutes % 60);
                record_of(&timestamp, 16.0 - step as f64 * 0.6, 10.0)
            })
            .collect();

        let rate = consumption_rate_from_records(&records, 10)
            .unwrap()
            .expect("a rate is produced");
        assert!(
            (rate.hourly_rate - 3.6).abs() < 0.5,
            "expected about 3.6/hour from total, got {}",
            rate.hourly_rate
        );
        assert!(rate.busy_hours_left > 0.0);
    }

    /// A non-positive balance is treated as spent: "hours left" must not go
    /// negative.
    #[test]
    fn a_non_positive_balance_yields_no_hours_left() {
        let records = vec![
            record_of("2026-01-01 10:00:00", -0.10, -0.10),
            record_of("2026-01-01 10:10:00", -0.23, -0.23),
        ];

        let rate = consumption_rate_from_records(&records, 10)
            .unwrap()
            .expect("a rate is produced");
        assert_eq!(rate.busy_hours_left, 0.0);
    }

    /// A top-up, then a manual re-check 46 seconds later: the drop is real
    /// but the span is not a measurement, and extrapolated it reads 6.26/h by
    /// itself. It must not be sampled.
    #[test]
    fn a_short_interval_alone_yields_no_rate() {
        let records = vec![
            record("2026-01-01 10:00:00", 10.0),
            record("2026-01-01 10:10:00", 10.5),
            record("2026-01-01 10:10:46", 10.42),
        ];

        assert!(consumption_rate_from_records(&records, 10)
            .unwrap()
            .is_none());
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
    fn a_day_of_window_spend_adds_the_rises() {
        let points = vec![
            point("2026-01-01 00:10:00", 20.0),
            point("2026-01-01 08:00:00", 24.0),
            // A renewal: the drop is not negative spend.
            point("2026-01-01 16:00:00", 21.0),
            point("2026-01-01 20:00:00", 22.5),
        ];

        assert!((window_day_spend(&points) - 5.5).abs() < 1e-9);
    }

    #[test]
    fn one_reading_has_no_window_spend() {
        assert_eq!(window_day_spend(&[]), 0.0);
        assert_eq!(window_day_spend(&[point("2026-01-01 10:00:00", 10.0)]), 0.0);
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

    /// One poll's `(five-hour used, target used)` reading.
    fn reading(five_hour: f64, target: f64) -> (f64, f64) {
        (five_hour, target)
    }

    /// Refines every prefix of the series — what the running model sees at
    /// each point — and returns the figures.
    fn refined_prefixes(readings: &[(f64, f64)], five_pool: f64, target_pool: f64) -> Vec<f64> {
        (1..=readings.len())
            .map(|end| refined_percent(&readings[..end], five_pool, target_pool).expect("a figure"))
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
    fn a_window_with_no_history_has_no_figure() {
        assert_eq!(refined_percent(&[], 12.0, 30.0), None);
    }

    #[test]
    fn nothing_spent_keeps_the_observation() {
        let readings = [
            reading(30.0, 41.0),
            reading(30.0, 41.0),
            reading(30.0, 41.0),
        ];

        assert_eq!(refined_percent(&readings, 12.0, 30.0), Some(41.0));
    }

    #[test]
    fn spending_inside_the_band_refines_it() {
        // The weekly window sits at 41; the five-hour window reports in
        // percents of its $12 pool, and 2.5 points of it are $0.30 — a whole
        // point of the $30 weekly pool. The rounded observation only allows
        // half a point, so the estimate stops at 41.44.
        let readings = [reading(30.0, 41.0), reading(32.5, 41.0)];

        let refined = refined_percent(&readings, 12.0, 30.0).expect("a figure");
        assert!((refined - 41.44).abs() < 0.000_001, "{refined}");
    }

    #[test]
    fn a_step_is_never_refined_past_its_band() {
        // $30 spent would be a hundred points; the rounded observation caps
        // the estimate at 41.44 regardless.
        let readings = [reading(0.0, 41.0), reading(100.0, 41.0)];

        let refined = refined_percent(&readings, 12.0, 30.0).expect("a figure");
        assert!((refined - 41.44).abs() < 0.000_001, "{refined}");
    }

    #[test]
    fn a_month_is_refined_with_its_own_pool() {
        // The monthly pool is $60, so the same $0.30 is half a point — past
        // the band's edge, so it stops at 70.44.
        let readings = [reading(0.0, 70.0), reading(2.5, 70.0)];

        let refined = refined_percent(&readings, 12.0, 60.0).expect("a figure");
        assert!((refined - 70.44).abs() < 0.000_001, "{refined}");
    }

    #[test]
    fn a_reset_re_anchors_the_estimate() {
        // The target drops: a new cycle starts at the rounded observation.
        let readings = [reading(0.0, 80.0), reading(10.0, 2.0), reading(12.5, 2.0)];

        let refined = refined_percent(&readings, 12.0, 30.0).expect("a figure");
        assert!((refined - 2.44).abs() < 0.000_001, "{refined}");
    }

    #[test]
    fn a_five_hour_reset_steps_the_estimate_up() {
        // The five-hour window reset (its reading fell), so nothing was
        // spent; the observation stepped up, and the band's lower bound
        // rescues the estimate.
        let readings = [reading(43.0, 34.0), reading(5.0, 35.0)];

        let refined = refined_percent(&readings, 12.0, 30.0).expect("a figure");
        assert!((refined - 34.56).abs() < 0.000_001, "{refined}");
    }

    /// The regression the model was measured for: with spend in between and a
    /// five-hour reset boundary, every refined point stays within half a point
    /// of its own rounded observation.
    #[test]
    fn every_point_stays_inside_the_round_band() {
        let readings = [
            reading(30.0, 33.0),
            reading(32.0, 33.0),
            reading(35.0, 34.0),
            reading(41.0, 34.0),
            reading(43.0, 34.0),
            reading(5.0, 35.0),
            reading(10.0, 36.0),
        ];

        for (index, refined) in refined_prefixes(&readings, 12.0, 60.0).iter().enumerate() {
            let obs = readings[index].1;
            assert!(
                (refined - obs).abs() <= 0.5,
                "prefix {}: refined {refined} vs observation {obs}",
                index + 1
            );
        }
    }
}
