//! API keys, kept in the macOS Keychain (ADR-020, DATA_MODEL §11.2). Keys
//! stay in Rust: nothing here is serializable, and no command returns one.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use keyring_core::api::CredentialStoreApi;

/// The Keychain service under which provider keys are stored. The account
/// is the provider's ID.
pub const KEYCHAIN_SERVICE: &str = "app.forma.desktop.ai-provider";

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct SecretError(String);

pub trait SecretStore: Send + Sync {
    fn get(&self, account: &str) -> Result<Option<String>, SecretError>;
    fn set(&self, account: &str, secret: &str) -> Result<(), SecretError>;
    /// Deleting a key that isn't there succeeds.
    fn delete(&self, account: &str) -> Result<(), SecretError>;
}

/// The user's login keychain.
pub struct KeychainStore {
    store: Arc<apple_native_keyring_store::keychain::Store>,
}

impl KeychainStore {
    pub fn new() -> Result<Self, SecretError> {
        let store = apple_native_keyring_store::keychain::Store::new().map_err(describe)?;
        Ok(Self { store })
    }

    fn entry(&self, account: &str) -> Result<keyring_core::Entry, SecretError> {
        self.store
            .build(KEYCHAIN_SERVICE, account, None)
            .map_err(describe)
    }
}

/// Keychain errors in plain words. Never includes the secret.
fn describe(err: keyring_core::Error) -> SecretError {
    SecretError(match err {
        keyring_core::Error::NoStorageAccess(_) => {
            "Forma isn't allowed to use the Keychain. Unlock it, or allow Forma when macOS asks."
                .into()
        }
        other => format!("The Keychain reported a problem: {other}"),
    })
}

impl SecretStore for KeychainStore {
    fn get(&self, account: &str) -> Result<Option<String>, SecretError> {
        match self.entry(account)?.get_password() {
            Ok(secret) => Ok(Some(secret)),
            Err(keyring_core::Error::NoEntry) => Ok(None),
            Err(err) => Err(describe(err)),
        }
    }

    fn set(&self, account: &str, secret: &str) -> Result<(), SecretError> {
        self.entry(account)?.set_password(secret).map_err(describe)
    }

    fn delete(&self, account: &str) -> Result<(), SecretError> {
        match self.entry(account)?.delete_credential() {
            Ok(()) | Err(keyring_core::Error::NoEntry) => Ok(()),
            Err(err) => Err(describe(err)),
        }
    }
}

/// An in-memory store for tests and for when the Keychain can't be opened.
#[derive(Default)]
pub struct MemoryStore {
    secrets: Mutex<HashMap<String, String>>,
}

impl SecretStore for MemoryStore {
    fn get(&self, account: &str) -> Result<Option<String>, SecretError> {
        Ok(self.secrets.lock().unwrap().get(account).cloned())
    }

    fn set(&self, account: &str, secret: &str) -> Result<(), SecretError> {
        self.secrets
            .lock()
            .unwrap()
            .insert(account.to_string(), secret.to_string());
        Ok(())
    }

    fn delete(&self, account: &str) -> Result<(), SecretError> {
        self.secrets.lock().unwrap().remove(account);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_store_round_trips() {
        let store = MemoryStore::default();
        assert_eq!(store.get("p").unwrap(), None);
        store.set("p", "k1").unwrap();
        store.set("p", "k2").unwrap();
        assert_eq!(store.get("p").unwrap().as_deref(), Some("k2"));
        store.delete("p").unwrap();
        store.delete("p").unwrap();
        assert_eq!(store.get("p").unwrap(), None);
    }

    /// Writes, reads and deletes a real item in the login keychain. Ignored by
    /// default because it touches the user's keychain and needs a logged-in
    /// session; run with `cargo test -p forma-ai -- --ignored keychain`.
    #[test]
    #[ignore = "uses the real login keychain"]
    fn keychain_round_trip() {
        let store = KeychainStore::new().unwrap();
        let account = format!("forma-test-{}", std::process::id());
        store.set(&account, "test-secret-1").unwrap();
        store.set(&account, "test-secret-2").unwrap();
        assert_eq!(
            store.get(&account).unwrap().as_deref(),
            Some("test-secret-2")
        );
        store.delete(&account).unwrap();
        assert_eq!(store.get(&account).unwrap(), None);
        store.delete(&account).unwrap();
    }
}
