use std::path::PathBuf;

use tauri::{AppHandle, Manager};

pub const WHISPER_MODEL_FILENAME: &str = "ggml-large-v3-turbo-q8_0.bin";

fn dev_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn first_existing(paths: impl IntoIterator<Item = PathBuf>) -> Option<PathBuf> {
    paths.into_iter().find(|p| p.exists())
}

pub fn app_data_dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map_err(|e| format!("Unable to resolve app data directory: {e}"))
}

pub fn bible_db_path(app: &AppHandle) -> PathBuf {
    first_existing(
        [
            app.path()
                .resource_dir()
                .ok()
                .map(|p| p.join("rhema.db")),
            Some(dev_root().join("data").join("rhema.db")),
        ]
        .into_iter()
        .flatten(),
    )
    .unwrap_or_else(|| dev_root().join("data").join("rhema.db"))
}

pub fn whisper_model_path(app: &AppHandle) -> PathBuf {
    first_existing(
        [
            app_data_dir(app)
                .ok()
                .map(|p| p.join("models").join("whisper").join(WHISPER_MODEL_FILENAME)),
            app.path()
                .resource_dir()
                .ok()
                .map(|p| {
                    p.join("models")
                        .join("whisper")
                        .join(WHISPER_MODEL_FILENAME)
                }),
            Some(
                dev_root()
                    .join("models")
                    .join("whisper")
                    .join(WHISPER_MODEL_FILENAME),
            ),
        ]
        .into_iter()
        .flatten(),
    )
    .unwrap_or_else(|| {
        app_data_dir(app)
            .unwrap_or_else(|_| dev_root())
            .join("models")
            .join("whisper")
            .join(WHISPER_MODEL_FILENAME)
    })
}

pub fn onnx_model_path(app: &AppHandle) -> PathBuf {
    first_existing(
        [
            app_data_dir(app).ok().map(|p| {
                p.join("models")
                    .join("qwen3-embedding-0.6b-int8")
                    .join("model_quantized.onnx")
            }),
            app.path().resource_dir().ok().map(|p| {
                p.join("models")
                    .join("qwen3-embedding-0.6b-int8")
                    .join("model_quantized.onnx")
            }),
            Some(
                dev_root()
                    .join("models")
                    .join("qwen3-embedding-0.6b-int8")
                    .join("model_quantized.onnx"),
            ),
            app_data_dir(app).ok().map(|p| {
                p.join("models")
                    .join("qwen3-embedding-0.6b")
                    .join("model.onnx")
            }),
            app.path().resource_dir().ok().map(|p| {
                p.join("models")
                    .join("qwen3-embedding-0.6b")
                    .join("model.onnx")
            }),
            Some(
                dev_root()
                    .join("models")
                    .join("qwen3-embedding-0.6b")
                    .join("model.onnx"),
            ),
        ]
        .into_iter()
        .flatten(),
    )
    .unwrap_or_else(|| {
        app_data_dir(app)
            .unwrap_or_else(|_| dev_root())
            .join("models")
            .join("qwen3-embedding-0.6b-int8")
            .join("model_quantized.onnx")
    })
}

pub fn tokenizer_path(app: &AppHandle) -> PathBuf {
    first_existing(
        [
            app_data_dir(app).ok().map(|p| {
                p.join("models")
                    .join("qwen3-embedding-0.6b")
                    .join("tokenizer.json")
            }),
            app.path().resource_dir().ok().map(|p| {
                p.join("models")
                    .join("qwen3-embedding-0.6b")
                    .join("tokenizer.json")
            }),
            Some(
                dev_root()
                    .join("models")
                    .join("qwen3-embedding-0.6b")
                    .join("tokenizer.json"),
            ),
        ]
        .into_iter()
        .flatten(),
    )
    .unwrap_or_else(|| {
        app_data_dir(app)
            .unwrap_or_else(|_| dev_root())
            .join("models")
            .join("qwen3-embedding-0.6b")
            .join("tokenizer.json")
    })
}

pub fn embeddings_path(app: &AppHandle) -> PathBuf {
    first_existing(
        [
            app_data_dir(app)
                .ok()
                .map(|p| p.join("embeddings").join("kjv-qwen3-0.6b.bin")),
            app.path()
                .resource_dir()
                .ok()
                .map(|p| p.join("embeddings").join("kjv-qwen3-0.6b.bin")),
            Some(dev_root().join("embeddings").join("kjv-qwen3-0.6b.bin")),
        ]
        .into_iter()
        .flatten(),
    )
    .unwrap_or_else(|| {
        app_data_dir(app)
            .unwrap_or_else(|_| dev_root())
            .join("embeddings")
            .join("kjv-qwen3-0.6b.bin")
    })
}

pub fn embedding_ids_path(app: &AppHandle) -> PathBuf {
    first_existing(
        [
            app_data_dir(app)
                .ok()
                .map(|p| p.join("embeddings").join("kjv-qwen3-0.6b-ids.bin")),
            app.path()
                .resource_dir()
                .ok()
                .map(|p| p.join("embeddings").join("kjv-qwen3-0.6b-ids.bin")),
            Some(dev_root().join("embeddings").join("kjv-qwen3-0.6b-ids.bin")),
        ]
        .into_iter()
        .flatten(),
    )
    .unwrap_or_else(|| {
        app_data_dir(app)
            .unwrap_or_else(|_| dev_root())
            .join("embeddings")
            .join("kjv-qwen3-0.6b-ids.bin")
    })
}

pub fn ndi_library_path(app: &AppHandle) -> PathBuf {
    let sdk_dir = |root: PathBuf| root.join("sdk").join("ndi");

    let library_candidates = |base: PathBuf| -> Vec<PathBuf> {
        let sdk = sdk_dir(base);
        if cfg!(target_os = "windows") {
            vec![sdk.join("windows").join("Processing.NDI.Lib.x64.dll")]
        } else if cfg!(target_os = "macos") {
            vec![sdk.join("macos").join("libndi.dylib")]
        } else {
            vec![
                sdk.join("linux").join("libndi.so"),
                sdk.join("linux").join("x86_64").join("libndi.so.6"),
                sdk.join("linux").join("libndi.so.6"),
            ]
        }
    };

    let mut candidates = Vec::new();
    if let Ok(path) = app_data_dir(app) {
        candidates.extend(library_candidates(path));
    }
    if let Ok(path) = app.path().resource_dir() {
        candidates.extend(library_candidates(path));
    }
    candidates.extend(library_candidates(dev_root()));

    first_existing(candidates).unwrap_or_else(|| {
        let sdk = sdk_dir(dev_root());
        if cfg!(target_os = "windows") {
            sdk.join("windows").join("Processing.NDI.Lib.x64.dll")
        } else if cfg!(target_os = "macos") {
            sdk.join("macos").join("libndi.dylib")
        } else {
            sdk.join("linux").join("libndi.so")
        }
    })
}
