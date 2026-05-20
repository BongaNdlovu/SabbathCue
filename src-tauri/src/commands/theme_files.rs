use std::path::Path;

use base64::Engine as _;
use tauri::command;

const MAX_THEME_BYTES: u64 = 1_000_000; // 1 MB
const MAX_IMAGE_BYTES: u64 = 10_000_000; // 10 MB
const MAX_JSON_DEPTH: usize = 32;
const MAX_JSON_NODES: usize = 20_000;

fn file_size(path: &Path) -> Result<u64, String> {
    std::fs::metadata(path)
        .map_err(|e| format!("Could not read file metadata: {e}"))?
        .len()
        .try_into()
        .map_err(|_| "Invalid file size".to_string())
}

fn enforce_json_limits(value: &serde_json::Value) -> Result<(), String> {
    fn walk(v: &serde_json::Value, depth: usize, nodes: &mut usize) -> Result<(), String> {
        *nodes += 1;
        if *nodes > MAX_JSON_NODES {
            return Err("Theme JSON is too large/complex".into());
        }
        if depth > MAX_JSON_DEPTH {
            return Err("Theme JSON is too deeply nested".into());
        }
        match v {
            serde_json::Value::Array(arr) => {
                for item in arr {
                    walk(item, depth + 1, nodes)?;
                }
            }
            serde_json::Value::Object(map) => {
                for (_k, val) in map {
                    walk(val, depth + 1, nodes)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    let mut nodes = 0usize;
    walk(value, 0, &mut nodes)
}

fn validate_theme_shape(value: &serde_json::Value) -> Result<(), String> {
    let obj = value.as_object().ok_or_else(|| "Theme JSON must be an object".to_string())?;

    for key in ["id", "name", "background", "layout", "resolution"] {
        if !obj.contains_key(key) {
            return Err(format!("Invalid theme: missing required field '{key}'"));
        }
    }

    Ok(())
}

fn theme_from_bytes(bytes: &[u8]) -> Result<serde_json::Value, String> {
    let value: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|e| format!("Invalid JSON: {e}"))?;
    enforce_json_limits(&value)?;
    validate_theme_shape(&value)?;
    Ok(value)
}

#[command]
pub fn import_theme_from_path(path: String) -> Result<serde_json::Value, String> {
    let p = Path::new(&path);
    let size = file_size(p)?;
    if size > MAX_THEME_BYTES {
        return Err(format!(
            "Theme file is too large ({size} bytes). Max is {MAX_THEME_BYTES} bytes."
        ));
    }
    let bytes = std::fs::read(p).map_err(|e| format!("Could not read theme file: {e}"))?;
    theme_from_bytes(&bytes)
}

#[command]
pub fn export_theme_to_path(path: String, theme: serde_json::Value) -> Result<(), String> {
    enforce_json_limits(&theme)?;
    validate_theme_shape(&theme)?;

    let json =
        serde_json::to_string_pretty(&theme).map_err(|e| format!("Could not serialize theme: {e}"))?;
    if json.len() as u64 > MAX_THEME_BYTES {
        return Err("Theme JSON is too large to export".into());
    }
    std::fs::write(Path::new(&path), json).map_err(|e| format!("Could not write theme file: {e}"))
}

fn image_mime_for_extension(ext: &str) -> Option<&'static str> {
    match ext {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "webp" => Some("image/webp"),
        "gif" => Some("image/gif"),
        "bmp" => Some("image/bmp"),
        // Intentionally no SVG for now.
        _ => None,
    }
}

#[command]
pub fn read_image_as_data_url(path: String) -> Result<String, String> {
    let p = Path::new(&path);
    let size = file_size(p)?;
    if size > MAX_IMAGE_BYTES {
        return Err(format!(
            "Image file is too large ({size} bytes). Max is {MAX_IMAGE_BYTES} bytes."
        ));
    }

    let ext = p
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    let mime = image_mime_for_extension(&ext).ok_or_else(|| {
        "Unsupported image format. Allowed: PNG, JPEG, WebP, GIF, BMP.".to_string()
    })?;

    let bytes = std::fs::read(p).map_err(|e| format!("Could not read image: {e}"))?;
    let b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
    Ok(format!("data:{mime};base64,{b64}"))
}

