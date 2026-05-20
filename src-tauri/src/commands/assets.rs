#![expect(clippy::needless_pass_by_value, reason = "Tauri command extractors require pass-by-value")]

use serde::Serialize;
use tauri::AppHandle;

use crate::asset_paths;

#[derive(Debug, Serialize)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "frontend asset readiness DTO mirrors independent asset flags"
)]
pub struct AssetStatus {
    pub bible_db: bool,
    pub whisper_model: bool,
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
    let whisper_model = asset_paths::whisper_model_path(&app).exists();
    let onnx_model = asset_paths::onnx_model_path(&app).exists();
    let tokenizer = asset_paths::tokenizer_path(&app).exists();
    let embeddings = asset_paths::embeddings_path(&app).exists();
    let embedding_ids = asset_paths::embedding_ids_path(&app).exists();
    let ndi_sdk = asset_paths::ndi_library_path(&app).exists();

    AssetStatus {
        bible_db,
        whisper_model,
        onnx_model,
        tokenizer,
        embeddings,
        embedding_ids,
        semantic_ready: onnx_model && tokenizer && embeddings && embedding_ids,
        ndi_sdk,
    }
}
