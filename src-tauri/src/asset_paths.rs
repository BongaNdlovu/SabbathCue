use std::path::PathBuf;

use tauri::{AppHandle, Manager};

pub const VOSK_MODEL_DIRNAME: &str = "vosk-model-small-en-us";
const USER_DEFAULT_VOSK_MODEL_DIR: &str = r"C:\Users\fanel\Downloads\vosk-model-small-en-us";
const VOSK_MODEL_DIRNAMES: &[&str] = &[
    "vosk-model-small-en-us",
    "vosk-model-en-us-0.22-lgraph",
    "vosk-model-en-us-0.22",
    "vosk-model-en-us-0.42-gigaspeech",
    "vosk-model-en-us-daanzu-20200905",
];

fn dev_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn first_existing(paths: impl IntoIterator<Item = PathBuf>) -> Option<PathBuf> {
    paths.into_iter().find(|p| p.exists())
}

fn is_vosk_model_dir(path: &PathBuf) -> bool {
    path.join("conf").exists() && path.join("am").exists()
}

fn resolve_vosk_model_dir(path: PathBuf) -> Option<PathBuf> {
    if is_vosk_model_dir(&path) {
        return Some(path);
    }

    for dirname in VOSK_MODEL_DIRNAMES {
        let nested = path.join(dirname);
        if is_vosk_model_dir(&nested) {
            return Some(nested);
        }
    }

    None
}

pub fn app_data_dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map_err(|e| format!("Unable to resolve app data directory: {e}"))
}

pub fn bible_db_path(app: &AppHandle) -> PathBuf {
    first_existing(
        [
            app.path().resource_dir().ok().map(|p| p.join("rhema.db")),
            Some(dev_root().join("data").join("rhema.db")),
        ]
        .into_iter()
        .flatten(),
    )
    .unwrap_or_else(|| dev_root().join("data").join("rhema.db"))
}

pub fn vosk_model_path(app: &AppHandle) -> PathBuf {
    let mut candidates = Vec::new();
    if let Ok(path) = std::env::var("SABBATHCUE_VOSK_MODEL_DIR") {
        if !path.trim().is_empty() {
            candidates.push(PathBuf::from(path));
        }
    }
    candidates.push(PathBuf::from(USER_DEFAULT_VOSK_MODEL_DIR));

    let roots = [
        app_data_dir(app)
            .ok()
            .map(|p| p.join("models").join("vosk")),
        app.path()
            .resource_dir()
            .ok()
            .map(|p| p.join("models").join("vosk")),
        Some(dev_root().join("models").join("vosk")),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>();

    for root in roots {
        candidates.push(root.join(VOSK_MODEL_DIRNAME));
        for dirname in VOSK_MODEL_DIRNAMES {
            candidates.push(root.join(dirname));
        }
    }

    candidates
        .into_iter()
        .find_map(resolve_vosk_model_dir)
        .unwrap_or_else(|| {
            app_data_dir(app)
                .unwrap_or_else(|_| dev_root())
                .join("models")
                .join("vosk")
                .join(VOSK_MODEL_DIRNAME)
        })
}

pub fn vosk_worker_path(app: &AppHandle) -> PathBuf {
    first_existing(
        [
            app.path()
                .resource_dir()
                .ok()
                .map(|p| p.join("scripts").join("vosk_worker.py")),
            Some(dev_root().join("scripts").join("vosk_worker.py")),
        ]
        .into_iter()
        .flatten(),
    )
    .unwrap_or_else(|| dev_root().join("scripts").join("vosk_worker.py"))
}

pub fn onnx_model_path(app: &AppHandle) -> PathBuf {
    first_existing(
        [
            app_data_dir(app).ok().map(|p| {
                p.join("models")
                    .join("minilm-l6-v2-int8")
                    .join("onnx")
                    .join("model_quantized.onnx")
            }),
            app.path().resource_dir().ok().map(|p| {
                p.join("models")
                    .join("minilm-l6-v2-int8")
                    .join("onnx")
                    .join("model_quantized.onnx")
            }),
            Some(
                dev_root()
                    .join("models")
                    .join("minilm-l6-v2-int8")
                    .join("onnx")
                    .join("model_quantized.onnx"),
            ),
            app_data_dir(app).ok().map(|p| {
                p.join("models")
                    .join("minilm-l6-v2")
                    .join("onnx")
                    .join("model.onnx")
            }),
            app.path().resource_dir().ok().map(|p| {
                p.join("models")
                    .join("minilm-l6-v2")
                    .join("onnx")
                    .join("model.onnx")
            }),
            Some(
                dev_root()
                    .join("models")
                    .join("minilm-l6-v2")
                    .join("onnx")
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
            .join("minilm-l6-v2-int8")
            .join("onnx")
            .join("model_quantized.onnx")
    })
}

pub fn tokenizer_path(app: &AppHandle) -> PathBuf {
    first_existing(
        [
            app_data_dir(app)
                .ok()
                .map(|p| p.join("models").join("minilm-l6-v2").join("tokenizer.json")),
            app.path()
                .resource_dir()
                .ok()
                .map(|p| p.join("models").join("minilm-l6-v2").join("tokenizer.json")),
            Some(
                dev_root()
                    .join("models")
                    .join("minilm-l6-v2")
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
            .join("minilm-l6-v2")
            .join("tokenizer.json")
    })
}

pub fn embeddings_path(app: &AppHandle) -> PathBuf {
    first_existing(
        [
            app_data_dir(app)
                .ok()
                .map(|p| p.join("embeddings").join("kjv-minilm-l6-v2.bin")),
            app.path()
                .resource_dir()
                .ok()
                .map(|p| p.join("embeddings").join("kjv-minilm-l6-v2.bin")),
            Some(dev_root().join("embeddings").join("kjv-minilm-l6-v2.bin")),
        ]
        .into_iter()
        .flatten(),
    )
    .unwrap_or_else(|| {
        app_data_dir(app)
            .unwrap_or_else(|_| dev_root())
            .join("embeddings")
            .join("kjv-minilm-l6-v2.bin")
    })
}

pub fn embedding_ids_path(app: &AppHandle) -> PathBuf {
    first_existing(
        [
            app_data_dir(app)
                .ok()
                .map(|p| p.join("embeddings").join("kjv-minilm-l6-v2-ids.bin")),
            app.path()
                .resource_dir()
                .ok()
                .map(|p| p.join("embeddings").join("kjv-minilm-l6-v2-ids.bin")),
            Some(
                dev_root()
                    .join("embeddings")
                    .join("kjv-minilm-l6-v2-ids.bin"),
            ),
        ]
        .into_iter()
        .flatten(),
    )
    .unwrap_or_else(|| {
        app_data_dir(app)
            .unwrap_or_else(|_| dev_root())
            .join("embeddings")
            .join("kjv-minilm-l6-v2-ids.bin")
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
