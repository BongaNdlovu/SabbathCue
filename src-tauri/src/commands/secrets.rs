use base64::Engine as _;
use rand::rngs::OsRng;
use rand::RngCore;
use tauri::command;

const SERVICE_NAME: &str = "sabbathcue";

/// Trait for keychain storage operations, allowing mock implementations for testing.
pub trait KeychainStore: Send + Sync {
    fn get_password(&self, name: &str) -> Result<String, keyring::Error>;
    fn set_password(&self, name: &str, password: &str) -> Result<(), keyring::Error>;
}

/// Production implementation using the OS keyring.
pub struct RealKeychainStore;

impl KeychainStore for RealKeychainStore {
    fn get_password(&self, name: &str) -> Result<String, keyring::Error> {
        keyring::Entry::new(SERVICE_NAME, name)?.get_password()
    }

    fn set_password(&self, name: &str, password: &str) -> Result<(), keyring::Error> {
        keyring::Entry::new(SERVICE_NAME, name)?.set_password(password)
    }
}

/// Default keychain store used in production.
static DEFAULT_STORE: RealKeychainStore = RealKeychainStore;

fn entry(name: &str) -> keyring::Entry {
    // The keyring crate uses the OS credential manager (Credential Manager, Keychain, etc.)
    // `name` acts like the account/username within the service namespace.
    keyring::Entry::new(SERVICE_NAME, name).expect("keyring entry construction should not fail")
}

fn generate_token() -> String {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

#[command]
pub fn has_deepgram_api_key() -> Result<bool, String> {
    match entry("deepgram_api_key").get_password() {
        Ok(pw) => Ok(!pw.trim().is_empty()),
        Err(keyring::Error::NoEntry) => Ok(false),
        Err(e) => Err(format!("Could not read Deepgram API key from OS keychain: {e}")),
    }
}

#[command]
pub fn set_deepgram_api_key(api_key: String) -> Result<(), String> {
    if api_key.trim().is_empty() {
        return Err("API key cannot be empty".into());
    }
    entry("deepgram_api_key")
        .set_password(api_key.trim())
        .map_err(|e| format!("Could not store Deepgram API key in OS keychain: {e}"))
}

#[command]
pub fn clear_deepgram_api_key() -> Result<(), String> {
    // keyring v3 does not expose a cross-platform delete API; overwriting with
    // an empty value is sufficient for our "configured vs not configured" model.
    entry("deepgram_api_key")
        .set_password("")
        .map_err(|e| format!("Could not remove Deepgram API key from OS keychain: {e}"))
}

#[command]
pub fn has_remote_http_token() -> Result<bool, String> {
    has_remote_http_token_with_store(&DEFAULT_STORE)
}

/// Testable version that accepts a KeychainStore implementation.
pub fn has_remote_http_token_with_store(store: &dyn KeychainStore) -> Result<bool, String> {
    match store.get_password("remote_http_token") {
        Ok(pw) => Ok(!pw.trim().is_empty()),
        Err(keyring::Error::NoEntry) => Ok(false),
        Err(e) => Err(format!("Could not read remote HTTP token from OS keychain: {e}")),
    }
}

/// Reveal the currently configured remote HTTP token (for copy/paste).
/// This does not persist the value anywhere on the frontend; callers should copy it immediately.
#[command]
pub fn reveal_remote_http_token() -> Result<String, String> {
    entry("remote_http_token")
        .get_password()
        .map_err(|e| format!("Could not read remote HTTP token from OS keychain: {e}"))
}

/// Rotate the remote HTTP token (generates a new one and persists it).
#[command]
pub fn rotate_remote_http_token() -> Result<String, String> {
    rotate_remote_http_token_with_store(&DEFAULT_STORE)
}

/// Testable version that accepts a KeychainStore implementation.
pub fn rotate_remote_http_token_with_store(store: &dyn KeychainStore) -> Result<String, String> {
    let token = generate_token();
    store
        .set_password("remote_http_token", &token)
        .map_err(|e| format!("Could not store remote HTTP token in OS keychain: {e}"))?;
    Ok(token)
}

/// Ensure a remote HTTP token exists. Returns `true` if it was created.
pub fn ensure_remote_http_token_exists() -> Result<bool, String> {
    match entry("remote_http_token").get_password() {
        Ok(pw) if !pw.trim().is_empty() => Ok(false),
        Ok(_) | Err(keyring::Error::NoEntry) => {
            let token = generate_token();
            entry("remote_http_token")
                .set_password(&token)
                .map_err(|e| format!("Could not store remote HTTP token in OS keychain: {e}"))?;
            Ok(true)
        }
        Err(e) => Err(format!("Could not read remote HTTP token from OS keychain: {e}")),
    }
}

pub fn get_remote_http_token() -> Result<String, String> {
    entry("remote_http_token")
        .get_password()
        .map_err(|e| format!("Could not read remote HTTP token from OS keychain: {e}"))
}

pub fn get_deepgram_api_key() -> Result<String, String> {
    entry("deepgram_api_key")
        .get_password()
        .map_err(|e| format!("Could not read Deepgram API key from OS keychain: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Mock keychain store for testing.
    pub struct MockKeychainStore {
        storage: Mutex<std::collections::HashMap<String, String>>,
    }

    impl MockKeychainStore {
        pub fn new() -> Self {
            Self {
                storage: Mutex::new(std::collections::HashMap::new()),
            }
        }
    }

    impl Default for MockKeychainStore {
        fn default() -> Self {
            Self::new()
        }
    }

    impl KeychainStore for MockKeychainStore {
        fn get_password(&self, name: &str) -> Result<String, keyring::Error> {
            self.storage
                .lock()
                .unwrap()
                .get(name)
                .cloned()
                .ok_or(keyring::Error::NoEntry)
        }

        fn set_password(&self, name: &str, password: &str) -> Result<(), keyring::Error> {
            self.storage
                .lock()
                .unwrap()
                .insert(name.to_string(), password.to_string());
            Ok(())
        }
    }

    #[test]
    fn has_remote_http_token_returns_false_when_not_set() {
        let store = MockKeychainStore::new();
        let result = has_remote_http_token_with_store(&store);
        assert_eq!(result.unwrap(), false);
    }

    #[test]
    fn has_remote_http_token_returns_true_when_set() {
        let store = MockKeychainStore::new();
        store
            .set_password("remote_http_token", "test-token")
            .unwrap();
        let result = has_remote_http_token_with_store(&store);
        assert_eq!(result.unwrap(), true);
    }

    #[test]
    fn has_remote_http_token_returns_false_when_empty() {
        let store = MockKeychainStore::new();
        store
            .set_password("remote_http_token", "   ")
            .unwrap();
        let result = has_remote_http_token_with_store(&store);
        assert_eq!(result.unwrap(), false);
    }

    #[test]
    fn rotate_remote_http_token_generates_and_stores_token() {
        let store = MockKeychainStore::new();
        let token = rotate_remote_http_token_with_store(&store).unwrap();

        // Token should be non-empty
        assert!(!token.is_empty());

        // Token should be stored
        let stored = store.get_password("remote_http_token").unwrap();
        assert_eq!(stored, token);
    }

    #[test]
    fn rotate_remote_http_token_overwrites_existing() {
        let store = MockKeychainStore::new();
        store
            .set_password("remote_http_token", "old-token")
            .unwrap();

        let new_token = rotate_remote_http_token_with_store(&store).unwrap();
        assert_ne!(new_token, "old-token");

        let stored = store.get_password("remote_http_token").unwrap();
        assert_eq!(stored, new_token);
    }

    #[test]
    fn rotate_remote_http_token_generates_unique_tokens() {
        let store = MockKeychainStore::new();
        let token1 = rotate_remote_http_token_with_store(&store).unwrap();
        let token2 = rotate_remote_http_token_with_store(&store).unwrap();

        assert_ne!(token1, token2);
    }
}

