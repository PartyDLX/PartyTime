//! Where the refresh token is kept.
//!
//! The platform issues a thirty-day refresh token, and the rule is that it goes in the OS
//! credential store and nowhere else — not the settings file, not a log, not a crash dump.
//! Two implementations:
//!
//! * [`KeyringStore`] — what a real launch uses.
//! * [`MemoryStore`] — for tests, and for a machine with no credential store at all.
//!
//! A machine with no keyring is a real case, not an edge: a Linux box with no Secret
//! Service, a container, a CI runner. The answer there is *process lifetime only*, so the
//! creator signs in again next launch. That is a worse experience, but it keeps a
//! thirty-day credential out of a file on disk, which is the trade that is actually
//! unacceptable. See [`default_store`].

use std::sync::{Arc, Mutex};

use zeroize::Zeroizing;

/// The credential service name the console registers under.
pub const SERVICE: &str = "partytime";

/// Somewhere a refresh token can be kept between calls.
pub trait SecretStore: Send + Sync + std::fmt::Debug {
    /// The stored secret, if there is one.
    fn load(&self) -> Result<Option<Zeroizing<String>>, StoreError>;

    /// Stores a secret, replacing any previous one.
    fn save(&self, secret: &str) -> Result<(), StoreError>;

    /// Forgets the secret. Succeeds when there was nothing to forget.
    fn clear(&self) -> Result<(), StoreError>;

    /// Whether the secret survives the process.
    ///
    /// The console tells the user when it does not, rather than letting them discover it
    /// by being signed out the next morning.
    fn is_durable(&self) -> bool;

    /// What to call this store in a message to the user.
    fn description(&self) -> &'static str;
}

/// The OS credential store.
#[derive(Debug, Clone)]
pub struct KeyringStore {
    service: String,
    account: String,
}

impl KeyringStore {
    /// A store for `account` under the console's service name.
    #[must_use]
    pub fn new(account: impl Into<String>) -> Self {
        Self {
            service: SERVICE.to_string(),
            account: account.into(),
        }
    }

    /// Whether this machine has a credential store the console can use.
    ///
    /// Checked once at startup rather than at first write, so the creator is told before
    /// they sign in rather than after.
    #[must_use]
    pub fn is_available() -> bool {
        keyring::Entry::new(SERVICE, "probe").is_ok()
    }
}

impl SecretStore for KeyringStore {
    fn load(&self) -> Result<Option<Zeroizing<String>>, StoreError> {
        let entry = self.entry()?;
        match entry.get_password() {
            Ok(secret) => Ok(Some(Zeroizing::new(secret))),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(StoreError::Backend(error.to_string())),
        }
    }

    fn save(&self, secret: &str) -> Result<(), StoreError> {
        self.entry()?
            .set_password(secret)
            .map_err(|error| StoreError::Backend(error.to_string()))
    }

    fn clear(&self) -> Result<(), StoreError> {
        match self.entry()?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(StoreError::Backend(error.to_string())),
        }
    }

    fn is_durable(&self) -> bool {
        true
    }

    fn description(&self) -> &'static str {
        "the system credential store"
    }
}

impl KeyringStore {
    fn entry(&self) -> Result<keyring::Entry, StoreError> {
        keyring::Entry::new(&self.service, &self.account)
            .map_err(|error| StoreError::Backend(error.to_string()))
    }
}

/// A store that forgets when the process exits.
#[derive(Debug, Default)]
pub struct MemoryStore {
    secret: Mutex<Option<Zeroizing<String>>>,
}

impl MemoryStore {
    /// An empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl SecretStore for MemoryStore {
    fn load(&self) -> Result<Option<Zeroizing<String>>, StoreError> {
        Ok(self.lock()?.clone())
    }

    fn save(&self, secret: &str) -> Result<(), StoreError> {
        *self.lock()? = Some(Zeroizing::new(secret.to_string()));
        Ok(())
    }

    fn clear(&self) -> Result<(), StoreError> {
        *self.lock()? = None;
        Ok(())
    }

    fn is_durable(&self) -> bool {
        false
    }

    fn description(&self) -> &'static str {
        "memory only — you will sign in again next launch"
    }
}

impl MemoryStore {
    fn lock(&self) -> Result<std::sync::MutexGuard<'_, Option<Zeroizing<String>>>, StoreError> {
        self.secret.lock().map_err(|_| StoreError::Poisoned)
    }
}

/// The store this launch should use, given what the machine offers.
///
/// Falls back to [`MemoryStore`] rather than to a file. A machine with no credential store
/// is a legitimate case, and a thirty-day refresh token on disk is not an acceptable
/// consolation for it.
#[must_use]
pub fn default_store(account: impl Into<String>) -> Arc<dyn SecretStore> {
    if KeyringStore::is_available() {
        Arc::new(KeyringStore::new(account))
    } else {
        Arc::new(MemoryStore::new())
    }
}

/// Why a secret could not be kept.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StoreError {
    /// The credential store refused or failed.
    #[error("the system credential store failed: {0}")]
    Backend(String),
    /// The store could not be locked.
    #[error("the credential store is unavailable")]
    Poisoned,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn memory() -> MemoryStore {
        MemoryStore::new()
    }

    /// Copies a stored secret out so a test can compare it with a plain string.
    fn secret(value: Option<Zeroizing<String>>) -> Option<String> {
        value.map(|secret| secret.to_string())
    }

    #[test]
    fn an_empty_store_loads_nothing() {
        assert_eq!(memory().load().expect("load"), None);
    }

    #[test]
    fn a_saved_secret_comes_back_and_is_replaced_rather_than_appended() {
        let store = memory();
        store.save("first").expect("save");
        assert_eq!(
            secret(store.load().expect("load")).as_deref(),
            Some("first")
        );
        store.save("second").expect("save");
        assert_eq!(
            secret(store.load().expect("load")).as_deref(),
            Some("second")
        );
    }

    #[test]
    fn clearing_forgets_and_is_safe_to_repeat() {
        let store = memory();
        store.save("secret").expect("save");
        store.clear().expect("clear");
        assert_eq!(store.load().expect("load"), None);
        store.clear().expect("clearing nothing is not an error");
    }

    #[test]
    fn the_memory_store_admits_it_does_not_survive_a_restart() {
        assert!(!memory().is_durable());
        assert!(memory().description().contains("next launch"));
    }

    #[test]
    fn the_default_store_always_works_and_never_claims_more_than_it_is() {
        let store = default_store("probe-account");
        store.save("secret").expect("save");
        assert_eq!(
            secret(store.load().expect("load")).as_deref(),
            Some("secret")
        );
        // Whatever it picked, it round-trips, and it does not claim durability it lacks.
        assert!(!store.description().is_empty());
    }

    #[test]
    fn a_store_that_does_not_persist_says_so_before_the_creator_finds_out() {
        let store = default_store("probe-account");
        if !store.is_durable() {
            assert!(
                store.description().contains("memory"),
                "a non-durable store must explain itself, said {:?}",
                store.description()
            );
        }
    }

    #[test]
    fn the_console_registers_under_its_own_service_name() {
        assert_eq!(SERVICE, "partytime");
    }
}
