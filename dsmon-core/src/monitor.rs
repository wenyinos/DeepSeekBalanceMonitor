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
use crate::model::{Balances, ConsumptionRate, HistoryRecord, PackageQuota, WindowRates};
use crate::{demo, history, platforms, storage, time, update};

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
    /// Readings per account id, for the balance providers.
    pub balances: std::collections::BTreeMap<String, Balances>,
    /// Why an account's last reading failed, keyed the same way.
    pub balance_errors: std::collections::BTreeMap<String, String>,
    pub service_status: String,
    /// The service indicator per platform that has a status page of its own
    /// (DeepSeek and MiniMax), keyed by the platform key.
    pub platform_status: std::collections::BTreeMap<String, String>,
    /// Burn-rate estimates per account.
    pub consumption_rates: std::collections::BTreeMap<String, ConsumptionRate>,
    /// The package plans' quota windows, keyed by account id.
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
    /// What each balance account has cost today, keyed by account id: the
    /// currency and the amount. An account with nothing to compare — a day
    /// holding one reading, a balance that went up, or before the first poll —
    /// is absent rather than zero.
    pub today_spend: std::collections::BTreeMap<String, (String, f64)>,
    /// The day's largest package-window spend per account: account id to
    /// (window, percentage points spent). Absent when no plan recorded a rise.
    pub package_day_spend: std::collections::BTreeMap<String, (String, f64)>,
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
        self.today_spend.iter().any(|(account, (_, spent))| {
            crate::accounts::platform(config, account) == Some(storage::KEY_DEEPSEEK)
                && *spent >= config.brisk_threshold_yuan
        })
    }

    /// Whether the largest package plan's day has passed the percent line.
    ///
    /// A line of zero means the alert is off, and a day with no recorded
    /// spend is never brisk.
    pub fn package_spending_is_brisk(&self, config: &crate::config::AppConfig) -> bool {
        if config.brisk_package_percent <= 0.0 {
            return false;
        }
        self.package_day_spend
            .values()
            .any(|(_, spent)| *spent >= config.brisk_package_percent)
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
///
/// Everything that touches the database happens before the snapshot is taken:
/// the lock is held only for the assignments. Holding it across SQLite work
/// makes every reader wait on it — the interface's own frame, and the widget's
/// request, which reads history to build its answer — and a busy database is
/// allowed to take seconds before it gives up.
fn poll_once(config: &AppConfig, snapshot: &Arc<Mutex<Snapshot>>, scope: Scope) {
    set_checking(snapshot, true);

    // A subscription pass leaves the balance and the service health as they are.
    if scope == Scope::Subscriptions {
        poll_subscriptions(config, snapshot);
        return;
    }

    let result = gather(config);
    let published = result.map(|mut outcome| {
        // Recorded before anything is refined: the history has to hold what
        // the endpoint actually said, or the next refinement would compare
        // a refined figure against a refined one and drift.
        record_subscription_usage(&outcome.packages, config);
        refine_coarse_windows(&mut outcome.packages, config);
        let window_rates = package_rates(&outcome.packages, config);
        let today_spend = today_spend(config);
        let package_day_spend = package_day_spend(&outcome.packages, config);
        (outcome, window_rates, today_spend, package_day_spend)
    });

    let Ok(mut guard) = snapshot.lock() else {
        return;
    };
    guard.checking = false;
    guard.last_check = Some(Local::now());

    match published {
        Ok((outcome, window_rates, today_spend, package_day_spend)) => {
            guard.balances = outcome.balances;
            guard.balance_errors = outcome.balance_errors;
            guard.service_status = outcome.service_status;
            guard.platform_status = outcome.platform_status;
            guard.consumption_rates = outcome.consumption_rates;
            guard.packages = outcome.packages;
            guard.window_rates = window_rates;
            guard.today_spend = today_spend;
            guard.package_day_spend = package_day_spend;
            guard.last_error = None;
        }
        Err(error) => guard.last_error = Some(error),
    }
}

/// What today has cost the DeepSeek account: the day's positive drops added
/// up, in the currency they share.
///
/// The window is the calendar day, and the aggregation is the earlier
/// build's: a drop counts, a rise (a top-up or a grant) does not — adding
/// only the drops keeps consumption visible after a recharge, where a
/// first-minus-last would cancel it out. A day holding one reading, and a day
/// with no positive drop at all, say nothing rather than a confident zero.
fn today_spend(config: &AppConfig) -> std::collections::BTreeMap<String, (String, f64)> {
    use crate::catalog::Mode;

    let today = format!("{} 00:00:00", Local::now().format("%Y-%m-%d"));
    let mut spends = std::collections::BTreeMap::new();
    for account in crate::accounts::enabled(config) {
        let Some(meta) = crate::catalog::find(&account.platform) else {
            continue;
        };
        if meta.mode != Mode::Payg {
            continue;
        }
        let Ok(records) = storage::history_records_since(&account.id, &today, 2000) else {
            continue;
        };
        if let Some(spend) = spent_of_day(&records) {
            spends.insert(account.id.clone(), spend);
        }
    }
    spends
}

/// The day's positive drops added up, in the currency they share.
fn spent_of_day(records: &[HistoryRecord]) -> Option<(String, f64)> {
    let first = records.first()?;
    let last = records.last()?;
    if first.currency != last.currency {
        return None;
    }

    let mut spent = 0.0;
    for pair in records.windows(2) {
        let drop = pair[0].total - pair[1].total;
        if drop > 0.0 {
            spent += drop;
        }
    }
    (spent > 0.0).then(|| (last.currency.clone(), spent))
}

/// The day's largest package-window spend: the platform and window it
/// belongs to, and the percentage points spent of it.
///
/// Each plan is read through its main window ([`crate::model::main_window`]) —
/// the window the earlier build's day figure used by default.
fn package_day_spend(
    packages: &std::collections::BTreeMap<String, Subscription<PackageQuota>>,
    config: &AppConfig,
) -> std::collections::BTreeMap<String, (String, f64)> {
    let today = format!("{} 00:00:00", Local::now().format("%Y-%m-%d"));
    let mut spends = std::collections::BTreeMap::new();

    for (account_id, subscription) in packages {
        let Subscription::Loaded(quota) = subscription else {
            continue;
        };
        let Some(platform) = crate::accounts::platform(config, account_id) else {
            continue;
        };
        let Some(window) = crate::model::main_window(quota) else {
            continue;
        };
        let Ok(points) = storage::subscription_usage_since(account_id, platform, window, &today)
        else {
            continue;
        };
        let spent = history::window_day_spend(&points);
        if spent > 0.0 {
            spends.insert(account_id.clone(), (window.to_owned(), spent));
        }
    }
    spends
}

/// Refreshes the package plans' quotas alone.
fn poll_subscriptions(config: &AppConfig, snapshot: &Arc<Mutex<Snapshot>>) {
    let packages = gather_packages(platforms::effective_proxy(config), config);
    record_subscription_usage(&packages, config);
    let window_rates = package_rates(&packages, config);

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
    platform_status: std::collections::BTreeMap<String, String>,
    consumption_rates: std::collections::BTreeMap<String, ConsumptionRate>,
    packages: std::collections::BTreeMap<String, Subscription<PackageQuota>>,
}

fn gather(config: &AppConfig) -> Result<Outcome, String> {
    let demo = crate::accounts::enabled(config).any(|account| {
        matches!(storage::read_secret(&account.id), Ok(Some(key)) if demo::is_enabled(&key))
    });
    if demo {
        return gather_demo(config);
    }

    let proxy = platforms::effective_proxy(config);
    let (balances, balance_errors) = gather_balances(proxy, config);

    if balances.is_empty() && balance_errors.is_empty() {
        return Err("No balance provider is configured.".to_owned());
    }

    let service_status = platforms::status::fetch(proxy);

    // DeepSeek and MiniMax are the two providers with a status page of their
    // own; the indicator is kept per platform so each page shows its own.
    let mut platform_status = std::collections::BTreeMap::new();
    platform_status.insert(storage::KEY_DEEPSEEK.to_owned(), service_status.clone());
    if minimax_configured(config) {
        let status = platforms::minimax::fetch_status(proxy);
        for meta in crate::catalog::implemented().filter(|meta| meta.key.starts_with("minimax_")) {
            platform_status.insert(meta.key.to_owned(), status.clone());
        }
    }

    // Every provider keeps its own history; only DeepSeek has a status page, so
    // the others record the health as unknown.
    for (account_id, found) in &balances {
        let platform = crate::accounts::platform(config, account_id).unwrap_or("");
        let status = if platform == storage::KEY_DEEPSEEK {
            service_status.as_str()
        } else {
            "unknown"
        };
        // A failed history write costs us the curve, not the reading: the figure
        // on screen comes from the snapshot, so it should still be shown.
        if let Err(error) = storage::save_balance_history(account_id, platform, found, status) {
            let _ = storage::log_line(&format!("history write failed for {account_id}: {error}"));
        }
    }
    let _ = storage::prune_balance_history(config.retention_days);
    let _ = storage::prune_subscription_history(config.retention_days);

    let mut consumption_rates = std::collections::BTreeMap::new();
    for account_id in balances.keys() {
        if let Ok(Some(rate)) = history::consumption_rate_with_fallback(
            account_id,
            config.retention_days,
            config.interval_minutes,
        ) {
            consumption_rates.insert(account_id.clone(), rate);
        }
    }

    Ok(Outcome {
        balances,
        balance_errors,
        service_status,
        platform_status,
        consumption_rates,
        packages: gather_packages(proxy, config),
    })
}

/// Whether any MiniMax plan holds a key: its status page is only fetched when
/// there is a plan for it to say something about.
fn minimax_configured(config: &AppConfig) -> bool {
    crate::accounts::enabled(config)
        .filter(|account| account.platform.starts_with("minimax_"))
        .any(|account| matches!(storage::read_secret(&account.id), Ok(Some(_))))
}

/// Reads every configured balance provider, keeping failures beside the
/// successes so one broken key does not hide the others.
fn gather_balances(
    proxy: &str,
    config: &AppConfig,
) -> (
    std::collections::BTreeMap<String, Balances>,
    std::collections::BTreeMap<String, String>,
) {
    use crate::catalog::{self, Mode};

    let mut balances = std::collections::BTreeMap::new();
    let mut errors = std::collections::BTreeMap::new();

    for account in crate::accounts::enabled(config) {
        let Some(meta) = catalog::find(&account.platform) else {
            continue;
        };
        if meta.mode != Mode::Payg {
            continue;
        }
        let Ok(Some(key)) = storage::read_secret(&account.id) else {
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
                balances.insert(account.id.clone(), found);
            }
            Err(error) => {
                errors.insert(account.id.clone(), error);
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
    let account_id = crate::accounts::enabled(config)
        .find(|account| {
            matches!(storage::read_secret(&account.id), Ok(Some(key)) if demo::is_enabled(&key))
        })
        .map(|account| account.id.clone())
        .or_else(|| {
            crate::accounts::of_platform(config, storage::KEY_DEEPSEEK)
                .next()
                .map(|account| account.id.clone())
        })
        .unwrap_or_else(|| storage::KEY_DEEPSEEK.to_owned());

    Ok(Outcome {
        balances: [(account_id.clone(), balances)].into_iter().collect(),
        balance_errors: Default::default(),
        service_status: "none".to_owned(),
        platform_status: Default::default(),
        consumption_rates: consumption_rate
            .map(|rate| [(account_id, rate)].into_iter().collect())
            .unwrap_or_default(),
        packages: gather_packages(platforms::effective_proxy(config), config),
    })
}

/// Logs the monthly allowance of every plan that reports one, so the
/// subscription page can chart it.
///
/// A plan that reports money is recorded in money; one that reports a share is
/// recorded out of a hundred.
fn record_subscription_usage(
    packages: &std::collections::BTreeMap<String, Subscription<PackageQuota>>,
    config: &AppConfig,
) {
    for (account_id, subscription) in packages {
        let Some(platform) = crate::accounts::platform(config, account_id) else {
            continue;
        };
        let Subscription::Loaded(quota) = subscription else {
            continue;
        };
        // Every window a plan reports, not just the monthly one: the
        // five-hour window is what refines the coarse weekly and monthly
        // figures ([`history::refined_percent`]), and that join is on the
        // timestamp, so the whole batch shares one.
        let timestamp = time::now();
        for window in ["monthly", "weekly", "5h"] {
            let Some(entry) = quota.get(window) else {
                continue;
            };
            let (used, cap) = entry.as_recorded_usage();
            if let Err(error) = storage::save_subscription_usage_at(
                account_id, platform, window, used, cap, &timestamp,
            ) {
                let _ = storage::log_line(&format!(
                    "usage history write failed for {account_id} {window}: {error}"
                ));
            }
        }
    }
}

/// Refines OpenCode Go's coarse windows with the money its five-hour window
/// reports.
///
/// It is the one plan this can be done for: it reports a five-hour window in
/// percents of a known pool next to whole-percent weekly and monthly windows
/// whose pools are known too — $12 per five hours, $30 a week, $60 a month
/// from the earlier build's measurements. A plan without that five-hour
/// window is left exactly as the endpoint reported it.
fn refine_coarse_windows(
    packages: &mut std::collections::BTreeMap<String, Subscription<PackageQuota>>,
    config: &AppConfig,
) {
    use crate::storage::PROVIDER_OPENCODE_GO;

    /// The warm-up window: the model re-establishes its estimate well before
    /// the latest point it answers for.
    const HISTORY_DAYS: u64 = 90;
    /// The five-hour pool, in dollars, that converts its percent readings
    /// into money.
    const FIVE_HOUR_POOL: f64 = 12.0;

    let accounts: Vec<String> = crate::accounts::enabled(config)
        .filter(|account| account.platform == PROVIDER_OPENCODE_GO)
        .map(|account| account.id.clone())
        .collect();

    for account_id in accounts {
        let Some(Subscription::Loaded(quota)) = packages.get_mut(&account_id) else {
            continue;
        };

        for (window, pool) in [("weekly", 30.0), ("monthly", 60.0)] {
            let Ok(readings) = storage::subscription_usage_pairs(
                &account_id,
                PROVIDER_OPENCODE_GO,
                window,
                HISTORY_DAYS,
            ) else {
                continue;
            };
            let Some(refined) = history::refined_percent(&readings, FIVE_HOUR_POOL, pool) else {
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
}

/// The pace of every window the plans report, read from the logged readings.
///
/// A single poll says what a quota is now; only the readings together say how
/// fast it is going, which is what tells a window that will be spent before it
/// resets from one that will not.
fn package_rates(
    packages: &std::collections::BTreeMap<String, Subscription<PackageQuota>>,
    config: &AppConfig,
) -> WindowRates {
    /// How far back the pace is read: a monthly window keeps its whole cycle
    /// inside this, and earlier readings describe cycles already reset.
    const HISTORY_DAYS: u64 = 30;

    let mut rates = WindowRates::new();
    for (account_id, subscription) in packages {
        let Some(platform) = crate::accounts::platform(config, account_id) else {
            continue;
        };
        let Subscription::Loaded(quota) = subscription else {
            continue;
        };
        let mut per_window = std::collections::BTreeMap::new();
        for (window, entry) in quota {
            let Ok(points) =
                storage::subscription_usage_history(account_id, platform, window, HISTORY_DAYS)
            else {
                continue;
            };
            if let Some(rate) = history::window_rate(&points, entry.reset_in_sec) {
                per_window.insert(window.clone(), rate);
            }
        }
        if !per_window.is_empty() {
            rates.insert(account_id.clone(), per_window);
        }
    }
    rates
}

/// Reads every configured package plan, keeping failures beside the successes
/// so one broken key does not hide the others.
fn gather_packages(
    proxy: &str,
    config: &AppConfig,
) -> std::collections::BTreeMap<String, Subscription<PackageQuota>> {
    use crate::catalog::{self, Mode};

    let mut packages = std::collections::BTreeMap::new();
    for account in crate::accounts::enabled(config) {
        let Some(meta) = catalog::find(&account.platform) else {
            continue;
        };
        if meta.mode != Mode::Package {
            continue;
        }
        let subscription = match storage::read_secret(&account.id) {
            Ok(Some(key)) => match fetch_package(meta.key, &key, proxy) {
                Ok(quota) => Subscription::Loaded(quota),
                Err(error) => Subscription::Failed(error),
            },
            Ok(None) => Subscription::NotConfigured,
            Err(error) => Subscription::Failed(error),
        };
        packages.insert(account.id.clone(), subscription);
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
        config.accounts.push(crate::config::AccountConfig {
            id: storage::KEY_DEEPSEEK.to_owned(),
            platform: storage::KEY_DEEPSEEK.to_owned(),
            name: "DeepSeek".to_owned(),
            enabled: true,
            billing_day: None,
        });
        let mut snapshot = Snapshot::default();
        snapshot
            .today_spend
            .insert("deepseek".to_owned(), ("CNY".to_owned(), 12.0));

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
        snapshot.today_spend.clear();
        config.brisk_threshold_yuan = 1.0;
        assert!(!snapshot.spending_is_brisk(&config));
    }
}
