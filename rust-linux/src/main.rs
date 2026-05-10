use chrono::{DateTime, Duration as ChronoDuration, Local, NaiveDateTime};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process;
use std::thread;
use std::time::Duration;

const APP_DIR: &str = "deepseek-balance-monitor";

#[derive(Clone, Serialize, Deserialize)]
struct AppConfig {
    #[serde(default)]
    api_key: String,
    #[serde(default = "default_interval")]
    interval_minutes: u64,
    #[serde(default = "default_threshold")]
    threshold_yuan: f64,
    #[serde(default = "default_lang")]
    language: String,
    #[serde(default)]
    auto_start: bool,
    #[serde(default = "default_alert_mode")]
    alert_mode: String,
    #[serde(default = "default_api_alert_enabled")]
    api_alert_enabled: bool,
    #[serde(default = "default_retention_days")]
    retention_days: u64,
    #[serde(flatten)]
    extra: BTreeMap<String, serde_json::Value>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            interval_minutes: default_interval(),
            threshold_yuan: default_threshold(),
            language: default_lang(),
            auto_start: false,
            alert_mode: default_alert_mode(),
            api_alert_enabled: default_api_alert_enabled(),
            retention_days: default_retention_days(),
            extra: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Serialize)]
struct Balance {
    total_balance: f64,
    granted_balance: f64,
    topped_up_balance: f64,
}

#[derive(Serialize)]
struct WidgetStatus {
    ok: bool,
    configured: bool,
    error: Option<String>,
    config_path: String,
    interval_minutes: u64,
    threshold_yuan: f64,
    retention_days: u64,
    language: String,
    last_check: String,
    total_currency: Option<String>,
    total_balance: Option<f64>,
    low_balance: bool,
    balances: BTreeMap<String, Balance>,
}

#[derive(Deserialize)]
struct ApiResponse {
    #[serde(default)]
    balance_infos: Vec<ApiBalanceInfo>,
}

#[derive(Deserialize)]
struct ApiBalanceInfo {
    #[serde(default = "default_currency")]
    currency: String,
    #[serde(default)]
    total_balance: String,
    #[serde(default)]
    granted_balance: String,
    #[serde(default)]
    topped_up_balance: String,
}

fn main() {
    process::exit(match run() {
        Ok(()) => 0,
        Err((code, message)) => {
            if !message.is_empty() {
                eprintln!("{message}");
            }
            code
        }
    });
}

fn run() -> Result<(), (i32, String)> {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str).unwrap_or("check") {
        "check" => check_once(),
        "daemon" => run_daemon(),
        "init-config" => init_config(),
        "config-path" => config_file().map_err(fail).and_then(print_path),
        "log-path" => log_file().map_err(fail).and_then(print_path),
        "clean-logs" => clean_logs(),
        "widget-status" => print_widget_status(),
        "config-json" => print_config_json(),
        "set-config" => set_config(&args[2..]),
        "-V" | "--version" => {
            println!("dsmon {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        "-h" | "--help" => {
            print_help();
            Ok(())
        }
        other => Err((1, format!("Unknown command: {other}\nRun: dsmon --help"))),
    }
}

fn check_once() -> Result<(), (i32, String)> {
    let config = load_config().map_err(fail)?;
    prune_logs_on_startup(&config).map_err(fail)?;
    let checked_at = Local::now();
    let key = config.api_key.trim();
    if key.is_empty() {
        ensure_config_file().map_err(fail)?;
        print_status(None, Some("DeepSeek API key is not configured."), checked_at);
        return Err((2, String::new()));
    }
    let api_key = key.chars().filter(|c| c.is_ascii()).collect::<String>();
    match fetch_balance(&api_key) {
        Ok(balances) => {
            print_status(Some(&balances), None, checked_at);
            log_line("Balance check succeeded").map_err(fail)?;
            Ok(())
        }
        Err(error) => {
            print_status(None, Some(&error), checked_at);
            log_line(&format!("Balance check failed: {error}")).ok();
            Err((1, String::new()))
        }
    }
}

fn run_daemon() -> Result<(), (i32, String)> {
    let config = load_config().map_err(fail)?;
    prune_logs_on_startup(&config).map_err(fail)?;
    let api_key = require_api_key(&config)?;
    let interval = Duration::from_secs(config.interval_minutes.clamp(1, 1440) * 60);
    let mut low_balance_reported = false;
    log_line("dsmon daemon started").map_err(fail)?;
    loop {
        match fetch_balance(&api_key) {
            Ok(balances) => {
                log_line(&format!("Balance check succeeded: {}", summary(&balances))).ok();
                let low_balance = is_low_balance(&balances, config.threshold_yuan);
                if low_balance {
                    log_line("Balance is below configured threshold").ok();
                    if should_low_balance_alert(&config, &mut low_balance_reported) {
                        eprintln!("{}", low_balance_message(&balances, config.threshold_yuan));
                    }
                } else {
                    low_balance_reported = false;
                }
            }
            Err(error) => {
                log_line(&format!("Balance check failed: {error}")).ok();
            }
        }
        thread::sleep(interval);
    }
}

fn init_config() -> Result<(), (i32, String)> {
    let path = config_file().map_err(fail)?;
    if !path.exists() {
        save_config(&AppConfig::default()).map_err(fail)?;
    }
    println!("Config file: {}", path.display());
    Ok(())
}

fn clean_logs() -> Result<(), (i32, String)> {
    let path = log_file().map_err(fail)?;
    match fs::remove_file(&path) {
        Ok(()) => println!("Removed log file: {}", path.display()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            println!("No log file found: {}", path.display())
        }
        Err(error) => return Err(fail(error)),
    }
    Ok(())
}

fn print_help() {
    println!(
        "Usage: dsmon [check|daemon|init-config|config-path|log-path|clean-logs|widget-status|config-json|set-config]"
    );
}

fn print_path(path: PathBuf) -> Result<(), (i32, String)> {
    println!("{}", path.display());
    Ok(())
}

fn require_api_key(config: &AppConfig) -> Result<String, (i32, String)> {
    let key = config.api_key.trim();
    if key.is_empty() {
        let path = config_file().map_err(fail)?;
        ensure_config_file().map_err(fail)?;
        return Err((
            2,
            format!(
                "DeepSeek API key is not configured.\nEdit config file: {}\nSet api_key to your DeepSeek API key.",
                path.display()
            ),
        ));
    }
    Ok(key.chars().filter(|c| c.is_ascii()).collect())
}

fn print_widget_status() -> Result<(), (i32, String)> {
    let config = load_config().map_err(fail)?;
    let config_path = config_file()
        .map(|path| path.display().to_string())
        .map_err(fail)?;
    let checked_at = Local::now();
    let key = config.api_key.trim();
    if key.is_empty() {
        ensure_config_file().map_err(fail)?;
        return write_widget_status(WidgetStatus {
            ok: false,
            configured: false,
            error: Some("DeepSeek API key is not configured.".to_string()),
            config_path,
            interval_minutes: config.interval_minutes,
            threshold_yuan: config.threshold_yuan,
            retention_days: config.retention_days,
            language: config.language.clone(),
            last_check: format_time(checked_at),
            total_currency: None,
            total_balance: None,
            low_balance: false,
            balances: BTreeMap::new(),
        });
    }
    let api_key = key.chars().filter(|c| c.is_ascii()).collect::<String>();
    match fetch_balance(&api_key) {
        Ok(balances) => {
            let (total_currency, total_balance) = preferred_balance(&balances)
                .map(|(currency, balance)| (Some(currency.clone()), Some(balance.total_balance)))
                .unwrap_or((None, None));
            write_widget_status(WidgetStatus {
                ok: true,
                configured: true,
                error: None,
                config_path,
                interval_minutes: config.interval_minutes,
                threshold_yuan: config.threshold_yuan,
                retention_days: config.retention_days,
                language: config.language.clone(),
                last_check: format_time(checked_at),
                total_currency,
                total_balance,
                low_balance: is_low_balance(&balances, config.threshold_yuan),
                balances,
            })
        }
        Err(error) => write_widget_status(WidgetStatus {
            ok: false,
            configured: true,
            error: Some(error),
            config_path,
            interval_minutes: config.interval_minutes,
            threshold_yuan: config.threshold_yuan,
            retention_days: config.retention_days,
            language: config.language,
            last_check: format_time(checked_at),
            total_currency: None,
            total_balance: None,
            low_balance: false,
            balances: BTreeMap::new(),
        }),
    }
}

fn write_widget_status(status: WidgetStatus) -> Result<(), (i32, String)> {
    let text = serde_json::to_string(&status).map_err(fail)?;
    println!("{text}");
    Ok(())
}

fn print_config_json() -> Result<(), (i32, String)> {
    let config = load_config().map_err(fail)?;
    println!("{}", serde_json::to_string(&config).map_err(fail)?);
    Ok(())
}

fn set_config(args: &[String]) -> Result<(), (i32, String)> {
    if args.len() != 7 {
        return Err(fail(
            "Usage: dsmon set-config <api_key> <interval_minutes> <threshold_yuan> <language> <auto_start> <alert_mode> <retention_days>",
        ));
    }
    let api_key = args[0].trim().to_string();
    if api_key.is_empty() {
        return Err((2, "DeepSeek API key is required.".to_string()));
    }
    let threshold_yuan = args[2].parse::<f64>().map_err(fail)?;
    if !(0.0..=10000.0).contains(&threshold_yuan) {
        return Err(fail("Balance threshold must be between 0 and 10000."));
    }
    let mut config = load_config().unwrap_or_default();
    config.api_key = api_key;
    config.interval_minutes = args[1].parse::<u64>().map_err(fail)?.clamp(1, 1440);
    config.threshold_yuan = threshold_yuan;
    config.language = if args[3] == "zh" { "zh" } else { "en" }.to_string();
    config.auto_start = parse_bool_arg(&args[4]).map_err(fail)?;
    config.alert_mode = parse_alert_mode_arg(&args[5]).map_err(fail)?;
    config.retention_days = args[6].parse::<u64>().map_err(fail)?.clamp(1, 3650);
    normalize_config(&mut config);
    save_config(&config).map_err(fail)?;
    set_auto_start(config.auto_start).map_err(fail)?;
    println!("Config saved.");
    Ok(())
}

fn fetch_balance(api_key: &str) -> Result<BTreeMap<String, Balance>, String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| e.to_string())?;
    let response = client
        .get("https://api.deepseek.com/user/balance")
        .header("Accept", "application/json")
        .bearer_auth(api_key)
        .send()
        .map_err(|e| e.to_string())?;
    if response.status() == StatusCode::UNAUTHORIZED {
        return Err("Invalid API key (401 Unauthorized)".to_string());
    }
    let payload: ApiResponse = response
        .error_for_status()
        .map_err(|e| e.to_string())?
        .json()
        .map_err(|e| e.to_string())?;
    if payload.balance_infos.is_empty() {
        return Err("No balance information in response".to_string());
    }
    let mut balances = BTreeMap::new();
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

fn print_status(
    balances: Option<&BTreeMap<String, Balance>>,
    error: Option<&str>,
    checked_at: DateTime<Local>,
) {
    println!("DeepSeek Balance:");
    if let Some((currency, balance)) = balances.and_then(|items| preferred_balance(items)) {
        println!(
            "{} {} (Topped {}, Granted {})",
            format_amount(balance.total_balance),
            currency,
            format_amount(balance.topped_up_balance),
            format_amount(balance.granted_balance)
        );
        println!("Last Check: {}", format_time(checked_at));
    } else if let Some(error) = error {
        println!("Query error: {error}");
    } else {
        println!("Not checked");
    }
    println!("DeepSeek API Status: 🟢 All Systems Operational");
}

fn summary(balances: &BTreeMap<String, Balance>) -> String {
    balances
        .iter()
        .map(|(currency, balance)| {
            format!("{currency} total={}", format_amount(balance.total_balance))
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn preferred_balance(balances: &BTreeMap<String, Balance>) -> Option<(&String, &Balance)> {
    balances.iter().next()
}

fn is_low_balance(balances: &BTreeMap<String, Balance>, threshold: f64) -> bool {
    preferred_balance(balances)
        .map(|(_, balance)| balance.total_balance < threshold)
        .unwrap_or(false)
}

fn low_balance_message(balances: &BTreeMap<String, Balance>, threshold: f64) -> String {
    if let Some((currency, balance)) = preferred_balance(balances) {
        format!(
            "⚠ DeepSeek Low Balance\nBalance is only {} {}, below your alert threshold of {} {}.\nPlease top up!",
            format_amount(balance.total_balance),
            currency,
            format_amount(threshold),
            currency
        )
    } else {
        "⚠ DeepSeek Low Balance\nNo balance information is available.".to_string()
    }
}

fn parse_bool_arg(value: &str) -> Result<bool, String> {
    match value {
        "true" | "1" | "yes" | "on" => Ok(true),
        "false" | "0" | "no" | "off" => Ok(false),
        _ => Err(format!("Invalid boolean value: {value}")),
    }
}

fn parse_alert_mode_arg(value: &str) -> Result<String, String> {
    match value {
        "never" | "always" | "once" => Ok(value.to_string()),
        _ => Ok((if parse_bool_arg(value)? { "once" } else { "never" }).to_string()),
    }
}

fn should_low_balance_alert(config: &AppConfig, reported: &mut bool) -> bool {
    match config.alert_mode.as_str() {
        "never" => false,
        "always" => true,
        _ if *reported => false,
        _ => {
            *reported = true;
            true
        }
    }
}

fn set_auto_start(enable: bool) -> Result<(), String> {
    let action = if enable { "enable" } else { "disable" };
    let output = std::process::Command::new("systemctl")
        .args(["--user", action, "--now", "dsmon.service"])
        .output()
        .map_err(|e| e.to_string())?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

fn load_config() -> Result<AppConfig, String> {
    let path = config_file().map_err(|e| e.to_string())?;
    if !path.exists() {
        save_config(&AppConfig::default()).map_err(|e| e.to_string())?;
    }
    let text = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let mut config = serde_json::from_str::<AppConfig>(&text).map_err(|e| e.to_string())?;
    normalize_config(&mut config);
    Ok(config)
}

fn normalize_config(config: &mut AppConfig) {
    if config.alert_mode == default_alert_mode() {
        if let Some(value) = config.extra.remove("enable_alerts") {
            config.alert_mode = if value.as_bool() == Some(false) {
                "never".to_string()
            } else {
                "once".to_string()
            };
        }
    } else {
        config.extra.remove("enable_alerts");
    }
    if let Some(value) = config.extra.remove("log_retention_days") {
        if let Some(days) = value.as_u64() {
            config.retention_days = days;
        }
    }
    config.interval_minutes = config.interval_minutes.clamp(1, 1440);
    config.threshold_yuan = config.threshold_yuan.clamp(0.0, 10000.0);
    config.retention_days = config.retention_days.clamp(1, 3650);
    if config.language != "zh" && config.language != "en" {
        config.language = default_lang();
    }
    if !matches!(config.alert_mode.as_str(), "never" | "always" | "once") {
        config.alert_mode = default_alert_mode();
    }
}

fn save_config(config: &AppConfig) -> std::io::Result<()> {
    ensure_dir(&config_dir()?)?;
    let file = File::create(config_file()?)?;
    serde_json::to_writer_pretty(file, config)?;
    Ok(())
}

fn ensure_config_file() -> std::io::Result<()> {
    if !config_file()?.exists() {
        save_config(&AppConfig::default())?;
    }
    Ok(())
}

fn log_line(message: &str) -> std::io::Result<()> {
    ensure_dir(&state_dir()?)?;
    let path = log_file()?;
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(file, "[{}] {}", format_time(Local::now()), message)
}

fn prune_logs_on_startup(config: &AppConfig) -> std::io::Result<()> {
    ensure_dir(&state_dir()?)?;
    prune_log_file(&log_file()?, config.retention_days)
}

fn prune_log_file(path: &Path, retention_days: u64) -> std::io::Result<()> {
    let content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    let cutoff = Local::now().naive_local() - ChronoDuration::days(retention_days as i64);
    let mut changed = false;
    let mut retained = String::new();
    for line in content.lines() {
        if keep_log_line(line, cutoff) {
            retained.push_str(line);
            retained.push('\n');
        } else {
            changed = true;
        }
    }
    if changed {
        fs::write(path, retained)?;
    }
    Ok(())
}

fn keep_log_line(line: &str, cutoff: NaiveDateTime) -> bool {
    let Some(timestamp) = line.strip_prefix('[').and_then(|rest| rest.get(..19)) else {
        return true;
    };
    NaiveDateTime::parse_from_str(timestamp, "%Y-%m-%d %H:%M:%S")
        .map(|logged_at| logged_at >= cutoff)
        .unwrap_or(true)
}

fn config_dir() -> std::io::Result<PathBuf> {
    Ok(std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or(home_dir()?.join(".config"))
        .join(APP_DIR))
}

fn state_dir() -> std::io::Result<PathBuf> {
    Ok(std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .unwrap_or(home_dir()?.join(".local").join("state"))
        .join(APP_DIR))
}

fn config_file() -> std::io::Result<PathBuf> {
    Ok(config_dir()?.join("config.json"))
}

fn log_file() -> std::io::Result<PathBuf> {
    Ok(state_dir()?.join("app.log"))
}

fn home_dir() -> std::io::Result<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "HOME is not set"))
}

fn ensure_dir(path: &Path) -> std::io::Result<()> {
    fs::create_dir_all(path)
}

fn parse_amount(value: &str) -> f64 {
    value.parse::<f64>().unwrap_or(0.0)
}

fn format_amount(value: f64) -> String {
    format!("{value:.2}")
}

fn format_time(value: DateTime<Local>) -> String {
    value.format("%Y-%m-%d %H:%M:%S").to_string()
}

fn default_interval() -> u64 {
    10
}

fn default_threshold() -> f64 {
    1.0
}

fn default_lang() -> String {
    "zh".to_string()
}

fn default_api_alert_enabled() -> bool {
    true
}

fn default_alert_mode() -> String {
    "once".to_string()
}

fn default_retention_days() -> u64 {
    30
}

fn default_currency() -> String {
    "CNY".to_string()
}

fn fail(error: impl ToString) -> (i32, String) {
    (1, error.to_string())
}
