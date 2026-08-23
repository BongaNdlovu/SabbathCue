#![expect(
    clippy::needless_pass_by_value,
    reason = "Tauri command extractors require pass-by-value"
)]

use std::sync::Mutex;
use std::time::Instant;

use rhema_broadcast::ndi::{NdiRuntime, NdiSessionInfo, NdiStartRequest};
use serde::Serialize;
use tauri::ipc::{InvokeBody, Request};
use tauri::State;
use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};

const NDI_FRAME_OUTPUT_ID_HEADER: &str = "x-sabbathcue-output-id";
const NDI_FRAME_WIDTH_HEADER: &str = "x-sabbathcue-width";
const NDI_FRAME_HEIGHT_HEADER: &str = "x-sabbathcue-height";

fn apply_projector_geometry(
    window: &tauri::WebviewWindow,
    pos: tauri::PhysicalPosition<i32>,
    size: tauri::PhysicalSize<u32>,
    fullscreen_enabled: bool,
) -> Result<(), String> {
    let _ = window.set_fullscreen(false);
    window
        .set_position(tauri::Position::Physical(pos))
        .map_err(|e| e.to_string())?;
    window
        .set_size(tauri::Size::Physical(size))
        .map_err(|e| e.to_string())?;
    window
        .set_decorations(!fullscreen_enabled)
        .map_err(|e| e.to_string())?;
    window.show().map_err(|e| e.to_string())?;
    window.set_focus().map_err(|e| e.to_string())?;
    if fullscreen_enabled {
        window.set_fullscreen(true).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Map `output_id` ("main" | "alt") to Tauri window label.
fn window_label(output_id: &str) -> &'static str {
    match output_id {
        "alt" => "broadcast-alt",
        _ => "broadcast",
    }
}

/// Map `output_id` to the user-visible projector window title.
fn projector_window_title(output_id: &str) -> &'static str {
    match output_id {
        "alt" => "Projector - Alt",
        _ => "Projector - Program",
    }
}

/// Map `output_id` to broadcast-output.html URL with query param.
fn window_url(output_id: &str) -> String {
    format!("broadcast-output.html?output={output_id}")
}

#[derive(Serialize)]
pub struct MonitorInfo {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub x: i32,
    pub y: i32,
    pub key: String,
}

pub fn build_monitor_key(name: &str, width: u32, height: u32, x: i32, y: i32) -> String {
    let normalized_name = name.trim().to_lowercase();
    format!("{normalized_name}|{width}x{height}|{x},{y}")
}

fn resolve_monitor_index(
    monitor_keys: &[String],
    monitor_key: Option<&str>,
    fallback_index: usize,
) -> Option<usize> {
    if let Some(key) = monitor_key.filter(|key| !key.trim().is_empty()) {
        if let Some(index) = monitor_keys.iter().position(|candidate| candidate == key) {
            return Some(index);
        }
    }

    monitor_keys.get(fallback_index).map(|_| fallback_index)
}

struct NdiRawFrame<'a> {
    output_id: String,
    width: u32,
    height: u32,
    rgba_data: &'a [u8],
}

// Broadcast window/monitor commands are async so they run on the async
// runtime instead of the main thread. As synchronous commands they executed
// on the UI event loop, so monitor enumeration and projector window creation
// blocked the whole app (and each other), making "refresh monitors" and
// HDMI connection feel stalled.
#[tauri::command]
pub async fn list_monitors(app: tauri::AppHandle) -> Result<Vec<MonitorInfo>, String> {
    let started_at = Instant::now();
    let monitors = app.available_monitors().map_err(|e| e.to_string())?;
    let mut result: Vec<MonitorInfo> = monitors
        .iter()
        .map(|m| {
            let size = m.size();
            let pos = m.position();
            let name = m.name().cloned().unwrap_or_else(|| "Unknown".to_string());
            MonitorInfo {
                key: build_monitor_key(&name, size.width, size.height, pos.x, pos.y),
                name,
                width: size.width,
                height: size.height,
                x: pos.x,
                y: pos.y,
            }
        })
        .collect();

    if result.is_empty() {
        log::warn!(
            "[BROADCAST] available_monitors returned no displays; falling back to synthetic primary display"
        );
        result.push(MonitorInfo {
            key: build_monitor_key("Primary Display", 1920, 1080, 0, 0),
            name: "Primary Display".to_string(),
            width: 1920,
            height: 1080,
            x: 0,
            y: 0,
        });
    }

    log::info!(
        "[BROADCAST] list_monitors count={} elapsed_ms={} monitors={}",
        result.len(),
        started_at.elapsed().as_millis(),
        result
            .iter()
            .map(|monitor| format!(
                "{} {}x{} @ {},{} key={}",
                monitor.name, monitor.width, monitor.height, monitor.x, monitor.y, monitor.key
            ))
            .collect::<Vec<_>>()
            .join("; ")
    );

    Ok(result)
}

/// Ensure the broadcast window for a given output exists (creates hidden if not).
#[tauri::command]
pub async fn ensure_broadcast_window(
    app: tauri::AppHandle,
    output_id: String,
) -> Result<(), String> {
    let label = window_label(&output_id);
    if app.get_webview_window(label).is_some() {
        return Ok(());
    }
    WebviewWindowBuilder::new(&app, label, WebviewUrl::App(window_url(&output_id).into()))
        .title(if output_id == "alt" {
            "SabbathCue NDI Alt"
        } else {
            "SabbathCue NDI"
        })
        .inner_size(1920.0, 1080.0)
        .visible(false)
        .skip_taskbar(true)
        .focused(false)
        .build()
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn open_broadcast_window(
    app: tauri::AppHandle,
    output_id: String,
    monitor_index: usize,
    monitor_key: Option<String>,
    fullscreen: Option<bool>,
) -> Result<(), String> {
    let started_at = Instant::now();
    let label = window_label(&output_id);
    let monitors = app.available_monitors().map_err(|e| e.to_string())?;
    let monitor_keys = monitors
        .iter()
        .map(|monitor| {
            let size = monitor.size();
            let pos = monitor.position();
            let name = monitor
                .name()
                .cloned()
                .unwrap_or_else(|| "Unknown".to_string());
            build_monitor_key(&name, size.width, size.height, pos.x, pos.y)
        })
        .collect::<Vec<_>>();
    let resolved_monitor_index =
        resolve_monitor_index(&monitor_keys, monitor_key.as_deref(), monitor_index)
            .ok_or_else(|| format!("Monitor index {monitor_index} out of range"))?;
    let monitor = monitors
        .get(resolved_monitor_index)
        .ok_or_else(|| format!("Monitor index {monitor_index} out of range"))?;

    let pos = monitor.position();
    let size = monitor.size();
    let fullscreen_enabled = fullscreen.unwrap_or(false);
    log::info!(
        "[BROADCAST] open_window output={} requested_index={} requested_key={:?} resolved_index={} target={} {}x{} @ {},{} fullscreen={} monitor_count={}",
        output_id,
        monitor_index,
        monitor_key,
        resolved_monitor_index,
        monitor.name().cloned().unwrap_or_else(|| "Unknown".to_string()),
        size.width,
        size.height,
        pos.x,
        pos.y,
        fullscreen_enabled,
        monitors.len()
    );

    // If window already exists (e.g. hidden for NDI), reuse it
    if let Some(window) = app.get_webview_window(label) {
        window
            .set_title(projector_window_title(&output_id))
            .map_err(|e| e.to_string())?;
        window.set_skip_taskbar(false).map_err(|e| e.to_string())?;
        apply_projector_geometry(&window, *pos, *size, fullscreen_enabled)?;
        log::info!(
            "[BROADCAST] reused_window output={} label={} elapsed_ms={}",
            output_id,
            label,
            started_at.elapsed().as_millis()
        );
        return Ok(());
    }

    let window =
        WebviewWindowBuilder::new(&app, label, WebviewUrl::App(window_url(&output_id).into()))
            .title(projector_window_title(&output_id))
            .position(f64::from(pos.x), f64::from(pos.y))
            .inner_size(f64::from(size.width), f64::from(size.height))
            .decorations(!fullscreen_enabled)
            .fullscreen(false)
            .visible(false)
            .always_on_top(false)
            .skip_taskbar(false)
            .focused(true)
            .build()
            .map_err(|e| e.to_string())?;

    apply_projector_geometry(&window, *pos, *size, fullscreen_enabled)?;
    log::info!(
        "[BROADCAST] created_window output={} label={} elapsed_ms={}",
        output_id,
        label,
        started_at.elapsed().as_millis()
    );
    Ok(())
}

#[tauri::command]
pub async fn close_broadcast_window(
    app: tauri::AppHandle,
    output_id: String,
    runtime: State<'_, Mutex<NdiRuntime>>,
) -> Result<(), String> {
    let started_at = Instant::now();
    let label = window_label(&output_id);
    if let Some(window) = app.get_webview_window(label) {
        let ndi_active = runtime
            .lock()
            .map_err(|e| e.to_string())?
            .is_active(&output_id);
        if ndi_active {
            window.hide().map_err(|e| e.to_string())?;
            log::info!(
                "[BROADCAST] hid_window output={} label={} ndi_active=true elapsed_ms={}",
                output_id,
                label,
                started_at.elapsed().as_millis()
            );
        } else {
            window.close().map_err(|e| e.to_string())?;
            log::info!(
                "[BROADCAST] closed_window output={} label={} elapsed_ms={}",
                output_id,
                label,
                started_at.elapsed().as_millis()
            );
        }
    } else {
        log::info!(
            "[BROADCAST] close_window_noop output={} label={} elapsed_ms={}",
            output_id,
            label,
            started_at.elapsed().as_millis()
        );
    }
    Ok(())
}

fn close_identify_overlays(overlays: &mut Vec<tauri::WebviewWindow>) {
    for window in overlays.drain(..) {
        let _ = window.close();
    }
}

/// Briefly flash a large number on every connected display so the operator can
/// tell which physical screen is which ("screen 2 is the projector"). Each
/// overlay window auto-closes after `duration_ms`.
#[tauri::command]
pub async fn flash_monitor_labels(
    app: tauri::AppHandle,
    duration_ms: Option<u64>,
) -> Result<(), String> {
    let duration = duration_ms.unwrap_or(4000).clamp(500, 15_000);
    let monitors = app.available_monitors().map_err(|e| e.to_string())?;

    let mut overlays: Vec<tauri::WebviewWindow> = Vec::new();
    for (index, monitor) in monitors.iter().enumerate() {
        let pos = monitor.position();
        let size = monitor.size();
        let label = format!("identify-{index}");

        if let Some(existing) = app.get_webview_window(&label) {
            let _ = existing.close();
        }

        let url = format!(
            "identify.html?n={}&w={}&h={}",
            index + 1,
            size.width,
            size.height
        );
        let window = WebviewWindowBuilder::new(&app, &label, WebviewUrl::App(url.into()))
            .title("Identify display")
            .position(f64::from(pos.x), f64::from(pos.y))
            .inner_size(f64::from(size.width), f64::from(size.height))
            .decorations(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .focused(false)
            .visible(false)
            .build()
            .map_err(|e| e.to_string());

        let window = match window {
            Ok(window) => window,
            Err(error) => {
                close_identify_overlays(&mut overlays);
                return Err(error);
            }
        };

        if let Err(error) = window
            .set_position(tauri::Position::Physical(*pos))
            .map_err(|e| e.to_string())
        {
            let _ = window.close();
            close_identify_overlays(&mut overlays);
            return Err(error);
        }
        if let Err(error) = window
            .set_size(tauri::Size::Physical(*size))
            .map_err(|e| e.to_string())
        {
            let _ = window.close();
            close_identify_overlays(&mut overlays);
            return Err(error);
        }
        if let Err(error) = window.show().map_err(|e| e.to_string()) {
            let _ = window.close();
            close_identify_overlays(&mut overlays);
            return Err(error);
        }
        overlays.push(window);
    }

    log::info!(
        "[BROADCAST] flash_monitor_labels count={} duration_ms={}",
        overlays.len(),
        duration
    );

    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(duration));
        for window in overlays {
            let _ = window.close();
        }
    });

    Ok(())
}

#[tauri::command]
pub fn start_ndi(
    output_id: String,
    runtime: State<'_, Mutex<NdiRuntime>>,
    request: NdiStartRequest,
) -> Result<NdiSessionInfo, String> {
    let mut runtime = runtime.lock().map_err(|e| e.to_string())?;
    runtime.start(output_id, request).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn stop_ndi(output_id: String, runtime: State<'_, Mutex<NdiRuntime>>) -> Result<(), String> {
    let mut runtime = runtime.lock().map_err(|e| e.to_string())?;
    runtime.stop(&output_id);
    Ok(())
}

#[derive(Serialize)]
pub struct NdiStatusResponse {
    pub active: bool,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
}

#[tauri::command]
pub fn get_ndi_status(
    output_id: String,
    runtime: State<'_, Mutex<NdiRuntime>>,
) -> Result<Option<NdiStatusResponse>, String> {
    let runtime = runtime.lock().map_err(|e| e.to_string())?;
    match runtime.current_info(&output_id) {
        Some(info) => Ok(Some(NdiStatusResponse {
            active: true,
            width: info.width,
            height: info.height,
            fps: info.fps,
        })),
        None => Ok(None),
    }
}

#[tauri::command]
pub fn push_ndi_frame(
    runtime: State<'_, Mutex<NdiRuntime>>,
    request: Request<'_>,
) -> Result<(), String> {
    let frame = ndi_raw_frame_from_request(&request)?;
    // Take only a session handle and release the runtime-wide lock before
    // the frame conversion + FFI submit: start/stop/status and the other
    // output's sends no longer queue behind this frame.
    let session = {
        let runtime = runtime.lock().map_err(|e| e.to_string())?;
        runtime.session_handle(&frame.output_id)
    };
    let Some(session) = session else {
        return Err("NDI session is not active".to_string());
    };
    session
        .send_frame_rgba(frame.width, frame.height, frame.rgba_data)
        .map_err(|e| e.to_string())
}

fn ndi_raw_frame_from_request<'a>(request: &'a Request<'_>) -> Result<NdiRawFrame<'a>, String> {
    let output_id = ndi_frame_header(request, NDI_FRAME_OUTPUT_ID_HEADER)?.to_string();
    if output_id.trim().is_empty() {
        return Err(format!(
            "{NDI_FRAME_OUTPUT_ID_HEADER} header cannot be empty"
        ));
    }

    let width = ndi_frame_u32_header(request, NDI_FRAME_WIDTH_HEADER)?;
    let height = ndi_frame_u32_header(request, NDI_FRAME_HEIGHT_HEADER)?;
    let rgba_data = match request.body() {
        InvokeBody::Raw(bytes) => bytes.as_slice(),
        InvokeBody::Json(_) => return Err("push_ndi_frame expects a raw binary body".to_string()),
    };

    validate_ndi_frame_payload(width, height, rgba_data)?;

    Ok(NdiRawFrame {
        output_id,
        width,
        height,
        rgba_data,
    })
}

fn ndi_frame_header<'a>(request: &'a Request<'_>, name: &str) -> Result<&'a str, String> {
    request
        .headers()
        .get(name)
        .ok_or_else(|| format!("Missing {name} header"))?
        .to_str()
        .map_err(|e| format!("Invalid {name} header: {e}"))
}

fn ndi_frame_u32_header(request: &Request<'_>, name: &str) -> Result<u32, String> {
    ndi_frame_header(request, name)?
        .parse::<u32>()
        .map_err(|e| format!("Invalid {name} header: {e}"))
}

fn validate_ndi_frame_payload(width: u32, height: u32, rgba_data: &[u8]) -> Result<(), String> {
    let expected = expected_ndi_frame_payload_len(width, height)?;
    if rgba_data.len() != expected {
        return Err(format!(
            "Invalid NDI frame byte length: expected {expected}, received {}",
            rgba_data.len()
        ));
    }
    Ok(())
}

fn expected_ndi_frame_payload_len(width: u32, height: u32) -> Result<usize, String> {
    if width == 0 || height == 0 {
        return Err(format!("Invalid NDI frame dimensions: {width}x{height}"));
    }

    let width = usize::try_from(width).map_err(|_| "NDI frame width is too large".to_string())?;
    let height =
        usize::try_from(height).map_err(|_| "NDI frame height is too large".to_string())?;
    width
        .checked_mul(height)
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| "NDI frame byte length overflow".to_string())
}

#[cfg(test)]
mod tests {
    use super::{
        build_monitor_key, expected_ndi_frame_payload_len, resolve_monitor_index,
        validate_ndi_frame_payload, window_label, window_url,
    };

    #[test]
    fn build_monitor_key_normalizes_name_and_includes_geometry() {
        assert_eq!(
            build_monitor_key("  HDMI-1  ", 1920, 1080, 100, 200),
            "hdmi-1|1920x1080|100,200"
        );
    }

    #[test]
    fn build_monitor_key_preserves_negative_extended_desktop_coordinates() {
        assert_eq!(
            build_monitor_key("HDMI Projector", 1280, 720, -1280, 0),
            "hdmi projector|1280x720|-1280,0"
        );
    }

    #[test]
    fn resolve_monitor_index_prefers_stable_hdmi_key_over_stale_index() {
        let monitor_keys = vec![
            "internal display|1920x1080|0,0".to_string(),
            "hdmi projector|1920x1080|1920,0".to_string(),
        ];

        assert_eq!(
            resolve_monitor_index(&monitor_keys, Some("hdmi projector|1920x1080|1920,0"), 0,),
            Some(1)
        );
    }

    #[test]
    fn resolve_monitor_index_falls_back_to_index_when_key_is_missing() {
        let monitor_keys = vec![
            "internal display|1920x1080|0,0".to_string(),
            "hdmi projector|1920x1080|1920,0".to_string(),
        ];

        assert_eq!(
            resolve_monitor_index(&monitor_keys, Some("missing"), 1),
            Some(1)
        );
    }

    #[test]
    fn resolve_monitor_index_rejects_out_of_range_fallback() {
        let monitor_keys = vec!["internal display|1920x1080|0,0".to_string()];

        assert_eq!(
            resolve_monitor_index(&monitor_keys, Some("missing"), 2),
            None
        );
    }

    #[test]
    fn window_label_maps_main_and_alt() {
        assert_eq!(window_label("main"), "broadcast");
        assert_eq!(window_label("alt"), "broadcast-alt");
        assert_eq!(window_label("unknown"), "broadcast");
    }

    #[test]
    fn window_url_includes_output_query_param() {
        assert_eq!(window_url("main"), "broadcast-output.html?output=main");
        assert_eq!(window_url("alt"), "broadcast-output.html?output=alt");
    }

    #[test]
    fn expected_ndi_frame_payload_len_counts_rgba_bytes() {
        assert_eq!(expected_ndi_frame_payload_len(2, 2), Ok(16));
    }

    #[test]
    fn validate_ndi_frame_payload_rejects_length_mismatch() {
        let err = validate_ndi_frame_payload(2, 2, &[0; 12]).unwrap_err();
        assert_eq!(
            err,
            "Invalid NDI frame byte length: expected 16, received 12"
        );
    }

    #[test]
    fn validate_ndi_frame_payload_rejects_zero_dimensions() {
        let err = validate_ndi_frame_payload(0, 2, &[]).unwrap_err();
        assert_eq!(err, "Invalid NDI frame dimensions: 0x2");
    }
}
