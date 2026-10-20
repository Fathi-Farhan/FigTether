//! Secret generation, single use pairing secrets, and constant time compare.
//!
//! The daemon holds two kinds of secret material:
//!
//! - the pairing secret, which the user shows to the plugin UI. It lives in
//!   daemon memory only, is single use, and expires after five minutes;
//! - the client credential, which the local clients read from the owner only
//!   runtime directory and present on the first frame of every connection.
//!
//! Both are compared in constant time. A comparison never returns early on a
//! mismatch, so a caller cannot learn a prefix of a secret by timing the
//! answer.

use std::collections::HashMap;
use std::fmt;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use rand::Rng as _;
use subtle::ConstantTimeEq;
use zeroize::Zeroizing;

/// Number of random bytes in a generated secret.
pub const SECRET_BYTES: usize = 32;

/// Default lifetime of a pairing secret.
pub const PAIRING_SECRET_TTL: Duration = Duration::from_secs(5 * 60);

/// Number of bytes compared in one constant time comparison.
///
/// Every comparison walks this many bytes so the running time does not depend
/// on where the first difference is.
const COMPARE_WINDOW: usize = 256;

/// A secret held in memory.
///
/// The value is zeroized when the last copy is dropped and it is never printed
/// by the `Debug` implementation.
#[derive(Clone)]
pub struct Secret(Zeroizing<String>);

impl Secret {
    /// Generate a secret with [`SECRET_BYTES`] bytes of entropy.
    pub fn generate() -> Self {
        Self::generate_with(SECRET_BYTES)
    }

    /// Generate a secret with the given number of random bytes.
    pub fn generate_with(bytes: usize) -> Self {
        let mut buffer = Zeroizing::new(vec![0u8; bytes]);
        rand::rng().fill_bytes(buffer.as_mut_slice());
        Self(Zeroizing::new(URL_SAFE_NO_PAD.encode(buffer.as_slice())))
    }

    /// Wrap an existing secret value.
    ///
    /// The value is copied into a zeroizing buffer, so the copy is wiped
    /// when the secret is dropped.
    pub fn from_value(value: &str) -> Self {
        Self(Zeroizing::new(value.to_string()))
    }

    /// The secret as a string slice.
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    /// Length of the encoded secret.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether the secret is empty.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret([redacted])")
    }
}

/// Compare two secrets in constant time.
///
/// The comparison always walks [`COMPARE_WINDOW`] bytes. A length difference
/// is folded into the result instead of returning early, so the running time
/// does not depend on the content of either input.
pub fn constant_time_eq(left: &str, right: &str) -> bool {
    let left_bytes = left.as_bytes();
    let right_bytes = right.as_bytes();
    if left_bytes.len() > COMPARE_WINDOW || right_bytes.len() > COMPARE_WINDOW {
        return false;
    }

    let mut left_window = [0u8; COMPARE_WINDOW];
    let mut right_window = [0u8; COMPARE_WINDOW];
    left_window[..left_bytes.len()].copy_from_slice(left_bytes);
    right_window[..right_bytes.len()].copy_from_slice(right_bytes);

    let same_length = (left_bytes.len() as u16).ct_eq(&(right_bytes.len() as u16));
    let same_bytes = left_window.as_slice().ct_eq(right_window.as_slice());
    bool::from(same_length & same_bytes)
}

/// Why a pairing secret was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PairingError {
    /// The secret is unknown, already consumed, or simply wrong.
    Unknown,
    /// The secret existed but its lifetime elapsed.
    Expired,
}

impl PairingError {
    /// Stable error code for the failure.
    pub const fn code(self) -> &'static str {
        match self {
            Self::Unknown => "AUTH_REQUIRED",
            Self::Expired => "AUTH_EXPIRED",
        }
    }

    /// Human readable reason.
    pub const fn message(self) -> &'static str {
        match self {
            Self::Unknown => "the pairing secret is unknown or already used",
            Self::Expired => "the pairing secret expired",
        }
    }
}

/// One pending pairing secret.
#[derive(Debug)]
struct PairingEntry {
    /// The secret itself.
    secret: Secret,
    /// The owner the secret was minted for.
    owner: String,
    /// When the secret stops being valid.
    expires_at: Instant,
}

/// A pairing secret together with the owner it was minted for.
///
/// The owner is the local installation that asked for the secret. The plugin
/// that spends the secret inherits the ownership, so the session it opens
/// belongs to the installation and not to the plugin UI.
#[derive(Debug, Clone)]
pub struct PairedSecret {
    /// The secret itself.
    pub secret: Secret,
    /// The local installation the secret was minted for.
    pub owner: String,
}

/// The set of pairing secrets the daemon accepts.
///
/// A secret is single use: the first successful comparison consumes it. A
/// failed comparison never consumes a secret, so a typo does not burn the
/// secret the user is looking at.
#[derive(Debug)]
pub struct PairingSecrets {
    /// Pending secrets by encoded value.
    inner: Mutex<HashMap<String, PairingEntry>>,
    /// Lifetime of a minted secret.
    ttl: Duration,
}

impl PairingSecrets {
    /// Create an empty registry with the given secret lifetime.
    pub fn new(ttl: Duration) -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
            ttl,
        }
    }

    /// Mint a new single use secret for one owner.
    ///
    /// The owner is the local installation the secret belongs to. The plugin
    /// that spends the secret opens a session of that owner.
    pub fn mint(&self, owner: impl Into<String>) -> Secret {
        let secret = Secret::generate();
        let now = Instant::now();
        let mut guard = self.lock();
        guard.retain(|_, entry| entry.expires_at > now);
        guard.insert(
            secret.as_str().to_string(),
            PairingEntry {
                secret: secret.clone(),
                owner: owner.into(),
                expires_at: now + self.ttl,
            },
        );
        secret
    }

    /// Consume the secret the caller presented.
    ///
    /// The comparison runs over every pending secret in constant time. On
    /// success the secret is removed, so it can never be replayed. The answer
    /// carries the owner the secret was minted for.
    pub fn consume(&self, presented: &str) -> Result<PairedSecret, PairingError> {
        let now = Instant::now();
        let mut guard = self.lock();

        let mut matched: Option<String> = None;
        for (key, _entry) in guard.iter() {
            if constant_time_eq(key, presented) {
                matched = Some(key.clone());
            }
        }

        let Some(key) = matched else {
            return Err(PairingError::Unknown);
        };

        let entry = guard.remove(&key).ok_or(PairingError::Unknown)?;
        if entry.expires_at <= now {
            return Err(PairingError::Expired);
        }
        Ok(PairedSecret {
            secret: entry.secret,
            owner: entry.owner,
        })
    }

    /// Number of secrets that are still usable.
    pub fn active_count(&self) -> usize {
        let now = Instant::now();
        self.lock()
            .values()
            .filter(|entry| entry.expires_at > now)
            .count()
    }

    /// Drop every pending secret.
    pub fn revoke_all(&self) {
        self.lock().clear();
    }

    /// Lock the registry, recovering from a poisoned lock.
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, PairingEntry>> {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

impl Default for PairingSecrets {
    fn default() -> Self {
        Self::new(PAIRING_SECRET_TTL)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_secrets_carry_256_bits() {
        let secret = Secret::generate();
        let decoded = URL_SAFE_NO_PAD.decode(secret.as_str()).expect("decodes");
        assert_eq!(decoded.len(), SECRET_BYTES);
        assert_eq!(decoded.len() * 8, 256);
    }

    #[test]
    fn generated_secrets_are_unique() {
        let first = Secret::generate();
        let second = Secret::generate();
        assert_ne!(first.as_str(), second.as_str());
        assert!(!first.is_empty());
    }

    #[test]
    fn secret_debug_output_is_redacted() {
        let secret = Secret::generate();
        let rendered = format!("{secret:?}");
        assert!(!rendered.contains(secret.as_str()));
        assert!(rendered.contains("redacted"));
    }

    #[test]
    fn constant_time_compare_accepts_only_equal_values() {
        assert!(constant_time_eq("abc", "abc"));
        assert!(!constant_time_eq("abc", "abd"));
        assert!(!constant_time_eq("abc", "ab"));
        assert!(!constant_time_eq("", "a"));
        assert!(constant_time_eq("", ""));
    }

    #[test]
    fn constant_time_compare_rejects_oversized_values() {
        let long = "x".repeat(COMPARE_WINDOW + 1);
        assert!(!constant_time_eq(&long, &long));
    }

    #[test]
    fn pairing_secret_is_single_use() {
        let secrets = PairingSecrets::default();
        let secret = secrets.mint("installation-1");
        assert_eq!(secrets.active_count(), 1);

        let first = secrets.consume(secret.as_str()).expect("accepted");
        assert_eq!(first.secret.as_str(), secret.as_str());
        assert_eq!(first.owner, "installation-1");
        assert_eq!(secrets.active_count(), 0);

        let second = secrets
            .consume(secret.as_str())
            .expect_err("replay refused");
        assert_eq!(second, PairingError::Unknown);
    }

    #[test]
    fn a_pairing_secret_carries_its_owner() {
        let secrets = PairingSecrets::default();
        let secret = secrets.mint("installation-7");
        let paired = secrets.consume(secret.as_str()).expect("accepted");
        assert_eq!(paired.owner, "installation-7");
    }

    #[test]
    fn wrong_secret_does_not_consume_the_pending_one() {
        let secrets = PairingSecrets::default();
        let secret = secrets.mint("installation-1");
        let err = secrets.consume("not-the-secret").expect_err("refused");
        assert_eq!(err, PairingError::Unknown);
        assert_eq!(secrets.active_count(), 1);
        assert!(secrets.consume(secret.as_str()).is_ok());
    }

    #[test]
    fn expired_secret_is_refused_and_removed() {
        let secrets = PairingSecrets::new(Duration::from_millis(1));
        let secret = secrets.mint("installation-1");
        std::thread::sleep(Duration::from_millis(5));
        let err = secrets.consume(secret.as_str()).expect_err("expired");
        assert_eq!(err, PairingError::Expired);
        assert_eq!(secrets.active_count(), 0);
    }

    #[test]
    fn revoke_all_drops_every_secret() {
        let secrets = PairingSecrets::default();
        let secret = secrets.mint("installation-1");
        secrets.revoke_all();
        assert_eq!(secrets.active_count(), 0);
        assert!(secrets.consume(secret.as_str()).is_err());
    }
}
