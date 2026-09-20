//! Background polling: fetches balances and quotas, stores history, and
//! publishes a snapshot the interface renders.
//!
//! One thread does all the network work. The interface never blocks on I/O; it
//! reads the snapshot and sends [`Command::Refresh`] when the user asks for a
//! manual poll.

use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use chrono::{DateTime, Local};

use crate::config::AppConfig;
use crate::model::{Balances, ConsumptionRate, PackageQuota, WindowRates};
use crate::{demo, history, platforms, storage, update};

/// How often the release page is asked for a newer version.
const UPDATE_CHECK_HOURS: i64 = 24;

/// Outcome of a subscription lookup.
///
/// The three cases are kept apart so the interface can say "not configured"
/// when there is no key and show the actual reason when a configured key
/// fails, instead of blaming the setup for a network or credential problem.
#[derive(Debug, Clone, Default, PartialEq)]
pub enum Subscription<T> {
    /// No API key is stored for this provider.
    #[default]
    NotConfigured,
    /// The quota was read successfully.
    Loaded(T),
    /// The lookup failed; carries the reason reported by the client.
    Failed(String),
}

/// Everything the interface needs to draw one frame.
#[derive(Debug, Clone, Default)]
pub struct Snapshot {
    /// Readings per platform key, for the balance providers.
    pub balances: std::collections::BTreeMap<String, Balances>,
    /// Why a platform's last reading failed, keyed the same way.
    pub balance_errors: std::collections::BTreeMap<String, String>,
    pub service_status: String,
    /// Burn-rate estimates per platform.
    pub consumption_rates: std::collections::BTreeMap<String, ConsumptionRate>,
    /// The package plans' quota windows, keyed by platform.
    pub packages: std::collections::BTreeMap<String, Subscription<PackageQuota>>,
    /// How fast each plan's windows are being spent, from the logged readings.
    pub window_rates: WindowRates,
    /// A newer published version, when the release page named one.
    pub newer_version: Option<String>,
    pub last_check: Option<DateTime<Local>>,
    /// Set when the last poll failed; cleared by the next success.
    pub last_error: Option<String>,
    /// True while a poll is in flight.
    pub checking: bool,
    /// Whether the configured key selects the demo data set.
    pub demo: bool,
    /// What DeepSeek has cost today, and in which currency — `None` when there
    /// is nothing to compare, which is the case for a day holding a single
    /// reading, for a balance that went up, and before the first poll.
    pub today_spend: Option<(String, f64)>,
}

impl Snapshot {
    /// Whether today's spending has passed the line the user set.
    ///
    /// A line of zero means the alert is off, and a day with nothing to
    /// compare (`today_spend` is `None`) is never brisk.
    pub fn spending_is_brisk(&self, config: &crate::config::AppConfig) -> bool {
        if config.brisk_threshold_yuan <= 0.0 {
            return false;
        }
        self.today_spend
            .as_ref()
            .is_some_and(|(_, spent)| *spent >= config.brisk_threshold_yuan)
    }
}

/// Commands the interface sends to the polling thread.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    /// Poll the balance, the service health and the subscriptions.
    Refresh,
    /// Poll only the subscription quotas.
    RefreshSubscriptions,
    Stop,
}

/// Handle to the polling thread. Dropping it stops the thread.
pub struct Monitor {
    snapshot: Arc<Mutex<Snapshot>>,
    commands: Sender<Command>,
    thread: Option<JoinHandle<()>>,
}

impl Monitor {
    /// Starts polling at the interval from `config`.
    pub fn start(config: AppConfig) -> Self {
        let snapshot = Arc::new(Mutex::new(Snapshot::default()));
        let (commands, receiver) = mpsc::channel();

        let worker_snapshot = Arc::clone(&snapshot);
        let thread = thread::Builder::new()
            .name("dsmon-poll".to_owned())
            .spawn(move || run(config, receiver, worker_snapshot))
            .ok();

        Self {
            snapshot,
            commands,
            thread,
        }
    }

    /// The latest snapshot.
    pub fn snapshot(&self) -> Snapshot {
        self.snapshot
            .lock()
            .map(|guard| guard.clone())
            .unwrap_or_default()
    }

    /// Asks the thread to poll everything immediately.
    pub fn refresh(&self) {
        let _ = self.commands.send(Command::Refresh);
    }

    /// Asks the thread to refresh only the subscription quotas.
    pub fn refresh_subscriptions(&self) {
        let _ = self.commands.send(Command::RefreshSubscriptions);
    }
}

/// A second handle on the same poller, for whoever serves the local interface.
///
/// It reads the latest reading and may ask for another poll, but it does not
/// own the thread: dropping it leaves the poller running, which is what the
/// interface needs when it winds down on its own.
#[derive(Clone)]
pub struct MonitorHandle {
    snapshot: Arc<Mutex<Snapshot>>,
    commands: Sender<Command>,
}

impl MonitorHandle {
    /// The latest snapshot, as [`Monitor::snapshot`] returns it.
    pub fn snapshot(&self) -> Snapshot {
        self.snapshot
            .lock()
            .map(|guard| guard.clone())
            .unwrap_or_default()
    }

    /// Asks for a poll now, as [`Monitor::refresh`] does.
    pub fn refresh(&self) {
        let _ = self.commands.send(Command::Refresh);
    }
}

impl Monitor {
    /// A handle for another thread, paired with the one this type owns.
    pub fn handle(&self) -> MonitorHandle {
        MonitorHandle {
            snapshot: Arc::clone(&self.snapshot),
            commands: self.commands.clone(),
        }
    }
}

impl Drop for Monitor {
    fn drop(&mut self) {
        let _ = self.commands.send(Command::Stop);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Which part of the snapshot a pass refreshes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Scope {
    Everything,
    Subscriptions,
}

/// The polling loop: one pass at start-up, then a pass per interval or command.
fn run(config: AppConfig, receiver: Receiver<Command>, snapshot: Arc<Mutex<Snapshot>>) {
    let mut config = config;
    let mut update_checked: Option<DateTime<Local>> = None;

    poll_once(&config, &snapshot, Scope::Everything);
    check_for_update(&config, &snapshot, &mut update_checked);

    loop {
        let interval = Duration::from_secs(config.interval_minutes.max(1) * 60);
        let scope = match receiver.recv_timeout(interval) {
            Ok(Command::Refresh) => Scope::Everything,
            Ok(Command::RefreshSubscriptions) => Scope::Subscriptions,
            Ok(Command::Stop) | Err(RecvTimeoutError::Disconnected) => return,
            Err(RecvTimeoutError::Timeout) => Scope::Everything,
        };

        // Pick up configuration changes made in the settings page.
        config = AppConfig::load();

        match scope {
            Scope::Everything => poll_once(&config, &snapshot, Scope::Everything),
            Scope::Subscriptions => poll_once(&config, &snapshot, Scope::Subscriptions),
        }
        check_for_update(&config, &snapshot, &mut update_checked);
    }
}

/// Asks the release page whether this build has been superseded, once a day.
///
/// It counts as the day's attempt whether or not it answers — a network that is
/// down should not be asked again on every poll — and a version that is not
/// newer clears what an earlier answer left behind.
fn check_for_update(
    config: &AppConfig,
    snapshot: &Arc<Mutex<Snapshot>>,
    last: &mut Option<DateTime<Local>>,
) {
    if !config.update_check_enabled {
        return;
    }
    let now = Local::now();
    if last.is_some_and(|checked| (now - checked).num_hours() < UPDATE_CHECK_HOURS) {
        return;
    }
    *last = Some(now);

    let Ok(Some(latest)) = update::latest_version(platforms::effective_proxy(config)) else {
        return;
    };

    let newer = update::is_newer(&latest, crate::VERSION).then_some(latest);
    if let Ok(mut guard) = snapshot.lock() {
        guard.newer_version = newer;
    }
}

/// Performs one poll and publishes the result.
fn poll_once(config: &AppConfig, snapshot: &Arc<Mutex<Snapshot>>, scope: Scope) {
    set_checking(snapshot, true);

    // A subscription pass leaves the balance and the service health as they are.
    if scope == Scope::Subscriptions {
        poll_subscriptions(config, snapshot);
        return;
    }

    let result = gather(config);
    let mut guard = match snapshot.lock() {
        Ok(guard) => guard,
        Err(_) => return,
    };
    guard.checking = false;

    match result {
        Ok(mut outcome) => {
            // Recorded before anything is refined: the history has to hold what
            // the endpoint actually said, or the next refinement would compare
            // a refined figure against a refined one and drift.
            record_subscription_usage(&outcome.packages);
            refine_coarse_windows(&mut outcome.packages);
            guard.window_rates = package_rates(&outcome.packages);
            guard.balances = outcome.balances;
            guard.balance_errors = outcome.balance_errors;
            guard.service_status = outcome.service_status;
            guard.consumption_rates = outcome.consumption_rates;
            guard.packages = outcome.packages;
            guard.today_spend = today_spend();
            guard.last_error = None;
            guard.last_check = Some(Local::now());
        }
        Err(error) => {
            guard.last_error = Some(error);
            guard.last_check = Some(Local::now());
        }
    }
}

/// What today has cost the DeepSeek account: the first reading of the day
/// against the last, in the currency they share.
///
/// The window is the same rolling day the history page calls "1d", so this
/// figure and that page agree. A day holding one reading has nothing to
/// compare, and a balance that went up (a top-up) is not spending: both say
/// nothing rather than a confident zero.
fn today_spend() -> Option<(String, f64)> {
    let records = storage::history_records(storage::KEY_DEEPSEEK, 1, None, 2000).ok()?;
    let first = records.first()?;
    let last = records.last()?;
    if first.currency != last.currency {
        return None;
    }

    let spent = first.total - last.total;
    (spent > 0.0).then(|| (last.currency.clone(), spent))
}

/// Refreshes the package plans' quotas alone.
fn poll_subscriptions(config: &AppConfig, snapshot: &Arc<Mutex<Snapshot>>) {
    let packages = gather_packages(platforms::effective_proxy(config));
    record_subscription_usage(&packages);
    let window_rates = package_rates(&packages);

    if let Ok(mut guard) = snapshot.lock() {
        guard.checking = false;
        guard.packages = packages;
        guard.window_rates = window_rates;
        guard.last_check = Some(Local::now());
    }
}

fn set_checking(snapshot: &Arc<Mutex<Snapshot>>, value: bool) {
    if let Ok(mut guard) = snapshot.lock() {
        guard.checking = value;
    }
}

/// What one successful poll produced.
struct Outcome {
    balances: std::collections::BTreeMap<String, Balances>,
    balance_errors: std::collections::BTreeMap<String, String>,
    service_status: String,
    consumption_rates: std::collections::BTreeMap<String, ConsumptionRate>,
    packages: std::collections::BTreeMap<String, Subscription<PackageQuota>>,
}

fn gather(config: &AppConfig) -> Result<Outcome, String> {
    let api_key = storage::read_secret(storage::KEY_DEEPSEEK)
        .ok()
        .flatten()
        .unwrap_or_default();

    if demo::is_enabled(&api_key) {
        return gather_demo(config);
    }

    let proxy = platforms::effective_proxy(config);
    let (balances, balance_errors) = gather_balances(proxy);

    if balances.is_empty() && balance_errors.is_empty() {
        return Err("No balance provider is configured.".to_owned());
    }

    let service_status = platforms::status::fetch(proxy);

    // Every provider keeps its own history; only DeepSeek has a status page, so
    // the others record the health as unknown.
    for (platform, found) in &balances {
        let status = if platform == storage::KEY_DEEPSEEK {
            service_status.as_str()
        } else {
            "unknown"
        };
        // A failed history write costs us the curve, not the reading: the figure
        // on screen comes from the snapshot, so it should still be shown.
        if let Err(error) = storage::save_balance_history(platform, found, status) {
            let _ = storage::log_line(&format!("history write failed for {platform}: {error}"));
        }
    }
    let _ = storage::prune_balance_history(config.retention_days);
    let _ = storage::prune_subscription_history(config.retention_days);

    let mut consumption_rates = std::collections::BTreeMap::new();
    for platform in balances.keys() {
        if let Ok(Some(rate)) = history::consumption_rate_with_fallback(
            platform,
            config.retention_days,
            config.interval_minutes,
        ) {
            consumption_rates.insert(platform.clone(), rate);
        }
    }

    Ok(Outcome {
        balances,
        balance_errors,
        service_status,
        consumption_rates,
        packages: gather_packages(proxy),
    })
}

/// Reads every configured balance provider, keeping failures beside the
/// successes so one broken key does not hide the others.
fn gather_balances(
    proxy: &str,
) -> (
    std::collections::BTreeMap<String, Balances>,
    std::collections::BTreeMap<String, String>,
) {
    use crate::catalog::{self, Mode};

    let mut balances = std::collections::BTreeMap::new();
    let mut errors = std::collections::BTreeMap::new();

    for meta in catalog::implemented().filter(|meta| meta.mode == Mode::Payg) {
        let Ok(Some(key)) = storage::read_secret(meta.key) else {
            continue;
        };

        let result = match meta.key {
            "deepseek" => platforms::deepseek::fetch_balance(&key, proxy),
            "kimi_token_cn" | "kimi_token_global" => {
                platforms::kimi::fetch_balance(meta.key, &key, proxy)
            }
            "stepfun_token_cn" | "stepfun_token_global" => {
                platforms::stepfun::fetch_balance(meta.key, &key, proxy)
            }
            "openrouter" => platforms::openrouter::fetch_balance(meta.key, &key, proxy),
            _ => continue,
        };

        match result {
            Ok(found) => {
                balances.insert(meta.key.to_owned(), found);
            }
            Err(error) => {
                errors.insert(meta.key.to_owned(), error);
            }
        }
    }

    (balances, errors)
}

fn gather_demo(config: &AppConfig) -> Result<Outcome, String> {
    let conn = storage::open_db()?;
    demo::prepare(&conn)?;
    let balances = demo::balances(&conn)?;
    let consumption_rate = demo::consumption_rate(&conn).ok();

    Ok(Outcome {
        balances: [("deepseek".to_owned(), balances)].into_iter().collect(),
        balance_errors: Default::default(),
        service_status: "none".to_owned(),
        consumption_rates: consumption_rate
            .map(|rate| [("deepseek".to_owned(), rate)].into_iter().collect())
            .unwrap_or_default(),
        packages: gather_packages(platforms::effective_proxy(config)),
    })
}

/// Logs the monthly allowance of every plan that reports one, so the
/// subscription page can chart it.
///
/// A plan that reports money is recorded in money; one that reports a share is
/// recorded out of a hundred.
fn record_subscription_usage(
    packages: &std::collections::BTreeMap<String, Subscription<PackageQuota>>,
) {
    for (platform, subscription) in packages {
        let Subscription::Loaded(quota) = subscription else {
            continue;
        };
        // Every window a plan reports, not just the monthly one: the five-hour
        // window reports money rather than percent, and that money is what
        // refines the coarse weekly and monthly figures (`history::refined`).
        for window in ["monthly", "weekly", "5h"] {
            let Some(entry) = quota.get(window) else {
                continue;
            };
            let (used, cap) = entry.as_recorded_usage();
            if let Err(error) = storage::save_subscription_usage(platform, window, used, cap) {
                let _ = storage::log_line(&format!(
                    "usage history write failed for {platform} {window}: {error}"
                ));
            }
        }
    }
}

/// Refines OpenCode Go's coarse windows with the money its five-hour window
/// reports.
///
/// It is the one plan this can be done for: it reports a five-hour window in
/// money next to whole-percent weekly and monthly ones, and its pools are known
/// ($30 a week, $60 a month from the previous build's experiments). A plan
/// without that five-hour window is left exactly as the endpoint reported it.
fn refine_coarse_windows(
    packages: &mut std::collections::BTreeMap<String, Subscription<PackageQuota>>,
) {
    use crate::storage::PROVIDER_OPENCODE_GO;

    let Ok(spent) = storage::subscription_usage_history(PROVIDER_OPENCODE_GO, "5h", 30) else {
        return;
    };

    let Some(Subscription::Loaded(quota)) = packages.get_mut(PROVIDER_OPENCODE_GO) else {
        return;
    };

    for (window, pool) in [("weekly", 30.0), ("monthly", 60.0)] {
        let Ok(coarse) = storage::subscription_usage_history(PROVIDER_OPENCODE_GO, window, 30)
        else {
            continue;
        };
        let Some(refined) = history::refined_percent(&coarse, &spent, pool) else {
            continue;
        };

        if let Some(entry) = quota.get(window) {
            let reset_in_sec = entry.reset_in_sec;
            quota.insert(
                window.to_owned(),
                crate::model::QuotaWindow::from_percent(refined, reset_in_sec),
            );
        }
    }
}

/// The pace of every window the plans report, read from the logged readings.
///
/// A single poll says what a quota is now; only the readings together say how
/// fast it is going, which is what tells a window that will be spent before it
/// resets from one that will not.
fn package_rates(
    packages: &std::collections::BTreeMap<String, Subscription<PackageQuota>>,
) -> WindowRates {
    /// How far back the pace is read: a monthly window keeps its whole cycle
    /// inside this, and earlier readings describe cycles already reset.
    const HISTORY_DAYS: u64 = 30;

    let mut rates = WindowRates::new();
    for (platform, subscription) in packages {
        let Subscription::Loaded(quota) = subscription else {
            continue;
        };
        let mut per_window = std::collections::BTreeMap::new();
        for (window, entry) in quota {
            let Ok(points) = storage::subscription_usage_history(platform, window, HISTORY_DAYS)
            else {
                continue;
            };
            if let Some(rate) = history::window_rate(&points, entry.reset_in_sec) {
                per_window.insert(window.clone(), rate);
            }
        }
        if !per_window.is_empty() {
            rates.insert(platform.clone(), per_window);
        }
    }
    rates
}

/// Reads every configured package plan, keeping failures beside the successes
/// so one broken key does not hide the others.
fn gather_packages(proxy: &str) -> std::collections::BTreeMap<String, Subscription<PackageQuota>> {
    use crate::catalog::{self, Mode};

    let mut packages = std::collections::BTreeMap::new();
    for meta in catalog::implemented().filter(|meta| meta.mode == Mode::Package) {
        let subscription = match storage::read_secret(meta.key) {
            Ok(Some(key)) => match fetch_package(meta.key, &key, proxy) {
                Ok(quota) => Subscription::Loaded(quota),
                Err(error) => Subscription::Failed(error),
            },
            Ok(None) => Subscription::NotConfigured,
            Err(error) => Subscription::Failed(error),
        };
        packages.insert(meta.key.to_owned(), subscription);
    }
    packages
}

/// Sends one plan's request to its client.
fn fetch_package(platform: &str, key: &str, proxy: &str) -> Result<PackageQuota, String> {
    match platform {
        "opencode_go" => platforms::opencode_go::fetch_quota(key, proxy),
        "command_code" => platforms::command_code::fetch_quota(key, proxy),
        "glm_coding_cn" | "glm_coding_global" => platforms::glm::fetch_quota(platform, key, proxy),
        "minimax_token_cn"
        | "minimax_token_global"
        | "minimax_coding_cn"
        | "minimax_coding_global" => platforms::minimax::fetch_quota(platform, key, proxy),
        other => Err(format!("No quota client for {other}.")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_subscription_starts_unconfigured() {
        assert_eq!(Subscription::<u8>::default(), Subscription::NotConfigured);
        assert_ne!(
            Subscription::Loaded(1u8),
            Subscription::Failed("boom".to_owned())
        );
    }

    #[test]
    fn snapshot_starts_empty() {
        let snapshot = Snapshot::default();
        assert!(snapshot.balances.is_empty());
        assert!(snapshot.packages.is_empty());
        assert!(snapshot.last_check.is_none());
        assert!(!snapshot.checking);
        assert!(!snapshot.demo);
    }

    #[test]
    fn brisk_spending_needs_a_line_and_a_figure() {
        let mut config = AppConfig::default();
        let mut snapshot = Snapshot {
            today_spend: Some(("CNY".to_owned(), 12.0)),
            ..Snapshot::default()
        };

        // It ships off: no line, no alert, whatever the day cost.
        assert!(!snapshot.spending_is_brisk(&config));

        config.brisk_threshold_yuan = 10.0;
        assert!(
            snapshot.spending_is_brisk(&config),
            "12 spent against a line of 10"
        );

        config.brisk_threshold_yuan = 20.0;
        assert!(!snapshot.spending_is_brisk(&config));

        // A day with nothing to compare is never brisk, however low the line.
        snapshot.today_spend = None;
        config.brisk_threshold_yuan = 1.0;
        assert!(!snapshot.spending_is_brisk(&config));
    }
}
