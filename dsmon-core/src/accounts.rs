//! The API keys the application polls, one entry per account.
//!
//! The list lives in `config.json`; the secret itself stays encrypted in
//! `secure_settings`, under the account's id. An account created from a build
//! that had one key per platform keeps the platform key as its id, so its
//! stored secret and its history need no rewriting.

use crate::catalog;
use crate::config::{AccountConfig, AppConfig};
use crate::storage;

/// Makes the account list agree with the stored keys.
///
/// A configuration written before accounts existed has no list at all; the
/// platforms that hold a secret become one account each, under the platform key
/// that secret already uses. Entries that name an unknown platform, or repeat
/// an id, are dropped. Returns whether the configuration changed.
pub fn ensure(config: &mut AppConfig) -> bool {
    ensure_with(config, |key| {
        matches!(storage::read_secret(key), Ok(Some(_)))
    })
}

/// The part of [`ensure`] that does not touch the database, so its rules can be
/// tested without sharing one.
fn ensure_with(config: &mut AppConfig, holds: impl Fn(&str) -> bool) -> bool {
    let before = config.accounts.len();
    config.accounts.retain(|account| {
        !account.id.trim().is_empty() && catalog::find(&account.platform).is_some()
    });
    let mut changed = config.accounts.len() != before;

    if config.accounts.is_empty() {
        for meta in catalog::implemented() {
            if holds(meta.key) {
                config.accounts.push(AccountConfig {
                    id: meta.key.to_owned(),
                    platform: meta.key.to_owned(),
                    name: meta.display_name.to_owned(),
                    enabled: true,
                    billing_day: None,
                });
                changed = true;
            }
        }
    }

    // An id names a secret, so two entries sharing one would quietly show the
    // same account twice. The later one is dropped.
    let mut seen = std::collections::BTreeSet::new();
    let before = config.accounts.len();
    config
        .accounts
        .retain(|account| seen.insert(account.id.clone()));
    changed |= config.accounts.len() != before;

    for account in &mut config.accounts {
        if account.name.trim().is_empty() {
            account.name = display_name(&account.platform).to_owned();
            changed = true;
        }
    }

    changed
}

/// The accounts that should be polled, in configuration order.
pub fn enabled(config: &AppConfig) -> impl Iterator<Item = &AccountConfig> {
    config.accounts.iter().filter(|account| account.enabled)
}

/// The accounts of one platform, in configuration order.
pub fn of_platform<'a>(
    config: &'a AppConfig,
    platform: &'a str,
) -> impl Iterator<Item = &'a AccountConfig> + 'a {
    config
        .accounts
        .iter()
        .filter(move |account| account.platform == platform)
}

pub fn find<'a>(config: &'a AppConfig, id: &str) -> Option<&'a AccountConfig> {
    config.accounts.iter().find(|account| account.id == id)
}

/// The account's name, falling back to the platform's display name.
pub fn label<'a>(config: &'a AppConfig, id: &str) -> String {
    find(config, id)
        .map(|account| {
            if account.name.trim().is_empty() {
                display_name(&account.platform).to_owned()
            } else {
                account.name.clone()
            }
        })
        .unwrap_or_else(|| id.to_owned())
}

/// The platform an account belongs to.
pub fn platform<'a>(config: &'a AppConfig, id: &str) -> Option<&'a str> {
    find(config, id).map(|account| account.platform.as_str())
}

/// The name a new account of this platform should start with.
pub fn next_name(config: &AppConfig, platform: &str) -> String {
    let base = display_name(platform);
    let names: std::collections::BTreeSet<&str> = config
        .accounts
        .iter()
        .map(|account| account.name.as_str())
        .collect();
    if !config
        .accounts
        .iter()
        .any(|account| account.platform == platform)
    {
        return base.to_owned();
    }
    for number in 2.. {
        let candidate = format!("{base}-{number}");
        if !names.contains(candidate.as_str()) {
            return candidate;
        }
    }
    unreachable!("the loop has no bound that can be reached")
}

/// A fresh id that names no account and no platform.
pub fn new_id(config: &AppConfig) -> String {
    loop {
        let id = random_id();
        if find(config, &id).is_none() && catalog::find(&id).is_none() {
            return id;
        }
    }
}

fn random_id() -> String {
    use ring::rand::{SecureRandom, SystemRandom};

    let mut bytes = [0u8; 4];
    let _ = SystemRandom::new().fill(&mut bytes);
    if bytes == [0; 4] {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.subsec_nanos())
            .unwrap_or(1);
        bytes = nanos.to_le_bytes();
    }
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn display_name(platform: &str) -> &'static str {
    catalog::find(platform)
        .map(|meta| meta.display_name)
        .unwrap_or("Account")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn account(id: &str, platform: &str) -> AccountConfig {
        AccountConfig {
            id: id.to_owned(),
            platform: platform.to_owned(),
            name: String::new(),
            enabled: true,
            billing_day: None,
        }
    }

    #[test]
    fn a_fresh_id_is_not_a_platform_and_is_not_in_use() {
        let config = AppConfig {
            accounts: vec![account("deepseek", "deepseek")],
            ..Default::default()
        };
        let id = new_id(&config);
        assert_eq!(id.len(), 8);
        assert!(id.chars().all(|c| c.is_ascii_hexdigit()), "{id}");
        assert_ne!(id, "deepseek");
        assert!(find(&config, &id).is_none());
    }

    #[test]
    fn names_grow_with_each_account_of_the_same_platform() {
        let mut config = AppConfig::default();
        assert_eq!(next_name(&config, "deepseek"), "DeepSeek");

        config.accounts.push(account("deepseek", "deepseek"));
        config.accounts[0].name = "DeepSeek".to_owned();
        assert_eq!(next_name(&config, "deepseek"), "DeepSeek-2");

        config.accounts.push(account("second", "deepseek"));
        config.accounts[1].name = "DeepSeek-2".to_owned();
        assert_eq!(next_name(&config, "deepseek"), "DeepSeek-3");
    }

    #[test]
    fn stored_platform_keys_become_one_account_each() {
        let mut config = AppConfig::default();
        assert!(ensure_with(&mut config, |key| key == "deepseek"));
        assert_eq!(config.accounts.len(), 1);
        assert_eq!(config.accounts[0].id, "deepseek");
        assert_eq!(config.accounts[0].platform, "deepseek");
        assert_eq!(config.accounts[0].name, "DeepSeek");

        // A second run, with the list now filled, is a no-op.
        assert!(!ensure_with(&mut config, |key| key == "deepseek"));
    }

    #[test]
    fn a_repeated_id_is_dropped() {
        let mut config = AppConfig {
            accounts: vec![account("same", "deepseek"), account("same", "deepseek")],
            ..Default::default()
        };
        assert!(ensure(&mut config));
        assert_eq!(config.accounts.len(), 1);
        assert_eq!(config.accounts[0].name, "DeepSeek");
    }
}
