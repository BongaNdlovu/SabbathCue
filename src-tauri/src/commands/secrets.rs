use base64::Engine as _;
use rand::rngs::OsRng;
use rand::RngCore;
use tauri::command;

const SERVICE_NAME: &str = "sabbathcue";

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
    match entry("remote_http_token").get_password() {
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
    let token = generate_token();
    entry("remote_http_token")
        .set_password(&token)
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

