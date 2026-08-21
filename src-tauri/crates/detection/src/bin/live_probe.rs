//! Throwaway diagnostic: replicate the exact live semantic path
//! (`SemanticDetector::detect` — chunker, ensemble, threshold) for probe
//! phrases that the live app reports `candidates=0` on, using the same
//! model/index files the app loads.
//!
//! Usage (from repo root so defaults resolve):
//!   cargo run -p rhema-detection --features precompute-bin --bin `live_probe` -- \
//!     --model models/minilm-l6-v2-int8/onnx/model_quantized.onnx \
//!     --tokenizer models/minilm-l6-v2/tokenizer.json \
//!     --embeddings embeddings/public-minilm-l6-v2-q8.bin \
//!     --ids embeddings/public-minilm-l6-v2-q8-ids.bin \
//!     --input probes.txt

use std::path::PathBuf;

use rhema_detection::semantic::embedder::TextEmbedder;
use rhema_detection::{HnswVectorIndex, OnnxEmbedder, SemanticDetector};

fn arg(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1).cloned())
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let args: Vec<String> = std::env::args().collect();
    let model = arg(&args, "--model")
        .unwrap_or_else(|| "models/minilm-l6-v2-int8/onnx/model_quantized.onnx".into());
    let tokenizer =
        arg(&args, "--tokenizer").unwrap_or_else(|| "models/minilm-l6-v2/tokenizer.json".into());
    // Keep in sync with sabbathcue asset_paths PREFERRED_EMBEDDINGS_FILENAME.
    let embeddings = arg(&args, "--embeddings")
        .unwrap_or_else(|| "embeddings/public-minilm-l6-v2-q8.bin".into());
    let ids =
        arg(&args, "--ids").unwrap_or_else(|| "embeddings/public-minilm-l6-v2-q8-ids.bin".into());
    let input = arg(&args, "--input").expect("--input probes.txt required");

    let embedder = OnnxEmbedder::load(&PathBuf::from(&model), &PathBuf::from(&tokenizer))
        .expect("load ONNX model + tokenizer");
    let dim = TextEmbedder::dimension(&embedder);
    let index = HnswVectorIndex::load(&PathBuf::from(&embeddings), &PathBuf::from(&ids), dim)
        .expect("load embeddings index");

    let threshold = arg(&args, "--threshold")
        .and_then(|t| t.parse::<f64>().ok())
        .unwrap_or(0.75); // semantic_threshold from live settings

    let mut detector = SemanticDetector::new(Box::new(embedder), Box::new(index));
    detector.set_use_synonyms(true); // paraphrase=true, as in the live log
    detector.set_confidence_threshold(threshold);

    let text = std::fs::read_to_string(&input).expect("read --input file");
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let detections = detector.detect(line);
        if detections.is_empty() {
            println!("NONE     <- {line:?}");
        } else {
            for d in detections.iter().take(3) {
                println!(
                    "{:.3} verse_id={:?} {} {}:{}  <- {line:?}",
                    d.confidence,
                    d.verse_id,
                    d.verse_ref.book_name,
                    d.verse_ref.chapter,
                    d.verse_ref.verse_start,
                );
            }
        }
    }
}
