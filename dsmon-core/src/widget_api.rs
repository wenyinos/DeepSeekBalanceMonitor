//! The local data interface the desktop widget reads.
//!
//! The widget is a program of its own: it renders what it is given here, never
//! talks to a provider, never opens the database and never touches a key. The
//! contract — endpoints, fields, invariants — is written down in
//! `docs/INTERFACES.md`, which is also what a second implementation has to
//! follow.
//!
//! Two rules from there shape this module: the socket is bound to the loopback
//! address and nothing else, and nothing that leaves over it is a secret. There
//! is no authentication either: a local user who could reach this port can read
//! the database file directly, so a token would add surface and no safety.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use chrono::{Datelike, Local, NaiveDate};
use serde::{Deserialize, Serialize};

use crate::catalog::{self, Mode};
use crate::config::AppConfig;
use crate::history;
use crate::monitor::{MonitorHandle, Subscription};

use crate::{storage, time};

/// Port the widget reads from.
///
/// Fixed rather than discovered: the widget has no place to look a port up, and
/// 1.x's Rainmeter interface sits on 17654, so the two builds can run beside
/// each other.
pub const PORT: u16 = 18964;

/// Version of the payload format. A client that does not know a version says so
/// instead of pretending the connection failed.
pub const VERSION: u32 = 2;

/// How many days of balance history the widget may ask for, matching the
/// history page's ranges.
pub const DAYS: [u64; 3] = [1, 7, 30];
const DEFAULT_DAYS: u64 = 7;

/// Days of subscription history behind the activity heat map.
const HEATMAP_DAYS: u64 = 30;

/// Points kept per `series`. A month of readings is thousands of points and the
/// widget draws a forty-pixel sparkline: sending them all would put tens of
/// kilobytes on a request that runs every two seconds.
const MAX_SERIES_POINTS: usize = 240;

/// A read or write that stalls longer than this is given up on, so one stuck
/// client cannot hold the serving thread.
const IO_TIMEOUT: Duration = Duration::from_secs(3);

// ---------------------------------------------------------------------------
// The payload
// ---------------------------------------------------------------------------

/// Everything the widget draws one frame from.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Payload {
    pub version: u32,
    pub provider: Provider,
    pub generated_at: String,
    /// Language the application is using, for clients that do not read the
    /// configuration themselves.
    pub lang: String,
    pub checking: bool,
    pub service_status: String,
    /// When the last successful poll happened, formatted for display.
    pub last_check_at: Option<String>,
    /// How long ago that was, in seconds.
    pub last_check_sec: Option<i64>,
    pub platforms: Vec<Platform>,
}

/// Who is answering.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Provider {
    pub name: String,
    pub version: String,
}

/// One platform's readings.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Platform {
    pub key: String,
    pub display: String,
    /// `payg` for a balance, `package` for quota windows.
    pub kind: String,
    pub balances: Vec<BalanceEntry>,
    pub rate: Option<Rate>,
    pub windows: Vec<Window>,
    pub series: Vec<Point>,
    pub daily: Vec<Day>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BalanceEntry {
    pub currency: String,
    pub total_balance: f64,
    pub topped_up_balance: f64,
    pub granted_balance: f64,
}

/// The burn rate, as the status page shows it: a seven-day average.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Rate {
    pub hourly_rate: f64,
    pub busy_hours_left: f64,
    pub currency: String,
}

/// One quota window. The name is an i18n key: the client translates it, so the
/// wording has a single source.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Window {
    pub name_key: String,
    pub usage_percent: f64,
    pub reset_in_sec: i64,
}

/// A point of the balance curve: epoch seconds and the total at that moment.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Point {
    pub t: i64,
    pub v: f64,
}

/// One day of the activity heat map. `weekday` is 0 for Monday, so the client
/// can lay the days out without a date library.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Day {
    pub date: String,
    pub used: f64,
    pub weekday: u32,
}

/// Builds the payload for one request.
///
/// `days` only affects `series`; the rate is the same seven-day figure the
/// status page shows, and clients are told not to recompute it.
pub fn payload(snapshot: &crate::monitor::Snapshot, config: &AppConfig, days: u64) -> Payload {
    let days = if DAYS.contains(&days) {
        days
    } else {
        DEFAULT_DAYS
    };
    let platforms = catalog::PLATFORMS
        .iter()
        .filter(|meta| meta.implemented && is_configured(meta.key))
        .map(|meta| platform(snapshot, meta, days))
        .collect();

    Payload {
        version: VERSION,
        provider: Provider {
            name: "dsmon2".to_owned(),
            version: crate::VERSION.to_owned(),
        },
        generated_at: time::now(),
        lang: config.ui_language.clone(),
        checking: snapshot.checking,
        service_status: snapshot.service_status.clone(),
        last_check_at: snapshot.last_check.map(time::format_local),
        last_check_sec: snapshot
            .last_check
            .map(|checked| (Local::now() - checked).num_seconds().max(0)),
        platforms,
    }
}

/// Whether the platform holds a key. The sidebar filters by the same rule, and
/// the two must agree: the widget shows exactly what the application would.
fn is_configured(key: &str) -> bool {
    matches!(storage::read_secret(key), Ok(Some(_)))
}

fn platform(
    snapshot: &crate::monitor::Snapshot,
    meta: &catalog::PlatformMeta,
    days: u64,
) -> Platform {
    let mut platform = Platform {
        key: meta.key.to_owned(),
        display: meta.display_name.to_owned(),
        kind: meta.mode.as_config().to_owned(),
        ..Platform::default()
    };

    match meta.mode {
        Mode::Payg => {
            if let Some(balances) = snapshot.balances.get(meta.key) {
                platform.balances = balances
                    .iter()
                    .map(|(currency, balance)| BalanceEntry {
                        currency: currency.clone(),
                        total_balance: balance.total_balance,
                        topped_up_balance: balance.topped_up_balance,
                        granted_balance: balance.granted_balance,
                    })
                    .collect();
            }
            platform.rate = snapshot.consumption_rates.get(meta.key).map(|rate| Rate {
                hourly_rate: rate.hourly_rate,
                busy_hours_left: rate.busy_hours_left,
                currency: rate.currency.clone(),
            });
            platform.series = balance_series(meta.key, days);
        }
        Mode::Package => {
            // A window the provider does not report is left out; the client
            // draws the ones it gets and nothing for the rest, which keeps
            // "not offered" apart from "zero used".
            if let Some(Subscription::Loaded(quota)) = snapshot.packages.get(meta.key) {
                platform.windows = meta
                    .windows
                    .iter()
                    .filter_map(|name| {
                        quota.get(*name).map(|window| Window {
                            name_key: catalog::window_label_key(name).to_owned(),
                            usage_percent: window.usage_percent,
                            reset_in_sec: window.reset_in_sec,
                        })
                    })
                    .collect();
            }
            platform.daily = activity(meta.key);
        }
    }

    platform
}

/// The balance curve over `days`, thinned to what a sparkline needs.
fn balance_series(platform: &str, days: u64) -> Vec<Point> {
    let records = match storage::history_records(platform, days, None, 20_000) {
        Ok(records) => records,
        Err(_) => return Vec::new(),
    };
    let points: Vec<Point> = records
        .iter()
        .filter_map(|record| {
            let moment = time::parse_local(&record.timestamp)?;
            Some(Point {
                t: moment.timestamp(),
                v: record.total,
            })
        })
        .collect();
    thinned(points)
}

/// Evenly thins a series, always keeping the last point.
fn thinned(points: Vec<Point>) -> Vec<Point> {
    if points.len() <= MAX_SERIES_POINTS {
        return points;
    }
    let step = points.len().div_ceil(MAX_SERIES_POINTS);
    let last = points.len() - 1;
    points
        .into_iter()
        .enumerate()
        .filter(|(index, _)| index % step == 0 || *index == last)
        .map(|(_, point)| point)
        .collect()
}

/// Per-day consumption, as the heat map draws it.
fn activity(platform: &str) -> Vec<Day> {
    let points = match storage::subscription_usage_history(platform, HEATMAP_DAYS) {
        Ok(points) => points,
        Err(_) => return Vec::new(),
    };
    history::daily_usage(&points)
        .into_iter()
        .map(|usage| Day {
            weekday: NaiveDate::parse_from_str(&usage.date, "%Y-%m-%d")
                .map(|date| date.weekday().num_days_from_monday())
                .unwrap_or(0),
            date: usage.date,
            used: usage.used,
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Serving
// ---------------------------------------------------------------------------

/// A running interface. Dropping it closes the socket and ends the thread.
pub struct Server {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    port: u16,
}

impl Server {
    /// Address the interface answers on.
    pub fn url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        // The thread is parked in `accept`, so it is woken with a connection of
        // its own rather than waited for.
        let _ = TcpStream::connect((Ipv4Addr::LOCALHOST, self.port));
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Starts serving. Binding happens here rather than in the thread so a port
/// that cannot be taken is reported to the caller, which logs it and carries on
/// without the interface.
pub fn start(handle: MonitorHandle) -> Result<Server, String> {
    start_on(handle, PORT)
}

/// Starts serving on `port`, with 0 meaning "any free one" — which is how the
/// tests get a socket without fighting a running application for 18964.
fn start_on(handle: MonitorHandle, port: u16) -> Result<Server, String> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, port)).map_err(|error| {
        format!("the local interface could not listen on 127.0.0.1:{port}: {error}")
    })?;
    let port = listener
        .local_addr()
        .map(|address| address.port())
        .unwrap_or(port);
    let stop = Arc::new(AtomicBool::new(false));
    let thread_stop = Arc::clone(&stop);

    let thread = thread::Builder::new()
        .name("dsmon-widget-api".to_owned())
        .spawn(move || {
            for stream in listener.incoming() {
                if thread_stop.load(Ordering::Relaxed) {
                    return;
                }
                if let Ok(mut stream) = stream {
                    // One request at a time is enough — they are tiny and come
                    // from the same machine — but a client that stalls must not
                    // hold the loop, hence the timeouts.
                    let _ = stream.set_read_timeout(Some(IO_TIMEOUT));
                    let _ = stream.set_write_timeout(Some(IO_TIMEOUT));
                    let _ = serve_one(&handle, &mut stream);
                }
            }
        })
        .map_err(|error| error.to_string())?;

    Ok(Server {
        stop,
        thread: Some(thread),
        port,
    })
}

fn serve_one(handle: &MonitorHandle, stream: &mut TcpStream) -> std::io::Result<()> {
    let line = match request_line(stream) {
        Some(line) => line,
        None => return Ok(()),
    };

    match parse_request(&line) {
        Request::Status { days } => {
            let payload = current(handle, days);
            let body = serde_json::to_string(&payload).unwrap_or_else(|_| "{}".to_owned());
            respond(stream, "200 OK", &body)
        }
        Request::Check { days } => {
            // Asked for a poll, but answered straight away: the widget shows the
            // reading it has and picks the new one up on its next request.
            handle.refresh();
            let payload = current(handle, days);
            let body = serde_json::to_string(&payload).unwrap_or_else(|_| "{}".to_owned());
            respond(stream, "200 OK", &body)
        }
        Request::NotFound => respond(stream, "404 Not Found", r#"{"error":"unknown path"}"#),
        Request::NotAllowed => respond(
            stream,
            "405 Method Not Allowed",
            r#"{"error":"only GET is served"}"#,
        ),
    }
}

/// The configuration is read per request, so a language or settings change in
/// the application reaches the widget without either side talking to the other.
fn current(handle: &MonitorHandle, days: u64) -> Payload {
    payload(&handle.snapshot(), &AppConfig::load(), days)
}

/// Reads the request line, then drains the headers so the client is not left
/// writing into a closed socket.
fn request_line(stream: &TcpStream) -> Option<String> {
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    if reader.read_line(&mut line).ok()? == 0 {
        return None;
    }
    let mut header = String::new();
    // Bounded: a request with no blank line must not be read forever.
    for _ in 0..64 {
        header.clear();
        match reader.read_line(&mut header) {
            Ok(0) => break,
            Ok(_) if header.trim().is_empty() => break,
            Ok(_) => continue,
            Err(_) => break,
        }
    }
    Some(line.trim_end().to_owned())
}

fn respond(stream: &mut TcpStream, status: &str, body: &str) -> std::io::Result<()> {
    write!(
        stream,
        "HTTP/1.1 {status}\r\n\
         Content-Type: application/json; charset=utf-8\r\n\
         Cache-Control: no-store\r\n\
         Content-Length: {}\r\n\
         Connection: close\r\n\r\n{body}",
        body.len()
    )?;
    stream.flush()
}

/// What a request line asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Request {
    Status { days: u64 },
    Check { days: u64 },
    NotFound,
    NotAllowed,
}

fn parse_request(line: &str) -> Request {
    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or_default();
    let target = parts.next().unwrap_or_default();

    if method != "GET" {
        return Request::NotAllowed;
    }
    let (path, query) = match target.split_once('?') {
        Some((path, query)) => (path, query),
        None => (target, ""),
    };
    let days = query
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .find(|(key, _)| *key == "days")
        .and_then(|(_, value)| value.parse::<u64>().ok())
        .filter(|days| DAYS.contains(days))
        .unwrap_or(DEFAULT_DAYS);

    match path {
        "/widget-status" => Request::Status { days },
        "/check" => Request::Check { days },
        _ => Request::NotFound,
    }
}

// ---------------------------------------------------------------------------
// Reading it, for a client on this machine
// ---------------------------------------------------------------------------

/// Fetches a path from a running application and returns the body.
///
/// Hand-written rather than sent through the HTTP client the rest of the crate
/// uses: that one honours the proxy settings, and a request to the loopback
/// address has no business going anywhere near a proxy.
pub fn get(port: u16, path: &str) -> Result<String, String> {
    let mut stream = TcpStream::connect_timeout(
        &(Ipv4Addr::LOCALHOST, port).into(),
        Duration::from_millis(600),
    )
    .map_err(|error| error.to_string())?;
    stream
        .set_read_timeout(Some(IO_TIMEOUT))
        .map_err(|error| error.to_string())?;

    write!(stream, "GET {path} HTTP/1.0\r\nConnection: close\r\n\r\n")
        .map_err(|error| error.to_string())?;

    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .map_err(|error| error.to_string())?;

    let (head, body) = response
        .split_once("\r\n\r\n")
        .ok_or_else(|| "the reply had no body".to_owned())?;
    let status = head.lines().next().unwrap_or_default();
    if !status.contains(" 200 ") {
        return Err(format!("the application answered {status}"));
    }
    Ok(body.to_owned())
}

/// Fetches a path and parses the payload, which is what a client wants.
///
/// Parsing lives here so that a client crate needs no JSON library of its own —
/// the widget reads a typed value and its dependency list stays as it was.
pub fn fetch(port: u16, path: &str) -> Result<Payload, String> {
    let body = get(port, path)?;
    serde_json::from_str(&body).map_err(|error| format!("the payload could not be read: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Balance, Balances};
    use crate::monitor::Snapshot;

    fn point(index: usize) -> Point {
        Point {
            t: index as i64,
            v: index as f64,
        }
    }

    #[test]
    fn a_request_line_says_what_to_serve() {
        assert_eq!(
            parse_request("GET /widget-status HTTP/1.1"),
            Request::Status { days: DEFAULT_DAYS }
        );
        assert_eq!(
            parse_request("GET /widget-status?days=30 HTTP/1.1"),
            Request::Status { days: 30 }
        );
        assert_eq!(
            parse_request("GET /check?lang=zh HTTP/1.1"),
            Request::Check { days: DEFAULT_DAYS }
        );
        assert_eq!(parse_request("GET /nonsense HTTP/1.1"), Request::NotFound);
        assert_eq!(
            parse_request("POST /widget-status HTTP/1.1"),
            Request::NotAllowed
        );
        // A range nobody offers, and a range that is not a number, both fall
        // back rather than erroring.
        assert_eq!(
            parse_request("GET /widget-status?days=9 HTTP/1.1"),
            Request::Status { days: DEFAULT_DAYS }
        );
        assert_eq!(
            parse_request("GET /widget-status?days=soon HTTP/1.1"),
            Request::Status { days: DEFAULT_DAYS }
        );
    }

    #[test]
    fn a_long_series_is_thinned_and_keeps_its_last_point() {
        let points: Vec<Point> = (0..10_000).map(point).collect();
        let kept = thinned(points);
        assert!(kept.len() <= MAX_SERIES_POINTS + 1, "{}", kept.len());
        assert_eq!(kept.last().map(|point| point.t), Some(9_999));
        assert_eq!(kept.first().map(|point| point.t), Some(0));

        let short: Vec<Point> = (0..10).map(point).collect();
        assert_eq!(thinned(short.clone()), short);
    }

    /// The payload has to survive a client that is older or newer than the
    /// application: missing fields take defaults, unknown ones are ignored.
    #[test]
    fn a_payload_tolerates_missing_and_unknown_fields() {
        let payload: Payload = serde_json::from_str(r#"{"version":2,"unknown":true}"#)
            .expect("a payload with no platforms parses");
        assert_eq!(payload.version, 2);
        assert!(payload.platforms.is_empty());
        assert_eq!(payload.lang, "");
        assert_eq!(payload.last_check_sec, None);
    }

    /// The whole path a widget takes: bind, ask, parse the reply.
    #[test]
    fn a_request_is_answered_over_the_socket() {
        let _guard = crate::test_support::state_in_a_scratch_directory();
        let monitor = crate::monitor::Monitor::start(AppConfig::default());
        let server = start_on(monitor.handle(), 0).expect("the interface listens");

        let body = get(server.port, "/widget-status?days=1").expect("the answer arrives");
        let payload: Payload = serde_json::from_str(&body).expect("the answer is a payload");
        assert_eq!(payload.version, VERSION);
        assert_eq!(payload.provider.name, "dsmon2");

        // A path that is not served is refused rather than answered with luck.
        assert!(get(server.port, "/nonsense").is_err());
    }

    #[test]
    fn the_payload_carries_the_readings_the_widget_draws() {
        let _guard = crate::test_support::state_in_a_scratch_directory();

        let mut snapshot = Snapshot {
            checking: true,
            service_status: "none".to_owned(),
            ..Snapshot::default()
        };
        let mut balances = Balances::new();
        balances.insert(
            "CNY".to_owned(),
            Balance {
                total_balance: 12.5,
                granted_balance: 2.5,
                topped_up_balance: 10.0,
            },
        );
        snapshot.balances.insert("deepseek".to_owned(), balances);

        let config = AppConfig::default();
        let payload = payload(&snapshot, &config, 7);

        assert_eq!(payload.version, VERSION);
        assert_eq!(payload.provider.name, "dsmon2");
        assert!(payload.checking);
        assert_eq!(payload.lang, config.ui_language);

        // No key is stored in this scratch directory, so the widget is told
        // about no platform at all — the same rule the sidebar follows.
        assert!(payload.platforms.is_empty());
    }
}
