mod asset_paths;
mod commands;
mod events;
mod memstats;
mod state;

use std::sync::Mutex;

#[expect(clippy::too_many_lines, reason = "app setup is inherently complex")]
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Load .env file — try src-tauri/.env first, then project root ../.env
    dotenvy::dotenv().ok();
    dotenvy::from_filename("../.env").ok();
    tauri::Builder::default()
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(tauri_plugin_log::log::LevelFilter::Info)
                .build(),
        )
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .manage(Mutex::new(state::AppState::new()))
        .manage(Mutex::new(rhema_detection::DetectionPipeline::new()))
        .manage(Mutex::new(rhema_broadcast::ndi::NdiRuntime::default()))
        .manage(Mutex::new(rhema_detection::DirectDetector::new()))
        .manage(Mutex::new(rhema_detection::DetectionMerger::new()))
        .manage(Mutex::new(rhema_detection::ReadingMode::new()))
        .manage(Mutex::new(commands::remote::OscRuntime::new()))
        .manage(Mutex::new(commands::remote::HttpRuntime::new()))
        .invoke_handler(tauri::generate_handler![
            commands::bible::list_translations,
            commands::bible::list_books,
            commands::bible::get_chapter,
            commands::bible::get_verse,
            commands::bible::search_verses,
            commands::bible::get_translation_verses_for_search,
            commands::bible::get_cross_references,
            commands::bible::get_active_translation,
            commands::bible::set_active_translation,
            commands::detection::detect_verses,
            commands::detection::detection_status,
            commands::detection::semantic_search,
            commands::detection::toggle_paraphrase_detection,
            commands::detection::reading_mode_status,
            commands::detection::stop_reading_mode,
            commands::detection::update_detection_settings,
            commands::detection::set_detection_paused,
            commands::detection::detection_control_status,
            commands::assets::asset_status,
            commands::audio::get_audio_devices,
            commands::stt::start_transcription,
            commands::stt::stop_transcription,
            commands::broadcast::list_monitors,
            commands::broadcast::ensure_broadcast_window,
            commands::broadcast::open_broadcast_window,
            commands::broadcast::close_broadcast_window,
            commands::broadcast::start_ndi,
            commands::broadcast::stop_ndi,
            commands::broadcast::get_ndi_status,
            commands::broadcast::push_ndi_frame,
            commands::remote::start_osc,
            commands::remote::stop_osc,
            commands::remote::get_osc_status,
            commands::remote::start_http,
            commands::remote::stop_http,
            commands::remote::get_http_status,
            commands::remote::update_remote_status,
            commands::secrets::has_deepgram_api_key,
            commands::secrets::set_deepgram_api_key,
            commands::secrets::clear_deepgram_api_key,
            commands::secrets::has_remote_http_token,
            commands::secrets::reveal_remote_http_token,
            commands::secrets::rotate_remote_http_token,
            commands::theme_files::import_theme_from_path,
            commands::theme_files::export_theme_to_path,
            commands::theme_files::read_image_as_data_url,
        ])
        .setup(|app| {
            use tauri::Manager;

            memstats::spawn();

            let db_path = asset_paths::bible_db_path(app.handle());

            if db_path.exists() {
                let bible_db = if app.path().resource_dir().ok().is_some_and(|dir| {
                    db_path.starts_with(dir)
                }) {
                    rhema_bible::BibleDb::open_readonly(&db_path)
                        .expect("Failed to open bundled Bible database")
                } else {
                    rhema_bible::BibleDb::open(&db_path)
                        .expect("Failed to open Bible database")
                };

                let managed_state = app.state::<Mutex<state::AppState>>();
                let mut state = managed_state.lock().unwrap();
                state.bible_db = Some(bible_db);
                drop(state);
                log::info!("Bible database loaded from {}", db_path.display());
            } else {
                log::warn!("Bible database not found at {}", db_path.display());
            }

            let model_path = asset_paths::onnx_model_path(app.handle());
            let tokenizer_path = asset_paths::tokenizer_path(app.handle());
            let embeddings_path = asset_paths::embeddings_path(app.handle());
            let ids_path = asset_paths::embedding_ids_path(app.handle());

            log::info!("Resolved ONNX model path: {}", model_path.display());
            log::info!("Resolved tokenizer path: {}", tokenizer_path.display());
            log::info!("Resolved embeddings path: {}", embeddings_path.display());
            log::info!("Resolved embedding ids path: {}", ids_path.display());

            if model_path.exists() && tokenizer_path.exists() {
                use rhema_detection::semantic::embedder::TextEmbedder;
                use rhema_detection::semantic::index::VectorIndex;
                match rhema_detection::OnnxEmbedder::load(&model_path, &tokenizer_path) {
                    Ok(embedder) => {
                        log::info!("ONNX embedding model loaded");
                        let managed_pipeline = app.state::<Mutex<rhema_detection::DetectionPipeline>>();
                        let mut pipeline = managed_pipeline.lock().unwrap();

                        // If pre-computed embeddings exist, load the vector index
                        if embeddings_path.exists() && ids_path.exists() {
                            let dim = embedder.dimension();
                            match rhema_detection::HnswVectorIndex::load(&embeddings_path, &ids_path, dim) {
                                Ok(index) => {
                                    log::info!("Verse embeddings loaded ({} vectors)", index.len());
                                    pipeline.set_semantic(
                                        rhema_detection::SemanticDetector::new(
                                            Box::new(embedder),
                                            Box::new(index),
                                        ),
                                    );
                                }
                                Err(e) => {
                                    log::warn!("Failed to load verse embeddings: {e}");
                                }
                            }
                        } else {
                            log::info!("No pre-computed verse embeddings found. Run 'bun run export:verses' then the precompute binary.");
                        }
                    }
                    Err(e) => {
                        log::warn!("Failed to load ONNX model: {e}");
                    }
                }
            } else {
                log::info!("ONNX model not found. Semantic search disabled. Run 'bun run download:model' to download.");
            }

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
