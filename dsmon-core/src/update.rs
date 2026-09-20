//! The release page, asked whether a newer version exists.
//!
//! The program ships as .deb/.rpm/MSI packages with no update channel of its
//! own, so nothing else can tell a user that the build they are running has
//! been superseded — and a build can be separated from its fixes by a single
//! release, which is exactly how a stale install goes unnoticed for weeks. The
//! check is one request a day and it reports nothing when it fails: a check
//! nobody asked for has no business complaining about the network.

use std::time::Duration;

use serde::Deserialize;

use crate::platforms::http_client;

/// The release page's own API, which answers with the newest published release.
const RELEASES_API: &str =
    "https://api.github.com/repos/wenyinos/DeepSeekBalanceMonitor/releases/latest";

#[derive(Deserialize)]
struct Release {
    tag_name: String,
}

/// The newest published version, without its tag prefix.
///
/// `Ok(None)` is a release page that answered with something this build cannot
/// read as a version; `Err` is a check that could not be made at all.
pub fn latest_version(http_proxy: &str) -> Result<Option<String>, String> {
    let client = http_client(Duration::from_secs(10), http_proxy)?;
    let release: Release = client
        .get(RELEASES_API)
        .header("Accept", "application/vnd.github+json")
        .header("User-Agent", crate::APP_NAME)
        .send()
        .map_err(|error| error.to_string())?
        .error_for_status()
        .map_err(|error| error.to_string())?
        .json()
        .map_err(|error| error.to_string())?;

    Ok(version_of(&release.tag_name))
}

/// A tag read as a version, or nothing when it is not one.
fn version_of(tag: &str) -> Option<String> {
    let version = tag.trim().trim_start_matches('v');
    let looks_like_one = version.starts_with(|first: char| first.is_ascii_digit())
        && version.chars().any(|character| character == '.');
    looks_like_one.then(|| version.to_owned())
}

/// Whether `latest` is a later version than `current`.
///
/// Compared part by part as numbers, so `2.1.10` is newer than `2.1.9` — a
/// string comparison gets that backwards. A part that is not a number ends the
/// comparison, which is what a pre-release suffix should do.
pub fn is_newer(latest: &str, current: &str) -> bool {
    let parts = |text: &str| -> Vec<u64> {
        text.split(['.', '-', '+'])
            .map_while(|part| part.parse::<u64>().ok())
            .collect()
    };

    let (latest, current) = (parts(latest), parts(current));
    if latest.is_empty() {
        return false;
    }

    for index in 0..latest.len().max(current.len()) {
        let left = latest.get(index).copied().unwrap_or(0);
        let right = current.get(index).copied().unwrap_or(0);
        if left != right {
            return left > right;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_tag_as_a_version() {
        assert_eq!(version_of("v2.1.3"), Some("2.1.3".to_owned()));
        assert_eq!(version_of("2.1.3"), Some("2.1.3".to_owned()));
        assert_eq!(version_of(" nightly "), None);
        assert_eq!(version_of(""), None);
    }

    #[test]
    fn a_later_version_is_newer() {
        assert!(is_newer("2.1.3", "2.1.2"));
        assert!(is_newer("2.2.0", "2.1.9"));
        // Two digits are two digits: this is the comparison a string gets wrong.
        assert!(is_newer("2.1.10", "2.1.9"));
        assert!(is_newer("3", "2.9.9"));
    }

    #[test]
    fn the_same_or_an_older_version_is_not() {
        assert!(!is_newer("2.1.3", "2.1.3"));
        assert!(!is_newer("2.1.2", "2.1.3"));
        assert!(!is_newer("1.9.9", "2.0.0"));
        // A pre-release of a version is not that version.
        assert!(!is_newer("2.1.3-rc1", "2.1.3"));
        assert!(!is_newer("", "2.1.3"));
    }
}
