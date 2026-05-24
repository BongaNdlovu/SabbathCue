#![expect(
    clippy::needless_pass_by_value,
    reason = "Tauri command extractors require pass-by-value"
)]

use std::path::{Component, Path, PathBuf};

use serde::Serialize;
use tauri::AppHandle;

use crate::asset_paths;

const MAX_SLIDE_SIZE_BYTES: u64 = 100 * 1024 * 1024;
const MAX_MEDIA_SIZE_BYTES: u64 = 750 * 1024 * 1024;

const SUPPORTED_ATTACHMENT_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "webp", "gif", "pdf"];

/// Response DTO for `validate_service_attachment_path`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceAttachmentValidation {
    pub label: String,
    pub kind: String,
    pub size_bytes: u64,
}

#[derive(Debug, Serialize)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "frontend asset readiness DTO mirrors independent asset flags"
)]
pub struct AssetStatus {
    pub bible_db: bool,
    pub vosk_model: bool,
    pub vosk_worker: bool,
    pub onnx_model: bool,
    pub tokenizer: bool,
    pub embeddings: bool,
    pub embedding_ids: bool,
    pub semantic_ready: bool,
    pub ndi_sdk: bool,
}

#[tauri::command]
pub fn asset_status(app: AppHandle) -> AssetStatus {
    let bible_db = asset_paths::bible_db_path(&app).exists();
    let vosk_model = asset_paths::vosk_model_path(&app).exists();
    let vosk_worker = asset_paths::vosk_worker_path(&app).exists();
    let onnx_model = asset_paths::onnx_model_path(&app).exists();
    let tokenizer = asset_paths::tokenizer_path(&app).exists();
    let embeddings = asset_paths::embeddings_path(&app).exists();
    let embedding_ids = asset_paths::embedding_ids_path(&app).exists();
    let ndi_sdk = asset_paths::ndi_library_path(&app).exists();

    AssetStatus {
        bible_db,
        vosk_model,
        vosk_worker,
        onnx_model,
        tokenizer,
        embeddings,
        embedding_ids,
        semantic_ready: onnx_model && tokenizer && embeddings && embedding_ids,
        ndi_sdk,
    }
}

fn file_name_from_path(path: &str) -> String {
    Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .unwrap_or("Attachment")
        .to_string()
}

fn extension_from_path(path: &str) -> String {
    Path::new(path)
        .extension()
        .and_then(|ext| ext.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default()
}

fn attachment_kind_from_extension(extension: &str) -> &'static str {
    if extension == "pdf" {
        "deck"
    } else if matches!(extension, "png" | "jpg" | "jpeg" | "webp" | "gif") {
        "slide"
    } else {
        "document"
    }
}

fn has_url_scheme(path: &str) -> bool {
    let Some(colon_idx) = path.find(':') else {
        return false;
    };
    if colon_idx == 0 {
        return false;
    }
    let scheme = &path[..colon_idx];
    let rest = &path[colon_idx + 1..];
    // Windows drive letter (e.g. C:\ or C:/) is not a URL scheme.
    if scheme.len() == 1
        && scheme.chars().all(|ch| ch.is_ascii_alphabetic())
        && rest.starts_with(['\\', '/'])
    {
        return false;
    }
    let first = scheme.chars().next().unwrap_or_default();
    if !first.is_ascii_alphabetic() {
        return false;
    }
    scheme
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '+' | '-' | '.'))
}

fn is_network_path(path: &str) -> bool {
    path.starts_with("\\\\") || path.starts_with("//")
}

fn is_blocked_system_path(path: &str) -> bool {
    let normalized = path.replace('\\', "/").to_ascii_lowercase();
    if normalized.starts_with("/etc/")
        || normalized.starts_with("/bin/")
        || normalized.starts_with("/sbin/")
        || normalized.starts_with("/usr/")
        || normalized.starts_with("/var/")
        || normalized.starts_with("/system/")
        || normalized.starts_with("/library/")
    {
        return true;
    }

    let Some((drive, rest)) = normalized.split_once(":/") else {
        return false;
    };
    if drive.len() != 1 || !drive.chars().all(|ch| ch.is_ascii_alphabetic()) {
        return false;
    }
    rest.starts_with("windows/")
        || rest.starts_with("program files/")
        || rest.starts_with("program files (x86)/")
        || rest.starts_with("programdata/")
}

fn is_supported_extension(extension: &str) -> bool {
    SUPPORTED_ATTACHMENT_EXTENSIONS.contains(&extension)
}

fn path_contains_parent_traversal(path: &str) -> bool {
    Path::new(path)
        .components()
        .any(|component| matches!(component, Component::ParentDir))
}

fn max_size_for_kind(kind: &str) -> u64 {
    if kind == "media" {
        MAX_MEDIA_SIZE_BYTES
    } else {
        MAX_SLIDE_SIZE_BYTES
    }
}

fn validate_service_attachment_path_inner(
    path: &str,
) -> Result<ServiceAttachmentValidation, String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err("Attachment path is empty".to_string());
    }
    if has_url_scheme(trimmed) {
        return Err("URL paths are not allowed".to_string());
    }
    if is_network_path(trimmed) {
        return Err("Network paths are not allowed".to_string());
    }
    if path_contains_parent_traversal(trimmed) {
        return Err("Parent directory traversal is not allowed".to_string());
    }
    if is_blocked_system_path(trimmed) {
        return Err("System paths are not allowed".to_string());
    }

    let extension = extension_from_path(trimmed);
    if !is_supported_extension(&extension) {
        return Err("Unsupported attachment extension".to_string());
    }

    let canonical = PathBuf::from(trimmed)
        .canonicalize()
        .map_err(|_| "Attachment path does not exist".to_string())?;
    let metadata = canonical
        .metadata()
        .map_err(|_| "Unable to read attachment metadata".to_string())?;
    if !metadata.is_file() {
        return Err("Attachment path is not a file".to_string());
    }

    let kind = attachment_kind_from_extension(&extension);
    let size_bytes = metadata.len();
    if size_bytes > max_size_for_kind(kind) {
        return Err("Attachment exceeds size limit".to_string());
    }

    Ok(ServiceAttachmentValidation {
        label: file_name_from_path(trimmed),
        kind: kind.to_string(),
        size_bytes,
    })
}

/// Validates a local service-plan attachment path and returns display metadata.
#[tauri::command]
pub fn validate_service_attachment_path(
    path: String,
) -> Result<ServiceAttachmentValidation, String> {
    validate_service_attachment_path_inner(&path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;

    #[test]
    fn rejects_empty_path() {
        assert_eq!(
            validate_service_attachment_path_inner("").unwrap_err(),
            "Attachment path is empty"
        );
    }

    #[test]
    fn rejects_url_scheme() {
        assert_eq!(
            validate_service_attachment_path_inner("file:///tmp/x.png").unwrap_err(),
            "URL paths are not allowed"
        );
    }

    #[test]
    fn rejects_network_path() {
        assert_eq!(
            validate_service_attachment_path_inner("\\\\server\\share\\x.png").unwrap_err(),
            "Network paths are not allowed"
        );
    }

    #[test]
    fn rejects_parent_traversal() {
        assert_eq!(
            validate_service_attachment_path_inner("../secret.png").unwrap_err(),
            "Parent directory traversal is not allowed"
        );
    }

    #[test]
    fn accepts_valid_image() {
        let dir = std::env::temp_dir().join(format!("sabbathcue-attach-{}", std::process::id()));
        fs::create_dir_all(&dir).expect("temp dir");
        let file_path = dir.join("sample.png");
        let mut file = fs::File::create(&file_path).expect("create file");
        file.write_all(b"png").expect("write bytes");
        let validated = validate_service_attachment_path_inner(file_path.to_str().unwrap())
            .expect("valid attachment");
        assert_eq!(validated.label, "sample.png");
        assert_eq!(validated.kind, "slide");
        assert!(validated.size_bytes > 0);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn accepts_valid_pdf() {
        let dir =
            std::env::temp_dir().join(format!("sabbathcue-attach-pdf-{}", std::process::id()));
        fs::create_dir_all(&dir).expect("temp dir");
        let file_path = dir.join("sample.pdf");
        let mut file = fs::File::create(&file_path).expect("create file");
        file.write_all(b"pdf").expect("write bytes");
        let validated = validate_service_attachment_path_inner(file_path.to_str().unwrap())
            .expect("valid attachment");
        assert_eq!(validated.label, "sample.pdf");
        assert_eq!(validated.kind, "deck");
        assert!(validated.size_bytes > 0);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn rejects_video_extension() {
        let dir =
            std::env::temp_dir().join(format!("sabbathcue-attach-video-{}", std::process::id()));
        fs::create_dir_all(&dir).expect("temp dir");
        let file_path = dir.join("sample.mp4");
        let mut file = fs::File::create(&file_path).expect("create file");
        file.write_all(b"video").expect("write bytes");
        let err = validate_service_attachment_path_inner(file_path.to_str().unwrap()).unwrap_err();
        assert_eq!(err, "Unsupported attachment extension");
        let _ = fs::remove_dir_all(dir);
    }
}
