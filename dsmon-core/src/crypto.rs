//! Secret storage: AES-256-GCM with a key file kept next to the data.
//!
//! The cipher is compiled into the binary rather than delegated to a platform
//! keystore, so one implementation serves both platforms and the ciphertext is
//! portable between them.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

use ring::aead::{Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM};
use ring::rand::{SecureRandom, SystemRandom};

use crate::paths;

/// Marks the blob layout so a future change can be told apart.
const PREFIX: &[u8] = b"DSBM1";
/// Binds the ciphertext to its purpose.
const AAD: &[u8] = b"deepseek-balance-monitor secure_settings api_key v1";
const NONCE_LEN: usize = 12;
const KEY_LEN: usize = 32;

/// Encrypts `plaintext`, returning `PREFIX || nonce || ciphertext || tag`.
pub fn encrypt(plaintext: &str) -> Result<Vec<u8>, String> {
    encrypt_with(&load_or_create_key()?, plaintext)
}

/// Encrypts with a key that is already in hand.
///
/// The tests use this one. What they exercise is the cipher, and going through
/// the key file would have each of them creating that file — the same one, at
/// the same time as the others — which is a filesystem race on Windows and
/// nothing to do with the cipher at all.
fn encrypt_with(key_bytes: &[u8; KEY_LEN], plaintext: &str) -> Result<Vec<u8>, String> {
    let key = LessSafeKey::new(
        UnboundKey::new(&AES_256_GCM, key_bytes).map_err(|_| "invalid secure key".to_string())?,
    );

    let mut nonce_bytes = [0u8; NONCE_LEN];
    SystemRandom::new()
        .fill(&mut nonce_bytes)
        .map_err(|_| "failed to generate a nonce".to_string())?;
    let nonce = Nonce::assume_unique_for_key(nonce_bytes);

    let mut payload = plaintext.as_bytes().to_vec();
    key.seal_in_place_append_tag(nonce, Aad::from(AAD), &mut payload)
        .map_err(|_| "failed to encrypt".to_string())?;

    let mut blob = Vec::with_capacity(PREFIX.len() + NONCE_LEN + payload.len());
    blob.extend_from_slice(PREFIX);
    blob.extend_from_slice(&nonce_bytes);
    blob.extend_from_slice(&payload);
    Ok(blob)
}

/// Reverses [`encrypt`]. Fails on a truncated blob, a wrong prefix or a
/// tampered payload.
pub fn decrypt(blob: &[u8]) -> Result<String, String> {
    decrypt_with(&load_or_create_key()?, blob)
}

/// Reverses [`encrypt_with`].
fn decrypt_with(key_bytes: &[u8; KEY_LEN], blob: &[u8]) -> Result<String, String> {
    if blob.len() <= PREFIX.len() + NONCE_LEN || !blob.starts_with(PREFIX) {
        return Err("invalid encrypted value".to_string());
    }

    let key = LessSafeKey::new(
        UnboundKey::new(&AES_256_GCM, key_bytes).map_err(|_| "invalid secure key".to_string())?,
    );

    let nonce_start = PREFIX.len();
    let mut nonce_bytes = [0u8; NONCE_LEN];
    nonce_bytes.copy_from_slice(&blob[nonce_start..nonce_start + NONCE_LEN]);
    let nonce = Nonce::assume_unique_for_key(nonce_bytes);

    let mut payload = blob[nonce_start + NONCE_LEN..].to_vec();
    let plaintext = key
        .open_in_place(nonce, Aad::from(AAD), &mut payload)
        .map_err(|_| "failed to decrypt".to_string())?;
    String::from_utf8(plaintext.to_vec()).map_err(|error| error.to_string())
}

/// Reads the key file, generating it on first use.
///
/// Several callers reach this at once in practice — the tests do, and so can
/// the polling thread and the interface — so the file is guarded within this
/// process, and read through a helper that waits out a write in progress.
fn load_or_create_key() -> Result<[u8; KEY_LEN], String> {
    static KEY_FILE: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _guard = KEY_FILE.lock().unwrap_or_else(|error| error.into_inner());

    let path = paths::secret_key_file();
    if path.exists() {
        return read_key_when_written(&path);
    }

    // Making the directory and the file can both go wrong for the same reason —
    // someone else is doing exactly this, right now — so neither failure is an
    // error until the file has been looked for again. Windows reports that
    // someone-else as "already exists" when the other caller is between calls,
    // but as "access is denied" while it still has the file open, and as
    // "cannot find the file" in the moment before it appears; the v2.1.0
    // release failed on all three across three runs. Looking for the file is
    // the answer to all of them: `read_key_when_written` waits out a write that
    // has not finished. Only when there is nothing to find is the original
    // problem reported, and it is reported as itself.
    if let Err(error) = paths::ensure_dir(&paths::state_dir()) {
        return read_key_when_written(&path).map_err(|_| error.to_string());
    }

    let mut key = [0u8; KEY_LEN];
    SystemRandom::new()
        .fill(&mut key)
        .map_err(|_| "failed to generate a secure key".to_string())?;

    match create_private_file(&path) {
        Ok(mut file) => {
            file.write_all(&key).map_err(|error| error.to_string())?;
            Ok(key)
        }
        // Another caller was quicker — another process (the 1.x daemon shares
        // this key file) or another thread of this one — so use what it wrote
        // rather than making a second key.
        Err(error) => read_key_when_written(&path).map_err(|_| error.to_string()),
    }
}

/// Reads the key file, waiting out a write that has not finished.
///
/// The file is created before its contents are written, so a reader can arrive
/// at an empty one. Short means "still being written" and is waited for; a file
/// longer than a key is corrupt rather than half-written, and no amount of
/// waiting will change it.
fn read_key_when_written(path: &Path) -> Result<[u8; KEY_LEN], String> {
    let mut last = String::new();
    for _ in 0..100 {
        match std::fs::read(path) {
            Ok(bytes) if bytes.len() == KEY_LEN => {
                let mut key = [0u8; KEY_LEN];
                key.copy_from_slice(&bytes);
                return Ok(key);
            }
            Ok(bytes) if bytes.len() < KEY_LEN => {
                last = format!("invalid secure key file: {}", path.display());
            }
            Ok(_) => return Err(format!("invalid secure key file: {}", path.display())),
            Err(error) => last = error.to_string(),
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    Err(last)
}

/// Creates the key file readable only by its owner.
#[cfg(unix)]
fn create_private_file(path: &Path) -> std::io::Result<std::fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
}

/// On Windows the per-user profile directory already restricts access.
#[cfg(not(unix))]
fn create_private_file(path: &Path) -> std::io::Result<std::fs::File> {
    OpenOptions::new().write(true).create_new(true).open(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A key of the tests' own. Everything here but one test uses it, and none
    /// of those ever goes near the key file — what they exercise is the cipher.
    const TEST_KEY: [u8; KEY_LEN] = [7u8; KEY_LEN];

    /// The one test that *is* about the key file needs the others out of the
    /// way while it works: it creates that file, and a second test creating the
    /// same file at the same time is a filesystem race on Windows rather than
    /// anything the code could wait out (v2.1.0 failed its release on exactly
    /// that). The eight threads inside it still run at once, which is the thing
    /// being tested.
    fn one_at_a_time() -> std::sync::MutexGuard<'static, ()> {
        static TURN: std::sync::Mutex<()> = std::sync::Mutex::new(());
        TURN.lock().unwrap_or_else(|error| error.into_inner())
    }

    #[test]
    fn round_trips_a_secret() {
        let blob = encrypt_with(&TEST_KEY, "sk-test-key").expect("encrypts");
        assert!(blob.starts_with(PREFIX));
        assert_eq!(
            decrypt_with(&TEST_KEY, &blob).expect("decrypts"),
            "sk-test-key"
        );
    }

    #[test]
    fn another_key_does_not_decrypt_it() {
        let blob = encrypt_with(&TEST_KEY, "sk-test-key").expect("encrypts");
        assert!(decrypt_with(&[9u8; KEY_LEN], &blob).is_err());
    }

    #[test]
    fn rejects_tampered_payloads() {
        let mut blob = encrypt_with(&TEST_KEY, "sk-test-key").expect("encrypts");
        let last = blob.len() - 1;
        blob[last] ^= 0xff;
        assert!(
            decrypt_with(&TEST_KEY, &blob).is_err(),
            "a flipped bit must not decrypt"
        );
    }

    #[test]
    fn rejects_foreign_blobs() {
        assert!(decrypt_with(&TEST_KEY, b"not-ours").is_err());
        assert!(decrypt_with(&TEST_KEY, b"DSBM1").is_err());
    }

    /// However many callers arrive at once, the key file is made once and they
    /// all end up with the same key.
    #[test]
    fn several_callers_share_one_key() {
        let _turn = one_at_a_time();
        crate::test_support::state_in_a_scratch_directory();

        let callers: Vec<_> = (0..8)
            .map(|_| std::thread::spawn(load_or_create_key))
            .collect();

        let keys: Vec<_> = callers
            .into_iter()
            .map(|caller| caller.join().expect("no caller panics").expect("a key"))
            .collect();
        for key in &keys {
            assert_eq!(key, &keys[0], "every caller gets the same key");
        }
    }
}
