use std::path::{Path, PathBuf};

use tauri::{AppHandle, Manager};

pub const VOSK_ACCURATE_MODEL_DIRNAME: &str = "vosk-model-en-us-0.22-lgraph";
pub const VOSK_MODEL_DIRNAME: &str = VOSK_ACCURATE_MODEL_DIRNAME;
/// Well-known `LibreOffice` install locations probed after env and PATH lookups.
const SOFFICE_FIXED_CANDIDATES: &[&str] = &[
    r"C:\Program Files\LibreOffice\program\soffice.exe",
    r"C:\Program Files (x86)\LibreOffice\program\soffice.exe",
];
pub const PREFERRED_EMBEDDINGS_FILENAME: &str = "public-minilm-l6-v2-q8.bin";
pub const PREFERRED_EMBEDDING_IDS_FILENAME: &str = "public-minilm-l6-v2-q8-ids.bin";
const F32_EMBEDDINGS_FILENAME: &str = "public-minilm-l6-v2.bin";
const F32_EMBEDDING_IDS_FILENAME: &str = "public-minilm-l6-v2-ids.bin";
/// Basenames (lowercase) that must never load as the live semantic index.
/// English-only / pre-split corpora pass Gen 1:1 sanity but silently degrade quality.
const REJECTED_EMBEDDINGS_BASENAME_MARKERS: &[&str] = &[
    "kjv-minilm-l6-v2",
    "kjv-nkjv-nlt-minilm",
    "kjv-nkjv-nlt-gte",
];
#[cfg(test)]
const VOSK_SMALL_MODEL_DIRNAME: &str = "vosk-model-small-en-us";
const VOSK_MODEL_DIRNAMES: &[&str] = &[
    "vosk-model-en-us-0.22-lgraph",
    "vosk-model-small-en-us",
    "vosk-model-small-en-us-0.15",
    "vosk-model-en-us-0.22",
    "vosk-model-en-us-0.42-gigaspeech",
    "vosk-model-en-us-daanzu-20200905",
];

fn dev_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

/// Strip the Windows extended-length prefix (`\\?\`) from a path.
///
/// Tauri's `resource_dir()` returns canonicalized paths carrying this prefix.
/// With the prefix, Windows disables path normalization, so consumers that
/// join path segments with forward slashes (the Vosk/Kaldi C library does:
/// `<model>/conf/model.conf`) fail to open files — the worker reports
/// "Failed to create a model" even though the model directory exists.
pub fn simplify_windows_path(path: PathBuf) -> PathBuf {
    let text = path.to_string_lossy();
    if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
        return PathBuf::from(format!(r"\\{rest}"));
    }
    if let Some(rest) = text.strip_prefix(r"\\?\") {
        return PathBuf::from(rest.to_string());
    }
    path
}

fn first_existing(paths: impl IntoIterator<Item = PathBuf>) -> Option<PathBuf> {
    paths.into_iter().find(|p| p.exists())
}

fn is_minilm_asset(path: &Path) -> bool {
    path.to_string_lossy()
        .to_ascii_lowercase()
        .contains("minilm-l6-v2")
}

fn is_rejected_embeddings_basename(path: &Path) -> bool {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    REJECTED_EMBEDDINGS_BASENAME_MARKERS
        .iter()
        .any(|marker| name.contains(marker))
}

pub fn semantic_assets_are_compatible(
    model_path: &Path,
    tokenizer_path: &Path,
    embeddings_path: &Path,
    ids_path: &Path,
) -> bool {
    if is_rejected_embeddings_basename(embeddings_path)
        || is_rejected_embeddings_basename(ids_path)
    {
        return false;
    }
    [model_path, tokenizer_path, embeddings_path, ids_path]
        .iter()
        .all(|path| is_minilm_asset(path))
}

fn is_vosk_model_dir(path: &Path) -> bool {
    path.join("conf").join("model.conf").exists()
        && path.join("am").join("final.mdl").exists()
        && path.join("graph").exists()
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

pub fn library_media_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app_data_dir(app)?.join("library").join("media");
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("Unable to create library media directory: {e}"))?;
    Ok(dir)
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

    // The Vosk C library joins this path with forward slashes, which Windows
    // rejects under the `\\?\` prefix — always hand it a simplified path.
    simplify_windows_path(
        candidates
            .into_iter()
            .find_map(resolve_vosk_model_dir)
            .unwrap_or_else(|| {
                app_data_dir(app)
                    .unwrap_or_else(|_| dev_root())
                    .join("models")
                    .join("vosk")
                    .join(VOSK_MODEL_DIRNAME)
            }),
    )
}

pub fn vosk_worker_path(app: &AppHandle) -> PathBuf {
    simplify_windows_path(
        first_existing(
            [
                app.path()
                    .resource_dir()
                    .ok()
                    .map(|p| p.join("scripts").join("vosk_worker.exe")),
                Some(dev_root().join("sidecars").join("vosk_worker.exe")),
                app.path()
                    .resource_dir()
                    .ok()
                    .map(|p| p.join("scripts").join("vosk_worker.py")),
                Some(dev_root().join("scripts").join("vosk_worker.py")),
            ]
            .into_iter()
            .flatten(),
        )
        .unwrap_or_else(|| dev_root().join("scripts").join("vosk_worker.py")),
    )
}

/// Executable names for the `LibreOffice` CLI, per platform.
fn soffice_executable_names() -> &'static [&'static str] {
    if cfg!(windows) {
        &["soffice.exe", "soffice.com"]
    } else {
        &["soffice"]
    }
}

/// Build the ordered list of candidate `soffice` paths from an explicit
/// override, the directories in `PATH`, and well-known install locations.
///
/// Pure so the precedence can be unit-tested without touching the real
/// environment.
fn soffice_candidate_paths(
    env_override: Option<&str>,
    path_var: Option<&std::ffi::OsStr>,
    fixed: &[&str],
    exe_names: &[&str],
) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(value) = env_override {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            candidates.push(PathBuf::from(trimmed));
        }
    }
    if let Some(path_var) = path_var {
        for dir in std::env::split_paths(path_var) {
            for name in exe_names {
                candidates.push(dir.join(name));
            }
        }
    }
    candidates.extend(fixed.iter().map(PathBuf::from));
    candidates
}

/// Resolve a `LibreOffice` `soffice` executable used to convert `PowerPoint` decks
/// to PDF. Honors `SABBATHCUE_SOFFICE_PATH`, then `PATH`, then the common
/// Windows install directories. Returns `None` when `LibreOffice` is absent.
pub fn resolve_soffice() -> Option<PathBuf> {
    let env_override = std::env::var("SABBATHCUE_SOFFICE_PATH").ok();
    let candidates = soffice_candidate_paths(
        env_override.as_deref(),
        std::env::var_os("PATH").as_deref(),
        SOFFICE_FIXED_CANDIDATES,
        soffice_executable_names(),
    );
    candidates.into_iter().find(|candidate| candidate.is_file())
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

pub const EGW_EMBEDDINGS_FILENAME: &str = "egw-minilm-l6-v2.bin";
pub const EGW_EMBEDDING_IDS_FILENAME: &str = "egw-minilm-l6-v2-ids.bin";

/// On-device EGW paragraph embeddings (user-generated, never shipped).
pub fn egw_embeddings_path(app: &AppHandle) -> PathBuf {
    app_data_dir(app)
        .unwrap_or_else(|_| dev_root())
        .join("embeddings")
        .join(EGW_EMBEDDINGS_FILENAME)
}

pub fn egw_embedding_ids_path(app: &AppHandle) -> PathBuf {
    app_data_dir(app)
        .unwrap_or_else(|_| dev_root())
        .join("embeddings")
        .join(EGW_EMBEDDING_IDS_FILENAME)
}

/// Sidecar content fingerprint for the on-device EGW index, used to detect and
/// discard a stale index after the EGW corpus is re-imported.
///
/// `.with_extension("meta.json")` replaces the `.bin` extension, yielding
/// `egw-minilm-l6-v2.meta.json` alongside the embeddings file.
pub fn egw_embeddings_meta_path(app: &AppHandle) -> PathBuf {
    egw_embeddings_path(app).with_extension("meta.json")
}

/// All existing embeddings/ids file pairs, in resolution order (app data,
/// bundled resources, dev tree; preferred filename before legacy).
///
/// Each pair comes from the same root so a stale file in one location can
/// never be combined with ids from another. Startup walks these in order and
/// uses the first pair that passes the semantic sanity check, so a corrupt or
/// outdated app-data file cannot silently disable vector search while a
/// healthy bundled copy exists.
pub fn semantic_embedding_candidates(app: &AppHandle) -> Vec<(PathBuf, PathBuf)> {
    let roots: Vec<PathBuf> = [
        app_data_dir(app).ok(),
        app.path().resource_dir().ok(),
        Some(dev_root()),
    ]
    .into_iter()
    .flatten()
    .collect();

    semantic_embedding_candidates_for_roots(&roots)
        .into_iter()
        .filter(|(embeddings, ids)| embeddings.exists() && ids.exists())
        .collect()
}

fn semantic_embedding_candidates_for_roots(roots: &[PathBuf]) -> Vec<(PathBuf, PathBuf)> {
    // Public multi-vector corpus only (q8 preferred, f32 fallback).
    // Do not fall back to pre-split English-only indexes — they pass Gen 1:1
    // sanity but silently drop WEB/ES/FR/PT vectors and de-truncated KJV.
    let pairs = [
        (
            PREFERRED_EMBEDDINGS_FILENAME,
            PREFERRED_EMBEDDING_IDS_FILENAME,
        ),
        (F32_EMBEDDINGS_FILENAME, F32_EMBEDDING_IDS_FILENAME),
    ];

    pairs
        .iter()
        .flat_map(|(embeddings_name, ids_name)| {
            roots.iter().map(move |root| {
                (
                    root.join("embeddings").join(embeddings_name),
                    root.join("embeddings").join(ids_name),
                )
            })
        })
        .collect()
}

pub fn embeddings_path(app: &AppHandle) -> PathBuf {
    semantic_embedding_candidates(app)
        .into_iter()
        .next()
        .map_or_else(
            || {
                app_data_dir(app)
                    .unwrap_or_else(|_| dev_root())
                    .join("embeddings")
                    .join(PREFERRED_EMBEDDINGS_FILENAME)
            },
            |(embeddings, _)| embeddings,
        )
}

pub fn embedding_ids_path(app: &AppHandle) -> PathBuf {
    semantic_embedding_candidates(app)
        .into_iter()
        .next()
        .map_or_else(
            || {
                app_data_dir(app)
                    .unwrap_or_else(|_| dev_root())
                    .join("embeddings")
                    .join(PREFERRED_EMBEDDING_IDS_FILENAME)
            },
            |(_, ids)| ids,
        )
}

#[cfg(test)]
#[expect(
    clippy::items_after_test_module,
    reason = "NDI library resolution is kept after shared asset tests to avoid mixing unrelated path fixtures"
)]
mod tests {
    use super::*;

    fn make_fake_vosk_model(root: &Path) {
        std::fs::create_dir_all(root.join("conf")).expect("conf dir");
        std::fs::write(root.join("conf").join("model.conf"), b"").expect("model.conf");
        std::fs::create_dir_all(root.join("am")).expect("am dir");
        std::fs::write(root.join("am").join("final.mdl"), b"").expect("final.mdl");
        std::fs::create_dir_all(root.join("graph")).expect("graph dir");
    }

    #[test]
    fn is_vosk_model_dir_requires_all_markers() {
        let temp = tempfile::tempdir().expect("temp dir");
        let model = temp.path().join("model");

        assert!(!is_vosk_model_dir(&model), "empty dir is not a model");

        make_fake_vosk_model(&model);
        assert!(is_vosk_model_dir(&model), "complete layout is a model");

        std::fs::remove_file(model.join("am").join("final.mdl")).expect("remove final.mdl");
        assert!(
            !is_vosk_model_dir(&model),
            "missing acoustic model must be rejected"
        );
    }

    #[test]
    fn resolve_vosk_model_dir_accepts_direct_path() {
        let temp = tempfile::tempdir().expect("temp dir");
        let model = temp.path().join(VOSK_MODEL_DIRNAME);
        make_fake_vosk_model(&model);

        assert_eq!(resolve_vosk_model_dir(model.clone()), Some(model));
    }

    #[test]
    fn resolve_vosk_model_dir_descends_into_known_dirnames() {
        // Zip extractions often produce models/vosk/<downloaded-model>/<model>,
        // where the picked candidate is the parent directory.
        let temp = tempfile::tempdir().expect("temp dir");
        let nested = temp.path().join(VOSK_ACCURATE_MODEL_DIRNAME);
        make_fake_vosk_model(&nested);

        assert_eq!(
            resolve_vosk_model_dir(temp.path().to_path_buf()),
            Some(nested)
        );
    }

    #[test]
    fn resolve_vosk_model_dir_prefers_accurate_dynamic_graph_model() {
        let temp = tempfile::tempdir().expect("temp dir");
        let accurate = temp.path().join(VOSK_ACCURATE_MODEL_DIRNAME);
        let small = temp.path().join(VOSK_SMALL_MODEL_DIRNAME);
        make_fake_vosk_model(&small);
        make_fake_vosk_model(&accurate);

        assert_eq!(
            resolve_vosk_model_dir(temp.path().to_path_buf()),
            Some(accurate)
        );
    }

    #[test]
    fn resolve_vosk_model_dir_rejects_unrelated_dirs() {
        let temp = tempfile::tempdir().expect("temp dir");
        std::fs::create_dir_all(temp.path().join("random")).expect("random dir");

        assert_eq!(resolve_vosk_model_dir(temp.path().to_path_buf()), None);
    }

    #[test]
    fn simplify_windows_path_strips_extended_length_prefix() {
        // Regression: Tauri's resource_dir() returns `\\?\C:\...` paths; the
        // Vosk worker cannot load a model from them (forward-slash joins are
        // not normalized under the prefix), failing with
        // "Failed to create a model".
        assert_eq!(
            simplify_windows_path(PathBuf::from(r"\\?\C:\app\models\vosk")),
            PathBuf::from(r"C:\app\models\vosk")
        );
        assert_eq!(
            simplify_windows_path(PathBuf::from(r"\\?\UNC\server\share\models")),
            PathBuf::from(r"\\server\share\models")
        );
        assert_eq!(
            simplify_windows_path(PathBuf::from(r"C:\app\models\vosk")),
            PathBuf::from(r"C:\app\models\vosk")
        );
    }

    #[test]
    fn soffice_candidate_paths_prefer_override_then_path_then_fixed() {
        let path_var = std::env::join_paths(["C:\\tools", "C:\\bin"]).expect("join paths");
        let candidates = soffice_candidate_paths(
            Some("C:\\custom\\soffice.exe"),
            Some(path_var.as_os_str()),
            &["C:\\Program Files\\LibreOffice\\program\\soffice.exe"],
            &["soffice.exe"],
        );

        assert_eq!(
            candidates.first(),
            Some(&PathBuf::from("C:\\custom\\soffice.exe"))
        );
        assert!(candidates.contains(&PathBuf::from("C:\\tools\\soffice.exe")));
        assert!(candidates.contains(&PathBuf::from("C:\\bin\\soffice.exe")));
        assert_eq!(
            candidates.last(),
            Some(&PathBuf::from(
                "C:\\Program Files\\LibreOffice\\program\\soffice.exe"
            ))
        );
    }

    #[test]
    fn soffice_candidate_paths_skip_blank_override_and_missing_path() {
        let candidates = soffice_candidate_paths(Some("   "), None, &["soffice"], &["soffice"]);
        assert_eq!(candidates, vec![PathBuf::from("soffice")]);
    }

    #[test]
    fn first_existing_picks_first_present_path() {
        let temp = tempfile::tempdir().expect("temp dir");
        let present = temp.path().join("present.txt");
        std::fs::write(&present, b"x").expect("present file");

        let missing = temp.path().join("missing.txt");
        assert_eq!(
            first_existing([missing.clone(), present.clone()]),
            Some(present.clone())
        );
        assert_eq!(first_existing([missing.clone()]), None);
        assert_eq!(first_existing([present.clone(), missing]), Some(present));
    }

    #[test]
    fn semantic_candidates_prefer_paired_q8_then_f32_public_assets_only() {
        let root_a = PathBuf::from("a");
        let root_b = PathBuf::from("b");

        let candidates = semantic_embedding_candidates_for_roots(&[root_a.clone(), root_b.clone()]);

        assert_eq!(
            candidates,
            vec![
                (
                    root_a
                        .join("embeddings")
                        .join(PREFERRED_EMBEDDINGS_FILENAME),
                    root_a
                        .join("embeddings")
                        .join(PREFERRED_EMBEDDING_IDS_FILENAME),
                ),
                (
                    root_b
                        .join("embeddings")
                        .join(PREFERRED_EMBEDDINGS_FILENAME),
                    root_b
                        .join("embeddings")
                        .join(PREFERRED_EMBEDDING_IDS_FILENAME),
                ),
                (
                    root_a.join("embeddings").join(F32_EMBEDDINGS_FILENAME),
                    root_a.join("embeddings").join(F32_EMBEDDING_IDS_FILENAME),
                ),
                (
                    root_b.join("embeddings").join(F32_EMBEDDINGS_FILENAME),
                    root_b.join("embeddings").join(F32_EMBEDDING_IDS_FILENAME),
                ),
            ]
        );
        assert!(
            candidates.iter().all(|(emb, _)| {
                let name = emb.file_name().and_then(|n| n.to_str()).unwrap_or("");
                name.starts_with("public-minilm-l6-v2")
            }),
            "legacy kjv-* corpora must not appear in the candidate list"
        );
    }

    #[test]
    fn semantic_assets_are_compatible_accepts_matching_minilm_assets() {
        assert!(semantic_assets_are_compatible(
            Path::new("models/minilm-l6-v2-int8/onnx/model_quantized.onnx"),
            Path::new("models/minilm-l6-v2-int8/tokenizer.json"),
            Path::new("embeddings/public-minilm-l6-v2.bin"),
            Path::new("embeddings/public-minilm-l6-v2-ids.bin"),
        ));
    }

    #[test]
    fn semantic_assets_are_compatible_rejects_legacy_english_only_corpus() {
        assert!(!semantic_assets_are_compatible(
            Path::new("models/minilm-l6-v2-int8/onnx/model_quantized.onnx"),
            Path::new("models/minilm-l6-v2/tokenizer.json"),
            Path::new("embeddings/kjv-minilm-l6-v2.bin"),
            Path::new("embeddings/kjv-minilm-l6-v2-ids.bin"),
        ));
        assert!(!semantic_assets_are_compatible(
            Path::new("models/minilm-l6-v2-int8/onnx/model_quantized.onnx"),
            Path::new("models/minilm-l6-v2/tokenizer.json"),
            Path::new("embeddings/kjv-nkjv-nlt-minilm-l6-v2.bin"),
            Path::new("embeddings/kjv-nkjv-nlt-minilm-l6-v2-ids.bin"),
        ));
    }

    #[test]
    fn semantic_assets_are_compatible_rejects_unknown_model_family() {
        assert!(!semantic_assets_are_compatible(
            Path::new("models/unknown/onnx/model_quantized.onnx"),
            Path::new("models/minilm-l6-v2-int8/tokenizer.json"),
            Path::new("embeddings/public-minilm-l6-v2.bin"),
            Path::new("embeddings/public-minilm-l6-v2-ids.bin"),
        ));
    }

    #[test]
    fn semantic_assets_are_compatible_rejects_unknown_tokenizer_family() {
        assert!(!semantic_assets_are_compatible(
            Path::new("models/minilm-l6-v2-int8/onnx/model_quantized.onnx"),
            Path::new("models/unknown/tokenizer.json"),
            Path::new("embeddings/public-minilm-l6-v2.bin"),
            Path::new("embeddings/public-minilm-l6-v2-ids.bin"),
        ));
    }
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
