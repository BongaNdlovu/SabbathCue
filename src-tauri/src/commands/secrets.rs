use base64::Engine as _;
use tauri::command;

const SERVICE_NAME: &str = "sabbathcue";

/// Trait for keychain storage operations, allowing mock implementations for testing.
pub trait KeychainStore: Send + Sync {
    fn get_password(&self, name: &str) -> Result<String, keyring::Error>;
    fn set_password(&self, name: &str, password: &str) -> Result<(), keyring::Error>;
    /// Remove the credential entirely; absent entries are not an error.
    fn delete_password(&self, name: &str) -> Result<(), keyring::Error>;
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

    fn delete_password(&self, name: &str) -> Result<(), keyring::Error> {
        match keyring::Entry::new(SERVICE_NAME, name)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e),
        }
    }
}

/// Default keychain store used in production.
static DEFAULT_STORE: RealKeychainStore = RealKeychainStore;

fn entry(name: &str) -> Result<keyring::Entry, String> {
    // The keyring crate uses the OS credential manager (Credential Manager, Keychain, etc.)
    // `name` acts like the account/username within the service namespace.
    keyring::Entry::new(SERVICE_NAME, name)
        .map_err(|e| format!("Could not access OS keychain entry: {e}"))
}

fn generate_token() -> String {
    let bytes: [u8; 32] = rand::random();
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

pub fn normalize_deepgram_api_key(api_key: &str) -> String {
    let trimmed = api_key
        .trim()
        .trim_matches(|c| matches!(c, '"' | '\'' | '`'));
    let without_header = trimmed
        .strip_prefix("Authorization:")
        .map_or(trimmed, str::trim);

    for prefix in ["Token ", "token ", "Bearer ", "bearer "] {
        if let Some(key) = without_header.strip_prefix(prefix) {
            return key.trim().to_string();
        }
    }

    without_header.to_string()
}

#[command]
pub fn has_deepgram_api_key() -> Result<bool, String> {
    has_deepgram_api_key_with_store(&DEFAULT_STORE)
}

/// Testable version that accepts a `KeychainStore` implementation.
pub fn has_deepgram_api_key_with_store(store: &dyn KeychainStore) -> Result<bool, String> {
    match store.get_password("deepgram_api_key") {
        Ok(pw) => Ok(!pw.trim().is_empty()),
        Err(keyring::Error::NoEntry) => Ok(false),
        Err(e) => Err(format!(
            "Could not read Deepgram API key from OS keychain: {e}"
        )),
    }
}

#[command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri command extractors require pass-by-value"
)]
pub fn set_deepgram_api_key(api_key: String) -> Result<(), String> {
    set_deepgram_api_key_with_store(&DEFAULT_STORE, &api_key)
}

/// Testable version that accepts a `KeychainStore` implementation.
pub fn set_deepgram_api_key_with_store(
    store: &dyn KeychainStore,
    api_key: &str,
) -> Result<(), String> {
    let normalized = normalize_deepgram_api_key(api_key);
    if normalized.is_empty() {
        return Err("API key cannot be empty".into());
    }
    store
        .set_password("deepgram_api_key", &normalized)
        .map_err(|e| {
            log::error!("[KEYCHAIN] Failed to store Deepgram API key: {e}");
            format!("Could not store Deepgram API key in OS keychain: {e}")
        })?;
    match store.get_password("deepgram_api_key") {
        Ok(pw) if !pw.trim().is_empty() => Ok(()),
        Ok(_) | Err(keyring::Error::NoEntry) => {
            log::error!("[KEYCHAIN] Deepgram API key read-back failed: stored value empty or missing after write");
            Err("Deepgram API key was not saved in OS keychain".into())
        }
        Err(e) => {
            log::error!("[KEYCHAIN] Deepgram API key read-back failed: {e}");
            Err(format!(
                "Could not verify Deepgram API key in OS keychain: {e}"
            ))
        }
    }
}

#[command]
pub fn clear_deepgram_api_key() -> Result<(), String> {
    DEFAULT_STORE
        .delete_password("deepgram_api_key")
        .map_err(|e| format!("Could not remove Deepgram API key from OS keychain: {e}"))
}

pub fn normalize_soniox_api_key(api_key: &str) -> String {
    api_key
        .trim()
        .trim_matches(|c| matches!(c, '"' | '\'' | '`'))
        .to_string()
}

#[command]
pub fn has_soniox_api_key() -> Result<bool, String> {
    has_soniox_api_key_with_store(&DEFAULT_STORE)
}

pub fn has_soniox_api_key_with_store(store: &dyn KeychainStore) -> Result<bool, String> {
    match store.get_password("soniox_api_key") {
        Ok(pw) => Ok(!pw.trim().is_empty()),
        Err(keyring::Error::NoEntry) => Ok(false),
        Err(e) => Err(format!(
            "Could not read Soniox API key from OS keychain: {e}"
        )),
    }
}

#[command]
pub fn set_soniox_api_key(api_key: &str) -> Result<(), String> {
    set_soniox_api_key_with_store(&DEFAULT_STORE, api_key)
}

pub fn set_soniox_api_key_with_store(
    store: &dyn KeychainStore,
    api_key: &str,
) -> Result<(), String> {
    let normalized = normalize_soniox_api_key(api_key);
    if normalized.is_empty() {
        return Err("API key cannot be empty".into());
    }
    store
        .set_password("soniox_api_key", &normalized)
        .map_err(|e| {
            log::error!("[KEYCHAIN] Failed to store Soniox API key: {e}");
            format!("Could not store Soniox API key in OS keychain: {e}")
        })?;
    Ok(())
}

#[command]
pub fn clear_soniox_api_key() -> Result<(), String> {
    DEFAULT_STORE
        .delete_password("soniox_api_key")
        .map_err(|e| format!("Could not remove Soniox API key from OS keychain: {e}"))
}

pub fn get_soniox_api_key_or_empty() -> Result<String, String> {
    get_soniox_api_key_or_empty_with_store(&DEFAULT_STORE)
}

pub fn get_soniox_api_key_or_empty_with_store(store: &dyn KeychainStore) -> Result<String, String> {
    match store.get_password("soniox_api_key") {
        Ok(key) => Ok(normalize_soniox_api_key(&key)),
        Err(keyring::Error::NoEntry) => Ok(String::new()),
        Err(e) => Err(format!(
            "Could not read Soniox API key from OS keychain: {e}"
        )),
    }
}

pub fn normalize_speechmatics_api_key(api_key: &str) -> String {
    let trimmed = api_key
        .trim()
        .trim_matches(|c| matches!(c, '"' | '\'' | '`'));
    trimmed
        .strip_prefix("Bearer ")
        .unwrap_or(trimmed)
        .trim()
        .to_string()
}

#[command]
pub fn has_speechmatics_api_key() -> Result<bool, String> {
    has_speechmatics_api_key_with_store(&DEFAULT_STORE)
}

pub fn has_speechmatics_api_key_with_store(store: &dyn KeychainStore) -> Result<bool, String> {
    match store.get_password("speechmatics_api_key") {
        Ok(value) => Ok(!value.trim().is_empty()),
        Err(keyring::Error::NoEntry) => Ok(false),
        Err(error) => Err(format!(
            "Could not read Speechmatics API key from OS keychain: {error}"
        )),
    }
}

#[command]
pub fn set_speechmatics_api_key(api_key: &str) -> Result<(), String> {
    set_speechmatics_api_key_with_store(&DEFAULT_STORE, api_key)
}

pub fn set_speechmatics_api_key_with_store(
    store: &dyn KeychainStore,
    api_key: &str,
) -> Result<(), String> {
    let normalized = normalize_speechmatics_api_key(api_key);
    if normalized.is_empty() {
        return Err("API key cannot be empty".into());
    }
    store
        .set_password("speechmatics_api_key", &normalized)
        .map_err(|error| {
            log::error!("[KEYCHAIN] Failed to store Speechmatics API key: {error}");
            format!("Could not store Speechmatics API key in OS keychain: {error}")
        })
}

#[command]
pub fn clear_speechmatics_api_key() -> Result<(), String> {
    DEFAULT_STORE
        .delete_password("speechmatics_api_key")
        .map_err(|error| format!("Could not remove Speechmatics API key from OS keychain: {error}"))
}

pub fn get_speechmatics_api_key_or_empty() -> Result<String, String> {
    get_speechmatics_api_key_or_empty_with_store(&DEFAULT_STORE)
}

pub fn get_speechmatics_api_key_or_empty_with_store(
    store: &dyn KeychainStore,
) -> Result<String, String> {
    match store.get_password("speechmatics_api_key") {
        Ok(value) => Ok(normalize_speechmatics_api_key(&value)),
        Err(keyring::Error::NoEntry) => Ok(String::new()),
        Err(error) => Err(format!(
            "Could not read Speechmatics API key from OS keychain: {error}"
        )),
    }
}

pub fn normalize_deepseek_api_key(api_key: &str) -> String {
    let trimmed = api_key
        .trim()
        .trim_matches(|c| matches!(c, '"' | '\'' | '`'));
    trimmed
        .strip_prefix("Bearer ")
        .unwrap_or(trimmed)
        .trim()
        .to_string()
}

#[command]
pub fn has_deepseek_api_key() -> Result<bool, String> {
    has_deepseek_api_key_with_store(&DEFAULT_STORE)
}

pub fn has_deepseek_api_key_with_store(store: &dyn KeychainStore) -> Result<bool, String> {
    match store.get_password("deepseek_api_key") {
        Ok(value) => Ok(!value.trim().is_empty()),
        Err(keyring::Error::NoEntry) => Ok(false),
        Err(error) => Err(format!(
            "Could not read DeepSeek API key from OS keychain: {error}"
        )),
    }
}

#[command]
pub fn set_deepseek_api_key(api_key: &str) -> Result<(), String> {
    set_deepseek_api_key_with_store(&DEFAULT_STORE, api_key)
}

pub fn set_deepseek_api_key_with_store(
    store: &dyn KeychainStore,
    api_key: &str,
) -> Result<(), String> {
    let normalized = normalize_deepseek_api_key(api_key);
    if normalized.is_empty() {
        return Err("API key cannot be empty".into());
    }
    store
        .set_password("deepseek_api_key", &normalized)
        .map_err(|error| {
            log::error!("[KEYCHAIN] Failed to store DeepSeek API key: {error}");
            format!("Could not store DeepSeek API key in OS keychain: {error}")
        })
}

#[command]
pub fn clear_deepseek_api_key() -> Result<(), String> {
    DEFAULT_STORE
        .delete_password("deepseek_api_key")
        .map_err(|error| format!("Could not remove DeepSeek API key from OS keychain: {error}"))
}

pub fn get_deepseek_api_key_or_empty() -> Result<String, String> {
    get_deepseek_api_key_or_empty_with_store(&DEFAULT_STORE)
}

pub fn get_deepseek_api_key_or_empty_with_store(
    store: &dyn KeychainStore,
) -> Result<String, String> {
    match store.get_password("deepseek_api_key") {
        Ok(value) => Ok(normalize_deepseek_api_key(&value)),
        Err(keyring::Error::NoEntry) => Ok(String::new()),
        Err(error) => Err(format!(
            "Could not read DeepSeek API key from OS keychain: {error}"
        )),
    }
}

pub fn normalize_cerebras_api_key(api_key: &str) -> String {
    let trimmed = api_key
        .trim()
        .trim_matches(|c| matches!(c, '"' | '\'' | '`'));
    trimmed
        .strip_prefix("Bearer ")
        .unwrap_or(trimmed)
        .trim()
        .to_string()
}

#[command]
pub fn has_cerebras_api_key() -> Result<bool, String> {
    has_cerebras_api_key_with_store(&DEFAULT_STORE)
}

pub fn has_cerebras_api_key_with_store(store: &dyn KeychainStore) -> Result<bool, String> {
    match store.get_password("cerebras_api_key") {
        Ok(value) => Ok(!value.trim().is_empty()),
        Err(keyring::Error::NoEntry) => Ok(false),
        Err(error) => Err(format!(
            "Could not read Cerebras API key from OS keychain: {error}"
        )),
    }
}

#[command]
pub fn set_cerebras_api_key(api_key: &str) -> Result<(), String> {
    set_cerebras_api_key_with_store(&DEFAULT_STORE, api_key)
}

pub fn set_cerebras_api_key_with_store(
    store: &dyn KeychainStore,
    api_key: &str,
) -> Result<(), String> {
    let normalized = normalize_cerebras_api_key(api_key);
    if normalized.is_empty() {
        return Err("API key cannot be empty".into());
    }
    store
        .set_password("cerebras_api_key", &normalized)
        .map_err(|error| {
            log::error!("[KEYCHAIN] Failed to store Cerebras API key: {error}");
            format!("Could not store Cerebras API key in OS keychain: {error}")
        })
}

#[command]
pub fn clear_cerebras_api_key() -> Result<(), String> {
    DEFAULT_STORE
        .delete_password("cerebras_api_key")
        .map_err(|error| format!("Could not remove Cerebras API key from OS keychain: {error}"))
}

pub fn get_cerebras_api_key_or_empty() -> Result<String, String> {
    get_cerebras_api_key_or_empty_with_store(&DEFAULT_STORE)
}

pub fn get_cerebras_api_key_or_empty_with_store(
    store: &dyn KeychainStore,
) -> Result<String, String> {
    match store.get_password("cerebras_api_key") {
        Ok(value) => Ok(normalize_cerebras_api_key(&value)),
        Err(keyring::Error::NoEntry) => Ok(String::new()),
        Err(error) => Err(format!(
            "Could not read Cerebras API key from OS keychain: {error}"
        )),
    }
}

#[command]
pub async fn validate_deepgram_api_key() -> Result<(), String> {
    let key = get_deepgram_api_key_or_empty()?;
    if key.is_empty() {
        return Err("Save a Deepgram API key before testing it.".into());
    }
    rhema_stt::credentials::validate_deepgram_api_key(&key).await
}

#[command]
pub async fn validate_soniox_api_key() -> Result<(), String> {
    let key = get_soniox_api_key_or_empty()?;
    if key.is_empty() {
        return Err("Save a Soniox API key before testing it.".into());
    }
    rhema_stt::credentials::validate_soniox_api_key(&key).await
}

#[command]
pub async fn validate_speechmatics_api_key() -> Result<(), String> {
    let key = get_speechmatics_api_key_or_empty()?;
    if key.is_empty() {
        return Err("Save a Speechmatics API key before testing it.".into());
    }
    rhema_stt::credentials::validate_speechmatics_api_key(&key).await
}

#[command]
pub fn has_remote_http_token() -> Result<bool, String> {
    has_remote_http_token_with_store(&DEFAULT_STORE)
}

#[command]
pub fn has_verification_token() -> Result<bool, String> {
    has_verification_token_with_store(&DEFAULT_STORE)
}

pub fn has_verification_token_with_store(store: &dyn KeychainStore) -> Result<bool, String> {
    match store.get_password("verification_token") {
        Ok(pw) => Ok(!pw.trim().is_empty()),
        Err(keyring::Error::NoEntry) => Ok(false),
        Err(e) => Err(format!(
            "Could not read verification token from OS keychain: {e}"
        )),
    }
}

#[command]
pub fn rotate_verification_token() -> Result<String, String> {
    rotate_verification_token_with_store(&DEFAULT_STORE)
}

pub fn rotate_verification_token_with_store(store: &dyn KeychainStore) -> Result<String, String> {
    let token = generate_token();
    store
        .set_password("verification_token", &token)
        .map_err(|e| format!("Could not store verification token in OS keychain: {e}"))?;
    Ok(token)
}

#[command]
pub fn clear_verification_token() -> Result<(), String> {
    clear_verification_token_with_store(&DEFAULT_STORE)
}

pub fn clear_verification_token_with_store(store: &dyn KeychainStore) -> Result<(), String> {
    store
        .delete_password("verification_token")
        .map_err(|e| format!("Could not clear verification token from OS keychain: {e}"))
}

#[command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri command extractors require pass-by-value"
)]
pub fn set_verification_token(value: String) -> Result<(), String> {
    set_verification_token_with_store(&DEFAULT_STORE, &value)
}

pub fn set_verification_token_with_store(
    store: &dyn KeychainStore,
    value: &str,
) -> Result<(), String> {
    if value.trim().is_empty() {
        return Err("Verification token cannot be empty".into());
    }
    store
        .set_password("verification_token", value)
        .map_err(|e| format!("Could not store verification token in OS keychain: {e}"))
}

#[command]
pub fn get_verification_token() -> Result<String, String> {
    get_verification_token_with_store(&DEFAULT_STORE)
}

pub fn get_verification_token_with_store(store: &dyn KeychainStore) -> Result<String, String> {
    match store.get_password("verification_token") {
        Ok(pw) if pw.trim().is_empty() => Ok(String::new()),
        Ok(pw) => Ok(pw),
        Err(keyring::Error::NoEntry) => Ok(String::new()),
        Err(e) => Err(format!(
            "Could not read verification token from OS keychain: {e}"
        )),
    }
}

/// Testable version that accepts a `KeychainStore` implementation.
pub fn has_remote_http_token_with_store(store: &dyn KeychainStore) -> Result<bool, String> {
    match store.get_password("remote_http_token") {
        Ok(pw) => Ok(!pw.trim().is_empty()),
        Err(keyring::Error::NoEntry) => Ok(false),
        Err(e) => Err(format!(
            "Could not read remote HTTP token from OS keychain: {e}"
        )),
    }
}

/// Rotate the remote HTTP token (generates a new one and persists it).
pub fn rotate_remote_http_token() -> Result<String, String> {
    rotate_remote_http_token_with_store(&DEFAULT_STORE)
}

/// Testable version that accepts a `KeychainStore` implementation.
pub fn rotate_remote_http_token_with_store(store: &dyn KeychainStore) -> Result<String, String> {
    let token = generate_token();
    store
        .set_password("remote_http_token", &token)
        .map_err(|e| format!("Could not store remote HTTP token in OS keychain: {e}"))?;
    Ok(token)
}

/// Ensure a remote HTTP token exists. Returns `true` if it was created.
pub fn ensure_remote_http_token_exists() -> Result<bool, String> {
    ensure_remote_http_token_exists_with_store(&DEFAULT_STORE)
}

pub fn ensure_remote_http_token_exists_with_store(
    store: &dyn KeychainStore,
) -> Result<bool, String> {
    match store.get_password("remote_http_token") {
        Ok(pw) if !pw.trim().is_empty() => Ok(false),
        Ok(_) => Err(
            "Remote HTTP token is empty. Rotate the remote token before starting remote control."
                .into(),
        ),
        Err(keyring::Error::NoEntry) => {
            let token = generate_token();
            store
                .set_password("remote_http_token", &token)
                .map_err(|e| format!("Could not store remote HTTP token in OS keychain: {e}"))?;
            Ok(true)
        }
        Err(e) => Err(format!(
            "Could not read remote HTTP token from OS keychain: {e}"
        )),
    }
}

pub fn get_remote_http_token() -> Result<String, String> {
    entry("remote_http_token")?
        .get_password()
        .map_err(|e| format!("Could not read remote HTTP token from OS keychain: {e}"))
}

pub fn get_deepgram_api_key_or_empty() -> Result<String, String> {
    get_deepgram_api_key_or_empty_with_store(&DEFAULT_STORE)
}

pub fn get_deepgram_api_key_or_empty_with_store(
    store: &dyn KeychainStore,
) -> Result<String, String> {
    match store.get_password("deepgram_api_key") {
        Ok(key) => Ok(normalize_deepgram_api_key(&key)),
        Err(keyring::Error::NoEntry) => Ok(String::new()),
        Err(e) => Err(format!(
            "Could not read Deepgram API key from OS keychain: {e}"
        )),
    }
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

        fn delete_password(&self, name: &str) -> Result<(), keyring::Error> {
            self.storage.lock().unwrap().remove(name);
            Ok(())
        }
    }

    #[test]
    fn has_remote_http_token_returns_false_when_not_set() {
        let store = MockKeychainStore::new();
        let result = has_remote_http_token_with_store(&store);
        assert!(!result.unwrap());
    }

    #[test]
    fn has_remote_http_token_returns_true_when_set() {
        let store = MockKeychainStore::new();
        store
            .set_password("remote_http_token", "test-token")
            .unwrap();
        let result = has_remote_http_token_with_store(&store);
        assert!(result.unwrap());
    }

    #[test]
    fn has_remote_http_token_returns_false_when_empty() {
        let store = MockKeychainStore::new();
        store.set_password("remote_http_token", "   ").unwrap();
        let result = has_remote_http_token_with_store(&store);
        assert!(!result.unwrap());
    }

    #[test]
    fn ensure_remote_http_token_creates_missing_token() {
        let store = MockKeychainStore::new();

        let created = ensure_remote_http_token_exists_with_store(&store).unwrap();

        assert!(created);
        assert!(!store
            .get_password("remote_http_token")
            .unwrap()
            .trim()
            .is_empty());
    }

    #[test]
    fn ensure_remote_http_token_rejects_empty_token_without_overwriting() {
        let store = MockKeychainStore::new();
        store.set_password("remote_http_token", "   ").unwrap();

        let result = ensure_remote_http_token_exists_with_store(&store);

        assert!(result.is_err());
        assert_eq!(store.get_password("remote_http_token").unwrap(), "   ");
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

    #[test]
    fn verification_token_is_detected_after_rotation() {
        let store = MockKeychainStore::new();
        assert!(!has_verification_token_with_store(&store).unwrap());

        let token = rotate_verification_token_with_store(&store).unwrap();

        assert!(!token.is_empty());
        assert!(has_verification_token_with_store(&store).unwrap());
    }

    #[test]
    fn set_verification_token_round_trips_through_get() {
        let store = MockKeychainStore::new();
        set_verification_token_with_store(&store, "supabase-refresh-token").unwrap();

        let stored = get_verification_token_with_store(&store).unwrap();
        assert_eq!(stored, "supabase-refresh-token");
        assert!(has_verification_token_with_store(&store).unwrap());
    }

    #[test]
    fn get_verification_token_returns_empty_when_missing() {
        let store = MockKeychainStore::new();
        let result = get_verification_token_with_store(&store).unwrap();
        assert_eq!(result, "");
        assert!(!has_verification_token_with_store(&store).unwrap());
    }

    #[test]
    fn get_verification_token_treats_empty_string_as_absent() {
        let store = MockKeychainStore::new();
        store.set_password("verification_token", "   ").unwrap();

        let result = get_verification_token_with_store(&store).unwrap();
        assert_eq!(result, "");
        assert!(!has_verification_token_with_store(&store).unwrap());
    }

    #[test]
    fn clear_verification_token_makes_get_return_empty() {
        let store = MockKeychainStore::new();
        set_verification_token_with_store(&store, "token-to-clear").unwrap();

        clear_verification_token_with_store(&store).unwrap();

        assert_eq!(get_verification_token_with_store(&store).unwrap(), "");
        assert!(!has_verification_token_with_store(&store).unwrap());
    }

    #[test]
    fn set_verification_token_rejects_empty_value() {
        let store = MockKeychainStore::new();
        let result = set_verification_token_with_store(&store, "   ");
        assert_eq!(result, Err("Verification token cannot be empty".into()));
    }

    #[test]
    fn normalizes_deepgram_key_wrappers() {
        assert_eq!(normalize_deepgram_api_key(" abc "), "abc");
        assert_eq!(normalize_deepgram_api_key("Token abc"), "abc");
        assert_eq!(normalize_deepgram_api_key("Bearer abc"), "abc");
        assert_eq!(
            normalize_deepgram_api_key("Authorization: Token abc"),
            "abc"
        );
        assert_eq!(normalize_deepgram_api_key("\"abc\""), "abc");
    }

    #[test]
    fn has_deepgram_api_key_returns_false_when_not_set() {
        let store = MockKeychainStore::new();
        let result = has_deepgram_api_key_with_store(&store);
        assert!(!result.unwrap());
    }

    #[test]
    fn has_deepgram_api_key_returns_true_when_set() {
        let store = MockKeychainStore::new();
        store.set_password("deepgram_api_key", "test-key").unwrap();
        let result = has_deepgram_api_key_with_store(&store);
        assert!(result.unwrap());
    }

    #[test]
    fn has_deepgram_api_key_returns_false_when_empty() {
        let store = MockKeychainStore::new();
        store.set_password("deepgram_api_key", "   ").unwrap();
        let result = has_deepgram_api_key_with_store(&store);
        assert!(!result.unwrap());
    }

    #[test]
    fn set_deepgram_api_key_saves_and_reads_back() {
        let store = MockKeychainStore::new();
        set_deepgram_api_key_with_store(&store, "my-api-key").unwrap();
        let stored = store.get_password("deepgram_api_key").unwrap();
        assert_eq!(stored, "my-api-key");
    }

    #[test]
    fn set_deepgram_api_key_normalizes_input() {
        let store = MockKeychainStore::new();
        set_deepgram_api_key_with_store(&store, "  Token abc  ").unwrap();
        let stored = store.get_password("deepgram_api_key").unwrap();
        assert_eq!(stored, "abc");
    }

    #[test]
    fn set_deepgram_api_key_rejects_empty() {
        let store = MockKeychainStore::new();
        let result = set_deepgram_api_key_with_store(&store, "");
        assert_eq!(result, Err("API key cannot be empty".into()));
    }

    #[test]
    fn set_deepgram_api_key_rejects_whitespace() {
        let store = MockKeychainStore::new();
        let result = set_deepgram_api_key_with_store(&store, "   ");
        assert_eq!(result, Err("API key cannot be empty".into()));
    }

    #[test]
    fn get_deepgram_api_key_or_empty_returns_empty_when_not_set() {
        let store = MockKeychainStore::new();
        let result = get_deepgram_api_key_or_empty_with_store(&store).unwrap();
        assert_eq!(result, "");
    }

    #[test]
    fn get_deepgram_api_key_or_empty_returns_stored_value() {
        let store = MockKeychainStore::new();
        store.set_password("deepgram_api_key", "my-key").unwrap();
        let result = get_deepgram_api_key_or_empty_with_store(&store).unwrap();
        assert_eq!(result, "my-key");
    }

    #[test]
    fn get_deepgram_api_key_or_empty_normalizes_output() {
        let store = MockKeychainStore::new();
        store
            .set_password("deepgram_api_key", "  Token abc  ")
            .unwrap();
        let result = get_deepgram_api_key_or_empty_with_store(&store).unwrap();
        assert_eq!(result, "abc");
    }

    #[test]
    fn speechmatics_key_round_trips_through_secure_store() {
        let store = MockKeychainStore::new();
        set_speechmatics_api_key_with_store(&store, "  Bearer speech-key  ").unwrap();

        assert!(has_speechmatics_api_key_with_store(&store).unwrap());
        assert_eq!(
            get_speechmatics_api_key_or_empty_with_store(&store).unwrap(),
            "speech-key"
        );
    }

    #[test]
    fn speechmatics_key_rejects_empty_values() {
        let store = MockKeychainStore::new();
        assert_eq!(
            set_speechmatics_api_key_with_store(&store, "   "),
            Err("API key cannot be empty".into())
        );
        assert!(!has_speechmatics_api_key_with_store(&store).unwrap());
    }

    #[test]
    fn deepseek_key_round_trips_through_secure_store() {
        let store = MockKeychainStore::new();
        set_deepseek_api_key_with_store(&store, "  Bearer sk-deep-123  ").unwrap();

        assert!(has_deepseek_api_key_with_store(&store).unwrap());
        assert_eq!(
            get_deepseek_api_key_or_empty_with_store(&store).unwrap(),
            "sk-deep-123"
        );
    }

    #[test]
    fn deepseek_key_rejects_empty_values() {
        let store = MockKeychainStore::new();
        assert_eq!(
            set_deepseek_api_key_with_store(&store, "   "),
            Err("API key cannot be empty".into())
        );
        assert!(!has_deepseek_api_key_with_store(&store).unwrap());
    }

    #[test]
    fn deepseek_key_or_empty_returns_empty_when_missing() {
        let store = MockKeychainStore::new();
        assert_eq!(
            get_deepseek_api_key_or_empty_with_store(&store).unwrap(),
            ""
        );
    }

    #[test]
    fn cerebras_key_round_trips_through_secure_store() {
        let store = MockKeychainStore::new();
        set_cerebras_api_key_with_store(&store, "  Bearer csk-test-123  ").unwrap();

        assert!(has_cerebras_api_key_with_store(&store).unwrap());
        assert_eq!(
            get_cerebras_api_key_or_empty_with_store(&store).unwrap(),
            "csk-test-123"
        );
    }

    #[test]
    fn cerebras_key_rejects_empty_values() {
        let store = MockKeychainStore::new();
        assert_eq!(
            set_cerebras_api_key_with_store(&store, "   "),
            Err("API key cannot be empty".into())
        );
        assert!(!has_cerebras_api_key_with_store(&store).unwrap());
    }

    #[test]
    fn cerebras_key_or_empty_returns_empty_when_missing() {
        let store = MockKeychainStore::new();
        assert_eq!(
            get_cerebras_api_key_or_empty_with_store(&store).unwrap(),
            ""
        );
    }

    #[test]
    fn normalizes_cerebras_key_wrappers() {
        assert_eq!(normalize_cerebras_api_key(" csk-123 "), "csk-123");
        assert_eq!(normalize_cerebras_api_key("Bearer csk-123"), "csk-123");
        assert_eq!(normalize_cerebras_api_key("\"csk-123\""), "csk-123");
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn keyring_uses_native_windows_persistence() {
        assert!(matches!(
            keyring::default::default_credential_builder().persistence(),
            keyring::credential::CredentialPersistence::UntilDelete
        ));
    }
}
