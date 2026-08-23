use std::sync::{Arc, Mutex};

use tauri::{AppHandle, Emitter, State};

use rhema_api::{
    new_shared_status, start_http_server, start_osc_listener, update_shared_http_token,
    CommandError, CommandSink, HttpConfig, HttpHandle, OscConfig, OscHandle, SharedHttpToken,
    SharedStatus,
};

use super::validation::{
    bounded_text, valid_confidence_threshold, valid_port, MAX_QUEUE_LENGTH, MAX_STATUS_TEXT_BYTES,
};
use crate::commands::secrets;

/// Tauri-aware implementation of `CommandSink`.
///
/// Routes frontend-bound commands as Tauri events (`app.emit()`)
/// and backend-bound commands as Tauri command invocations.
struct TauriSink {
    app: AppHandle,
}

fn map_backend_remote_action(action: &str) -> Result<(&'static str, bool), CommandError> {
    match action {
        "show_broadcast" => Ok(("remote:show", false)),
        "hide_broadcast" => Ok(("remote:hide", false)),
        "set_confidence" => Ok(("remote:confidence", true)),
        _ => Err(CommandError::DispatchFailed(format!(
            "Unknown backend action: {action}"
        ))),
    }
}

impl CommandSink for TauriSink {
    fn emit_event(&self, event: &str, payload: &str) -> Result<(), CommandError> {
        self.app
            .emit(event, payload.to_string())
            .map_err(|e| CommandError::DispatchFailed(format!("Tauri emit failed: {e}")))
    }

    fn invoke_backend(&self, action: &str, args: &str) -> Result<(), CommandError> {
        let (event, uses_args) = map_backend_remote_action(action)?;
        if action == "show_broadcast" {
            log::info!("Remote control: show broadcast");
        } else if action == "hide_broadcast" {
            log::info!("Remote control: hide broadcast");
        }
        let payload = if uses_args {
            args.to_string()
        } else {
            "{}".to_string()
        };
        self.app
            .emit(event, payload)
            .map_err(|e| CommandError::DispatchFailed(e.to_string()))
    }
}

// --- OSC Runtime ---

/// Managed state for the OSC runtime.
pub struct OscRuntime {
    handle: Option<OscHandle>,
    bound_port: Option<u16>,
}

impl OscRuntime {
    pub fn new() -> Self {
        Self {
            handle: None,
            bound_port: None,
        }
    }
}

impl Default for OscRuntime {
    fn default() -> Self {
        Self::new()
    }
}

/// Start the OSC listener on the given port.
#[tauri::command]
pub async fn start_osc(
    app: AppHandle,
    state: State<'_, Mutex<OscRuntime>>,
    port: Option<u16>,
) -> Result<u16, String> {
    let mut runtime = state.lock().map_err(|e| e.to_string())?;

    if runtime.handle.is_some() {
        return Err("OSC listener is already running".into());
    }

    // Source the shared remote-control token (same secret as the HTTP API)
    // so the OSC surface authenticates like HTTP does.
    let token = (|| -> Result<String, String> {
        secrets::ensure_remote_http_token_exists()?;
        secrets::get_remote_http_token()
    })()?;

    let config = OscConfig {
        port: valid_port(port, 8000)?,
        host: "127.0.0.1".into(),
        token: Some(token),
    };

    let sink = Arc::new(TauriSink { app });
    let result = start_osc_listener(config, sink).map_err(|e| e.to_string())?;

    let bound_port = result.bound_port;
    runtime.handle = Some(result.handle);
    runtime.bound_port = Some(bound_port);

    log::info!("OSC listener started on port {bound_port}");
    Ok(bound_port)
}

/// Stop the OSC listener.
#[tauri::command]
pub async fn stop_osc(state: State<'_, Mutex<OscRuntime>>) -> Result<(), String> {
    let mut handle = {
        let mut runtime = state.lock().map_err(|e| e.to_string())?;
        match runtime.handle.take() {
            Some(handle) => {
                runtime.bound_port = None;
                handle
            }
            None => return Err("OSC listener is not running".into()),
        }
    };

    // Join the listener thread off the async worker: stop() blocks up to
    // one read-timeout (100 ms), which must not stall unrelated tasks.
    tauri::async_runtime::spawn_blocking(move || handle.stop())
        .await
        .map_err(|e| e.to_string())?;

    log::info!("OSC listener stopped");
    Ok(())
}

/// Get the current OSC listener status.
#[tauri::command]
pub async fn get_osc_status(state: State<'_, Mutex<OscRuntime>>) -> Result<OscStatus, String> {
    let runtime = state.lock().map_err(|e| e.to_string())?;

    Ok(OscStatus {
        running: runtime
            .handle
            .as_ref()
            .is_some_and(rhema_api::OscHandle::is_active),
        port: runtime.bound_port,
    })
}

#[derive(serde::Serialize)]
pub struct OscStatus {
    pub running: bool,
    pub port: Option<u16>,
}

// --- HTTP Runtime ---

/// Managed state for the HTTP API runtime.
pub struct HttpRuntime {
    handle: Option<HttpHandle>,
    bound_port: Option<u16>,
    status: SharedStatus,
    token: Option<SharedHttpToken>,
    starting: bool,
}

impl HttpRuntime {
    pub fn new() -> Self {
        Self {
            handle: None,
            bound_port: None,
            status: new_shared_status(),
            token: None,
            starting: false,
        }
    }

    pub fn shared_status(&self) -> SharedStatus {
        self.status.clone()
    }
}

impl Default for HttpRuntime {
    fn default() -> Self {
        Self::new()
    }
}

/// Start the HTTP API server on the given port.
#[tauri::command]
pub async fn start_http(
    app: AppHandle,
    state: State<'_, Mutex<HttpRuntime>>,
    port: Option<u16>,
) -> Result<u16, String> {
    let port = valid_port(port, 8080)?;
    let status = {
        let mut runtime = state.lock().map_err(|e| e.to_string())?;
        if runtime.handle.is_some() || runtime.starting {
            return Err("HTTP API server is already running".into());
        }
        runtime.starting = true;
        runtime.shared_status()
    };

    let token = match (|| -> Result<String, String> {
        let _created = secrets::ensure_remote_http_token_exists()?;
        secrets::get_remote_http_token()
    })() {
        Ok(token) => token,
        Err(error) => {
            let mut runtime = state.lock().map_err(|e| e.to_string())?;
            runtime.starting = false;
            return Err(error);
        }
    };

    let config = HttpConfig {
        port,
        host: "127.0.0.1".into(),
        token,
    };

    let sink = Arc::new(TauriSink { app });
    let result = match start_http_server(config, sink, status).await {
        Ok(result) => result,
        Err(error) => {
            let mut runtime = state.lock().map_err(|e| e.to_string())?;
            runtime.starting = false;
            return Err(error.to_string());
        }
    };

    let bound_port = result.bound_port;

    {
        let mut runtime = state.lock().map_err(|e| e.to_string())?;
        runtime.handle = Some(result.handle);
        runtime.bound_port = Some(bound_port);
        runtime.token = Some(result.token);
        runtime.starting = false;
    }

    log::info!("HTTP API server started on port {bound_port}");
    Ok(bound_port)
}

/// Stop the HTTP API server.
#[tauri::command]
pub async fn stop_http(state: State<'_, Mutex<HttpRuntime>>) -> Result<(), String> {
    let mut runtime = state.lock().map_err(|e| e.to_string())?;

    match runtime.handle.take() {
        Some(mut handle) => {
            handle.stop();
            runtime.bound_port = None;
            runtime.token = None;
            log::info!("HTTP API server stopped");
            Ok(())
        }
        None => Err("HTTP API server is not running".into()),
    }
}

/// Rotate the remote HTTP token and apply it to a running HTTP server.
#[tauri::command]
pub async fn rotate_remote_http_token(
    state: State<'_, Mutex<HttpRuntime>>,
) -> Result<String, String> {
    let token = secrets::rotate_remote_http_token()?;
    let shared_token = {
        let runtime = state.lock().map_err(|e| e.to_string())?;
        runtime.token.clone()
    };

    if let Some(shared_token) = shared_token {
        update_shared_http_token(&shared_token, token.clone()).await;
    }

    Ok(token)
}

/// Get the current HTTP API server status.
#[tauri::command]
pub async fn get_http_status(state: State<'_, Mutex<HttpRuntime>>) -> Result<HttpStatus, String> {
    let runtime = state.lock().map_err(|e| e.to_string())?;

    Ok(HttpStatus {
        running: runtime
            .handle
            .as_ref()
            .is_some_and(rhema_api::HttpHandle::is_active),
        port: runtime.bound_port,
    })
}

/// Update the status snapshot from the frontend.
#[tauri::command]
pub async fn update_remote_status(
    state: State<'_, Mutex<HttpRuntime>>,
    on_air: Option<bool>,
    active_theme: Option<String>,
    live_verse: Option<String>,
    queue_length: Option<usize>,
    confidence_threshold: Option<f32>,
) -> Result<(), String> {
    let status = {
        let runtime = state.lock().map_err(|e| e.to_string())?;
        runtime.shared_status()
    };

    let mut snapshot = status.write().await;
    if let Some(v) = on_air {
        snapshot.on_air = v;
    }
    if let Some(v) = active_theme {
        bounded_text(&v, "active_theme", MAX_STATUS_TEXT_BYTES)?;
        snapshot.active_theme = Some(v);
    }
    if let Some(v) = live_verse {
        bounded_text(&v, "live_verse", MAX_STATUS_TEXT_BYTES)?;
        snapshot.live_verse = Some(v);
    }
    if let Some(v) = queue_length {
        if v > MAX_QUEUE_LENGTH {
            return Err(format!("queue_length must be <= {MAX_QUEUE_LENGTH}"));
        }
        snapshot.queue_length = v;
    }
    if let Some(v) = confidence_threshold {
        snapshot.confidence_threshold = valid_confidence_threshold(v)?;
    }

    Ok(())
}

#[derive(serde::Serialize)]
pub struct HttpStatus {
    pub running: bool,
    pub port: Option<u16>,
}

#[cfg(test)]
mod tests {
    use super::map_backend_remote_action;
    use rhema_api::CommandError;

    #[test]
    fn maps_show_and_hide_actions() {
        assert_eq!(
            map_backend_remote_action("show_broadcast").unwrap(),
            ("remote:show", false)
        );
        assert_eq!(
            map_backend_remote_action("hide_broadcast").unwrap(),
            ("remote:hide", false)
        );
    }

    #[test]
    fn maps_confidence_action_with_args() {
        assert_eq!(
            map_backend_remote_action("set_confidence").unwrap(),
            ("remote:confidence", true)
        );
    }

    #[test]
    fn rejects_unknown_backend_action() {
        let err = map_backend_remote_action("toggle_lasers").unwrap_err();
        assert!(
            matches!(err, CommandError::DispatchFailed(message) if message.contains("Unknown backend action"))
        );
    }
}
