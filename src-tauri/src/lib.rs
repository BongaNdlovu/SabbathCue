mod asset_paths;
mod commands;
mod events;
mod logging;
mod memstats;
mod state;

use std::io;
use std::panic;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, Once};

use rhema_detection::semantic::embedder::TextEmbedder;
use rhema_detection::semantic::index::VectorIndex;

static PANIC_HOOK: Once = Once::new();

fn install_panic_hook() {
    PANIC_HOOK.call_once(|| {
        let default_hook = panic::take_hook();
        panic::set_hook(Box::new(move |info| {
            eprintln!("Unhandled panic: {info}");
            log::error!("Unhandled panic: {info}");
            default_hook(info);
        }));
    });
}

fn poisoned_lock_error(name: &str) -> io::Error {
    io::Error::other(format!("{name} lock was poisoned"))
}

/// Verbatim KJV Genesis 1:1 — a verse guaranteed to be in every corpus.
const SEMANTIC_SANITY_PROBE: &str = "In the beginning God created the heaven and the earth.";
/// A healthy index returns its own verse at ~0.93+ cosine; a mismatched
/// embeddings file (built with a different model/pipeline) lands below the
/// 0.42 retrieval cutoff. Anything under this floor means the file does not
/// match the runtime embedder.
const SEMANTIC_SANITY_MIN_SIMILARITY: f64 = 0.80;

/// Embed a known verse and require the index to find it with near-self
/// similarity, so a mismatched embeddings file fails loudly at startup
/// instead of silently returning nothing for every live query.
fn semantic_index_sanity_check(
    embedder: &rhema_detection::OnnxEmbedder,
    index: &rhema_detection::HnswVectorIndex,
) -> Result<f64, String> {
    let embedding = embedder
        .embed(SEMANTIC_SANITY_PROBE)
        .map_err(|e| format!("sanity probe embed failed: {e}"))?;
    let results = index
        .search(&embedding, 1)
        .map_err(|e| format!("sanity probe search failed: {e}"))?;
    let top = results.first().map_or(0.0, |r| r.similarity);
    if top >= SEMANTIC_SANITY_MIN_SIMILARITY {
        Ok(top)
    } else {
        Err(format!(
            "top similarity {top:.3} for a verbatim verse (need >= {SEMANTIC_SANITY_MIN_SIMILARITY}); \
             the embeddings file does not match the runtime embedder — \
             regenerate it with `bun run precompute:embeddings`"
        ))
    }
}

/// Walk embedding candidates in resolution order; first load that passes the
/// self-similarity sanity check wins. A stale app-data file must not disable
/// vector search while a healthy bundled/dev copy exists.
fn load_first_healthy_index(
    embedder: &rhema_detection::OnnxEmbedder,
    model_path: &Path,
    tokenizer_path: &Path,
    embedding_candidates: &[(PathBuf, PathBuf)],
) -> Option<(rhema_detection::HnswVectorIndex, PathBuf)> {
    let dim = embedder.dimension();
    for (embeddings_path, ids_path) in embedding_candidates {
        if !asset_paths::semantic_assets_are_compatible(
            model_path,
            tokenizer_path,
            embeddings_path,
            ids_path,
        ) {
            log::warn!(
                "Skipping embeddings candidate from a different model family: {}",
                embeddings_path.display()
            );
            continue;
        }
        match rhema_detection::HnswVectorIndex::load(embeddings_path, ids_path, dim) {
            Ok(index) => match semantic_index_sanity_check(embedder, &index) {
                Ok(similarity) => {
                    log::info!(
                        "Resolved embeddings path: {} (sanity check passed, self-similarity {similarity:.3})",
                        embeddings_path.display()
                    );
                    return Some((index, embeddings_path.clone()));
                }
                Err(reason) => {
                    log::error!(
                        "Embeddings candidate failed sanity check, trying next: {reason} (embeddings={})",
                        embeddings_path.display()
                    );
                }
            },
            Err(e) => {
                log::warn!(
                    "Failed to load verse embeddings from {}: {e}",
                    embeddings_path.display()
                );
            }
        }
    }
    None
}

/// Read `record_count` out of a composition manifest.
///
/// Tolerates a leading UTF-8 BOM: editors on Windows readily add one, and
/// `serde_json` rejects U+FEFF outright. That cost a full session of semantic
/// vector search on 2026-08-04 — the manifest was present and correct, and the
/// only symptom was `could not parse manifest`.
fn manifest_record_count(manifest_text: &str) -> Result<u64, String> {
    let manifest_text = manifest_text.trim_start_matches('\u{feff}');
    let manifest: serde_json::Value = serde_json::from_str(manifest_text)
        .map_err(|e| format!("could not parse manifest: {e}"))?;
    manifest
        .get("record_count")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| "manifest missing record_count".to_string())
}

/// Composition fingerprint written by `export:verses` / setup (shared for f32 + q8).
/// Fail closed when missing or mismatched so a silent stale/wrong corpus cannot load.
fn embeddings_manifest_matches(
    embeddings_path: &Path,
    index: &rhema_detection::HnswVectorIndex,
) -> bool {
    let Some(manifest_dir) = embeddings_path.parent() else {
        log::error!(
            "SEMANTIC VECTOR SEARCH DISABLED — embeddings path has no parent directory: {}",
            embeddings_path.display()
        );
        return false;
    };
    let manifest_path = manifest_dir.join("public-minilm-l6-v2.manifest.json");
    if !manifest_path.exists() {
        log::error!(
            "SEMANTIC VECTOR SEARCH DISABLED — missing composition manifest {}. \
             Re-run `bun run export:verses` (writes the manifest) then \
             `bun run precompute:embeddings` && `bun run quantize:embeddings` if binaries are stale.",
            manifest_path.display()
        );
        return false;
    }
    let manifest_text = match std::fs::read_to_string(&manifest_path) {
        Ok(text) => text,
        Err(e) => {
            log::error!(
                "SEMANTIC VECTOR SEARCH DISABLED — could not read manifest {}: {e}",
                manifest_path.display()
            );
            return false;
        }
    };
    let expected = match manifest_record_count(&manifest_text) {
        Ok(count) => count,
        Err(reason) => {
            log::error!(
                "SEMANTIC VECTOR SEARCH DISABLED — {reason}: {}",
                manifest_path.display()
            );
            return false;
        }
    };
    let actual = index.len() as u64;
    if actual != expected {
        log::error!(
            "SEMANTIC VECTOR SEARCH DISABLED — embeddings count {actual} != \
             manifest record_count {expected} ({}). Regenerate embeddings.",
            manifest_path.display()
        );
        return false;
    }
    log::info!(
        "Embedding corpus manifest OK (record_count={expected}, {})",
        manifest_path.display()
    );
    true
}

/// Load the ONNX embedder and the pre-computed verse index into the shared
/// pipeline.
///
/// Runs off the setup hook: the model plus the embedding index take long
/// enough that doing this inline delays the event loop, and nothing needs it
/// to start — the pipeline serves direct detection from its stub semantic
/// detector until this swaps in the real one.
fn load_semantic_assets(app: &tauri::AppHandle) {
    use tauri::Manager;

    let model_path = asset_paths::onnx_model_path(app);
    let tokenizer_path = asset_paths::tokenizer_path(app);
    let embedding_candidates = asset_paths::semantic_embedding_candidates(app);

    log::info!("Resolved ONNX model path: {}", model_path.display());
    log::info!("Resolved tokenizer path: {}", tokenizer_path.display());
    for (embeddings, ids) in &embedding_candidates {
        log::info!(
            "Embeddings candidate: {} (ids={})",
            embeddings.display(),
            ids.display()
        );
    }

    if !model_path.exists() || !tokenizer_path.exists() {
        log::info!(
            "ONNX model not found. Semantic search disabled. Run 'bun run download:model' to download."
        );
        return;
    }

    let embedder = match rhema_detection::OnnxEmbedder::load(&model_path, &tokenizer_path) {
        Ok(embedder) => {
            log::info!("ONNX embedding model loaded");
            embedder
        }
        Err(e) => {
            log::warn!("Failed to load ONNX model: {e}");
            return;
        }
    };

    if embedding_candidates.is_empty() {
        log::info!(
            "No pre-computed public verse embeddings found. Regenerate with: \
             `bun run export:verses` then `bun run precompute:embeddings` then `bun run quantize:embeddings`"
        );
    }

    let Some((index, embeddings_path)) = load_first_healthy_index(
        &embedder,
        &model_path,
        &tokenizer_path,
        &embedding_candidates,
    ) else {
        log::error!(
            "SEMANTIC VECTOR SEARCH DISABLED — no public-minilm-l6-v2 embeddings candidate loaded. \
             English-only legacy indexes (kjv-minilm-*) are no longer used. Regenerate with: \
             `bun run export:verses` && `bun run precompute:embeddings` && `bun run quantize:embeddings`"
        );
        return;
    };

    if !embeddings_manifest_matches(&embeddings_path, &index) {
        return;
    }

    let semantic_corpus = "public-domain multi-vector corpus";
    log::info!(
        "Verse embeddings loaded ({} vectors, corpus={semantic_corpus}; semantic hits resolve to active translation)",
        index.len(),
    );

    let semantic = rhema_detection::SemanticDetector::new(Box::new(embedder), Box::new(index));
    let managed_pipeline = app.state::<Mutex<rhema_detection::DetectionPipeline>>();
    match managed_pipeline.lock() {
        Ok(mut pipeline) => pipeline.set_semantic(semantic),
        Err(_) => log::error!("Detection pipeline lock poisoned; semantic search stays disabled"),
    };
}

#[expect(clippy::too_many_lines, reason = "app setup is inherently complex")]
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Load .env file — try src-tauri/.env first, then project root ../.env
    dotenvy::dotenv().ok();
    dotenvy::from_filename("../.env").ok();
    install_panic_hook();
    let detection_cooldown = rhema_detection::AutoQueueCooldown::default();
    let run_result = tauri::Builder::default()
        .plugin(logging::build_log_plugin())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .manage(Mutex::new(state::AppState::new()))
        .manage(Mutex::new(
            rhema_detection::DetectionPipeline::with_cooldown(detection_cooldown.clone()),
        ))
        .manage(Mutex::new(rhema_broadcast::ndi::NdiRuntime::default()))
        .manage(Mutex::new(rhema_detection::DirectDetector::new()))
        .manage(Mutex::new(rhema_detection::DetectionMerger::with_cooldown(
            detection_cooldown,
        )))
        .manage(Mutex::new(rhema_detection::ReadingMode::new()))
        .manage(Mutex::new(
            commands::egw_semantic::EgwSemanticState::default(),
        ))
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
            commands::egw::egw_list_books,
            commands::egw::egw_list_chapters,
            commands::egw::egw_list_pages,
            commands::egw::egw_get_chapter,
            commands::egw::egw_get_page,
            commands::egw::egw_get_paragraph,
            commands::egw::egw_search,
            commands::egw_semantic::egw_semantic_status,
            commands::egw_semantic::egw_build_semantic_index,
            commands::egw_semantic::egw_semantic_search,
            commands::detection::detect_verses,
            commands::detection::detection_status,
            commands::detection::semantic_search,
            commands::detection::toggle_paraphrase_detection,
            commands::detection::reading_mode_status,
            commands::detection::stop_reading_mode,
            commands::detection::set_reading_mode_reference,
            commands::detection::update_detection_settings,
            commands::detection::set_detection_paused,
            commands::detection::detection_control_status,
            commands::assets::asset_status,
            commands::assets::get_service_attachment_limits,
            commands::assets::validate_service_attachment_path,
            commands::audio::get_audio_devices,
            commands::stt::start_transcription,
            commands::stt::set_input_gain,
            commands::stt::stop_transcription,
            commands::broadcast::list_monitors,
            commands::broadcast::ensure_broadcast_window,
            commands::broadcast::open_broadcast_window,
            commands::broadcast::close_broadcast_window,
            commands::broadcast::flash_monitor_labels,
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
            commands::secrets::has_soniox_api_key,
            commands::secrets::set_soniox_api_key,
            commands::secrets::clear_soniox_api_key,
            commands::secrets::has_speechmatics_api_key,
            commands::secrets::set_speechmatics_api_key,
            commands::secrets::clear_speechmatics_api_key,
            commands::secrets::has_deepseek_api_key,
            commands::secrets::set_deepseek_api_key,
            commands::secrets::clear_deepseek_api_key,
            commands::secrets::has_cerebras_api_key,
            commands::secrets::set_cerebras_api_key,
            commands::secrets::clear_cerebras_api_key,
            commands::secrets::validate_deepgram_api_key,
            commands::secrets::validate_soniox_api_key,
            commands::secrets::validate_speechmatics_api_key,
            commands::deepseek::validate_deepseek_api_key,
            commands::deepseek::validate_cerebras_api_key,
            commands::deepseek::rank_detection_candidates,
            commands::secrets::has_remote_http_token,
            commands::remote::rotate_remote_http_token,
            commands::secrets::has_verification_token,
            commands::secrets::set_verification_token,
            commands::secrets::get_verification_token,
            commands::secrets::rotate_verification_token,
            commands::secrets::clear_verification_token,
            commands::installation_identity::get_or_create_installation_identity,
            commands::installation_identity::adopt_installation_device_id,
            commands::installation_identity::sign_installation_challenge,
            commands::theme_files::import_theme_from_path,
            commands::theme_files::export_theme_to_path,
            commands::theme_files::read_image_as_data_url,
            commands::library::save_library_image,
            commands::library::delete_library_image,
            commands::powerpoint::convert_powerpoint_to_pdf,
            commands::video::validate_video_path,
        ])
        .setup(|app| {
            use tauri::Manager;

            // Startup banner: guarantees the session log is never empty and
            // records the resolved STT asset paths for offline diagnosis.
            log::info!(
                "SabbathCue v{} starting (pid {})",
                app.package_info().version,
                std::process::id()
            );
            let vosk_model = asset_paths::vosk_model_path(app.handle());
            let vosk_worker = asset_paths::vosk_worker_path(app.handle());
            log::info!(
                "Resolved Vosk model path: {} (exists={}) - default local STT",
                vosk_model.display(),
                vosk_model.exists()
            );
            log::info!(
                "Resolved Vosk worker path: {} (exists={})",
                vosk_worker.display(),
                vosk_worker.exists()
            );

            memstats::spawn();

            let db_path = asset_paths::bible_db_path(app.handle());

            if db_path.exists() {
                let bible_db = if app
                    .path()
                    .resource_dir()
                    .ok()
                    .is_some_and(|dir| db_path.starts_with(dir))
                {
                    rhema_bible::BibleDb::open_readonly(&db_path)
                } else {
                    rhema_bible::BibleDb::open(&db_path)
                };

                match bible_db {
                    Ok(bible_db) => {
                        let managed_state = app.state::<Mutex<state::AppState>>();
                        let mut state = managed_state
                            .lock()
                            .map_err(|_| poisoned_lock_error("App state"))?;
                        if let Ok(translations) = bible_db.list_translations() {
                            if let Some(translation_id) =
                                state::initial_translation_id(&translations)
                            {
                                state.active_translation_id = translation_id;
                            }
                        }
                        state.bible_db = Some(bible_db);
                        drop(state);
                        log::info!("Bible database loaded from {}", db_path.display());
                    }
                    Err(error) => {
                        log::error!(
                            "Failed to open Bible database at {}: {error}",
                            db_path.display()
                        );
                    }
                }
            } else {
                log::warn!("Bible database not found at {}", db_path.display());
            }

            let semantic_app = app.handle().clone();
            std::thread::spawn(move || load_semantic_assets(&semantic_app));

            Ok(())
        })
        .run(tauri::generate_context!());

    if let Err(error) = run_result {
        log::error!("Tauri application exited with error: {error}");
    }
}

#[cfg(test)]
mod manifest_tests {
    use super::manifest_record_count;

    const MANIFEST: &str = r#"{"schema_version":1,"record_count":155345}"#;

    #[test]
    fn reads_record_count() {
        assert_eq!(manifest_record_count(MANIFEST), Ok(155_345));
    }

    #[test]
    fn tolerates_a_utf8_bom() {
        // The shipped manifest carried EF BB BF and disabled vector search for
        // an entire service; the file itself was correct.
        let with_bom = format!("\u{feff}{MANIFEST}");
        assert_eq!(manifest_record_count(&with_bom), Ok(155_345));
    }

    #[test]
    fn reports_missing_record_count() {
        assert!(manifest_record_count(r#"{"schema_version":1}"#)
            .is_err_and(|reason| reason.contains("missing record_count")));
    }

    #[test]
    fn reports_unparseable_manifest() {
        assert!(manifest_record_count("not json")
            .is_err_and(|reason| reason.contains("could not parse")));
    }
}
