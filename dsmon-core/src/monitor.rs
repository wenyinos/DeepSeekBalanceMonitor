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
use crate::model::{Balances, CommandCodeQuota, ConsumptionRate, OpenCodeGoQuota};
use crate::{demo, history, platforms, storage};

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
    pub balances: Balances,
    pub service_status: String,
    pub consumption_rate: Option<ConsumptionRate>,
    pub opencode_go: Subscription<OpenCodeGoQuota>,
    pub command_code: Subscription<CommandCodeQuota>,
    pub last_check: Option<DateTime<Local>>,
    /// Set when the last poll failed; cleared by the next success.
    pub last_error: Option<String>,
    /// True while a poll is in flight.
    pub checking: bool,
    /// Whether the configured key selects the demo data set.
    pub demo: bool,
}

/// Commands the interface sends to the polling thread.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Refresh,
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

    /// Asks the thread to poll immediately.
    pub fn refresh(&self) {
        let _ = self.commands.send(Command::Refresh);
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

/// The polling loop: sleeps until the next interval, or until told to refresh.
fn run(config: AppConfig, receiver: Receiver<Command>, snapshot: Arc<Mutex<Snapshot>>) {
    let mut config = config;
    loop {
        poll_once(&config, &snapshot);

        let interval = Duration::from_secs(config.interval_minutes.max(1) * 60);
        match receiver.recv_timeout(interval) {
            Ok(Command::Refresh) => {
                // Pick up configuration changes made in the settings page.
                config = AppConfig::load();
            }
            Ok(Command::Stop) | Err(RecvTimeoutError::Disconnected) => return,
            Err(RecvTimeoutError::Timeout) => {
                config = AppConfig::load();
            }
        }
    }
}

/// Performs one poll and publishes the result.
fn poll_once(config: &AppConfig, snapshot: &Arc<Mutex<Snapshot>>) {
    set_checking(snapshot, true);

    let result = gather(config);
    let mut guard = match snapshot.lock() {
        Ok(guard) => guard,
        Err(_) => return,
    };
    guard.checking = false;

    match result {
        Ok(outcome) => {
            guard.balances = outcome.balances;
            guard.service_status = outcome.service_status;
            guard.consumption_rate = outcome.consumption_rate;
            guard.opencode_go = outcome.opencode_go;
            guard.command_code = outcome.command_code;
            guard.last_error = None;
            guard.last_check = Some(Local::now());
        }
        Err(error) => {
            guard.last_error = Some(error);
            guard.last_check = Some(Local::now());
        }
    }
}

fn set_checking(snapshot: &Arc<Mutex<Snapshot>>, value: bool) {
    if let Ok(mut guard) = snapshot.lock() {
        guard.checking = value;
    }
}

/// What one successful poll produced.
struct Outcome {
    balances: Balances,
    service_status: String,
    consumption_rate: Option<ConsumptionRate>,
    opencode_go: Subscription<OpenCodeGoQuota>,
    command_code: Subscription<CommandCodeQuota>,
}

fn gather(config: &AppConfig) -> Result<Outcome, String> {
    let api_key = storage::read_secret(storage::KEY_DEEPSEEK)
        .ok()
        .flatten()
        .unwrap_or_default();

    if demo::is_enabled(&api_key) {
        return gather_demo(config);
    }
    if api_key.is_empty() {
        return Err("API key is not configured.".to_owned());
    }

    let proxy = platforms::effective_proxy(config);
    let balances = platforms::deepseek::fetch_balance(&api_key, proxy)?;
    let service_status = platforms::status::fetch(proxy);
    storage::save_balance_history(&balances, &service_status)?;
    let _ = storage::prune_balance_history(config.retention_days);

    let consumption_rate =
        history::consumption_rate_with_fallback(config.retention_days, config.interval_minutes)
            .ok()
            .flatten();

    Ok(Outcome {
        balances,
        service_status,
        consumption_rate,
        opencode_go: fetch_opencode_go(proxy),
        command_code: fetch_command_code(proxy),
    })
}

fn gather_demo(config: &AppConfig) -> Result<Outcome, String> {
    let conn = storage::open_db()?;
    demo::prepare(&conn)?;
    let balances = demo::balances(&conn)?;
    let consumption_rate = demo::consumption_rate(&conn).ok();

    Ok(Outcome {
        balances,
        service_status: "none".to_owned(),
        consumption_rate,
        opencode_go: fetch_opencode_go(platforms::effective_proxy(config)),
        command_code: fetch_command_code(platforms::effective_proxy(config)),
    })
}

fn fetch_opencode_go(proxy: &str) -> Subscription<OpenCodeGoQuota> {
    let key = match storage::read_secret(storage::KEY_OPENCODE_GO) {
        Ok(Some(key)) => key,
        Ok(None) => return Subscription::NotConfigured,
        Err(error) => return Subscription::Failed(error),
    };

    match platforms::opencode_go::fetch_quota(&key, proxy) {
        Ok(quota) => Subscription::Loaded(quota),
        Err(error) => Subscription::Failed(error),
    }
}

fn fetch_command_code(proxy: &str) -> Subscription<CommandCodeQuota> {
    let key = match storage::read_secret(storage::KEY_COMMAND_CODE) {
        Ok(Some(key)) => key,
        Ok(None) => return Subscription::NotConfigured,
        Err(error) => return Subscription::Failed(error),
    };

    match platforms::command_code::fetch_quota(&key, proxy) {
        Ok(quota) => Subscription::Loaded(quota),
        Err(error) => Subscription::Failed(error),
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
        assert!(snapshot.last_check.is_none());
        assert!(!snapshot.checking);
        assert!(!snapshot.demo);
    }
}
