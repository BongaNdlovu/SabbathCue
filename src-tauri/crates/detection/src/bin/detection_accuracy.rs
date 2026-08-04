//! Detection accuracy benchmark: runs a labeled set of sermon-style utterances
//! through the live auto-live detection path (direct reference parsing + hybrid
//! FTS5 BM25 + vector search, mirroring the STT pipeline) and reports precision /
//! recall / accuracy at the auto-live confidence threshold.
//!
//! The dataset is authored and documented (not field-recorded), so the number
//! reflects a curated, diverse test set rather than live-service data. It is the
//! best defensible measurement available without a labeled sermon corpus.
//!
//! Usage (from repo root, so the default asset paths resolve):
//!   cargo run -p rhema-detection --features precompute-bin --release \
//!     --bin `detection_accuracy` -- [--threshold 0.90] \
//!     [--cases PATH] [--model PATH] [--tokenizer PATH] \
//!     [--embeddings PATH] [--ids PATH] [--verses PATH]

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use rhema_bible::BibleDb;
use rhema_detection::semantic::embedder::TextEmbedder;
use rhema_detection::{
    DetectionPipeline, HnswVectorIndex, MergedDetection, OnnxEmbedder, SemanticDetector,
};

const DEFAULT_EXTERNAL_CASES: &str = "data/detection-fixtures/sermon-transcript-cases.json";
const SEMANTIC_AUTO_LIVE_MIN_MARGIN: f64 = 0.02;

/// One labeled case: (category, utterance, expected reference or None for noise).
type Case = (&'static str, &'static str, Option<&'static str>);

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
enum CaseMode {
    Fire,
    Hint,
    Abstain,
    Silent,
}

#[derive(Debug, Clone)]
struct BenchCase {
    language: String,
    category: String,
    timestamp: Option<String>,
    text: String,
    mode: CaseMode,
    expected_refs: Vec<String>,
    forbidden_refs: Vec<String>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct FixtureCase {
    language: Option<String>,
    category: String,
    timestamp: Option<String>,
    text: String,
    mode: Option<CaseMode>,
    expected: Option<String>,
    expected_any: Option<Vec<String>>,
    forbidden: Option<Vec<String>>,
}

/// Authored labeled dataset (~50 cases) spanning the detector's strategies:
/// `direct` (spoken explicit references), `spoken` (number-word references),
/// `quote` (near-verbatim KJV text), `para` (paraphrase), and `noise`
/// (ordinary sermon/announcement speech that must NOT fire).
const CASES: &[Case] = &[
    // --- direct: explicit "Book chapter:verse" references ---
    (
        "direct",
        "Let's turn to John 3:16 this morning",
        Some("John 3:16"),
    ),
    (
        "direct",
        "Our text today is Romans 8:28",
        Some("Romans 8:28"),
    ),
    (
        "direct",
        "Open your Bibles to Genesis 1:1",
        Some("Genesis 1:1"),
    ),
    ("direct", "Turn with me to Psalm 23:1", Some("Psalms 23:1")),
    (
        "direct",
        "Reading from Philippians 4:13",
        Some("Philippians 4:13"),
    ),
    ("direct", "Consider Proverbs 3:5", Some("Proverbs 3:5")),
    ("direct", "Matthew 5:16 tells us", Some("Matthew 5:16")),
    ("direct", "As we see in Isaiah 53:5", Some("Isaiah 53:5")),
    (
        "direct",
        "Look at First Corinthians 13:4",
        Some("1 Corinthians 13:4"),
    ),
    (
        "direct",
        "It is found in Hebrews 11:1",
        Some("Hebrews 11:1"),
    ),
    (
        "direct",
        "From the book of Jeremiah 29:11",
        Some("Jeremiah 29:11"),
    ),
    ("direct", "Ephesians 2:8 reminds us", Some("Ephesians 2:8")),
    // --- spoken: number words instead of digits ---
    (
        "spoken",
        "John chapter three verse sixteen",
        Some("John 3:16"),
    ),
    (
        "spoken",
        "Romans chapter eight verse twenty eight",
        Some("Romans 8:28"),
    ),
    (
        "spoken",
        "Psalm chapter twenty three verse one",
        Some("Psalms 23:1"),
    ),
    (
        "spoken",
        "Genesis chapter one verse one",
        Some("Genesis 1:1"),
    ),
    (
        "spoken",
        "Matthew chapter five verse sixteen",
        Some("Matthew 5:16"),
    ),
    (
        "spoken",
        "First Corinthians chapter thirteen verse four",
        Some("1 Corinthians 13:4"),
    ),
    (
        "spoken",
        "Philippians chapter four verse thirteen",
        Some("Philippians 4:13"),
    ),
    (
        "spoken",
        "Proverbs chapter three verse five",
        Some("Proverbs 3:5"),
    ),
    // --- quote: near-verbatim KJV text, no spoken reference ---
    (
        "quote",
        "For God so loved the world that he gave his only begotten Son",
        Some("John 3:16"),
    ),
    (
        "quote",
        "The Lord is my shepherd I shall not want",
        Some("Psalms 23:1"),
    ),
    (
        "quote",
        "In the beginning God created the heaven and the earth",
        Some("Genesis 1:1"),
    ),
    (
        "quote",
        "I can do all things through Christ which strengtheneth me",
        Some("Philippians 4:13"),
    ),
    (
        "quote",
        "Trust in the Lord with all thine heart and lean not unto thine own understanding",
        Some("Proverbs 3:5"),
    ),
    (
        "quote",
        "For all have sinned and come short of the glory of God",
        Some("Romans 3:23"),
    ),
    (
        "quote",
        "The wages of sin is death but the gift of God is eternal life",
        Some("Romans 6:23"),
    ),
    (
        "quote",
        "I am the way the truth and the life",
        Some("John 14:6"),
    ),
    (
        "quote",
        "Be still and know that I am God",
        Some("Psalms 46:10"),
    ),
    (
        "quote",
        "Now faith is the substance of things hoped for the evidence of things not seen",
        Some("Hebrews 11:1"),
    ),
    (
        "quote",
        "Let your light so shine before men that they may see your good works",
        Some("Matthew 5:16"),
    ),
    (
        "quote",
        "And we know that all things work together for good to them that love God",
        Some("Romans 8:28"),
    ),
    // --- para: paraphrase, same meaning, different words ---
    (
        "para",
        "God loved us so much that he sent his only son to save everyone who believes",
        Some("John 3:16"),
    ),
    (
        "para",
        "the Lord looks after me like a shepherd so I never go without",
        Some("Psalms 23:1"),
    ),
    (
        "para",
        "we are saved by grace through faith and not by our own works",
        Some("Ephesians 2:8"),
    ),
    (
        "para",
        "let your light shine so people can see your good deeds",
        Some("Matthew 5:16"),
    ),
    (
        "para",
        "give all your worries to God because he genuinely cares about you",
        Some("1 Peter 5:7"),
    ),
    (
        "para",
        "everyone has sinned and fallen short of God's glory",
        Some("Romans 3:23"),
    ),
    (
        "para",
        "I have learned to be content no matter what situation I am in",
        Some("Philippians 4:11"),
    ),
    (
        "para",
        "the joy of the Lord is my strength",
        Some("Nehemiah 8:10"),
    ),
    // --- noise: ordinary speech that must NOT fire a verse ---
    (
        "noise",
        "good morning church it is so wonderful to be together today",
        None,
    ),
    (
        "noise",
        "as we continue our sermon series this morning let us settle our hearts",
        None,
    ),
    (
        "noise",
        "the offering plates will be passed down each row in just a moment",
        None,
    ),
    (
        "noise",
        "a big thank you to the worship team for leading us so beautifully",
        None,
    ),
    (
        "noise",
        "please remember the fellowship lunch in the hall after the service",
        None,
    ),
    (
        "noise",
        "we have some announcements about the youth retreat next weekend",
        None,
    ),
    (
        "noise",
        "let us stand together and greet one another with a smile",
        None,
    ),
    (
        "noise",
        "the parking lot will be repaved starting on monday",
        None,
    ),
    (
        "noise",
        "our guest speaker travelled a long way to be with us today",
        None,
    ),
    (
        "noise",
        "please silence your phones before we begin the message",
        None,
    ),
    // ===== Expanded validation set (harder + more diverse) =====
    // --- direct: more books, including numbered and less-common ---
    ("direct", "Please turn to Acts 2:38", Some("Acts 2:38")),
    (
        "direct",
        "We're looking at Galatians 5:22 today",
        Some("Galatians 5:22"),
    ),
    ("direct", "Second Timothy 3:16", Some("2 Timothy 3:16")),
    ("direct", "Let's read Exodus 20:3", Some("Exodus 20:3")),
    ("direct", "Open to Revelation 21:4", Some("Revelation 21:4")),
    ("direct", "Our passage is James 1:5", Some("James 1:5")),
    ("direct", "Turn to Joshua 1:9", Some("Joshua 1:9")),
    ("direct", "Look at Micah 6:8", Some("Micah 6:8")),
    ("direct", "Reading Colossians 3:23", Some("Colossians 3:23")),
    ("direct", "First John 4:8", Some("1 John 4:8")),
    ("direct", "Daniel 3:17", Some("Daniel 3:17")),
    (
        "direct",
        "The blessing in Numbers 6:24",
        Some("Numbers 6:24"),
    ),
    // --- spoken: number words ---
    (
        "spoken",
        "Acts chapter two verse thirty eight",
        Some("Acts 2:38"),
    ),
    (
        "spoken",
        "Galatians chapter five verse twenty two",
        Some("Galatians 5:22"),
    ),
    (
        "spoken",
        "Exodus chapter twenty verse three",
        Some("Exodus 20:3"),
    ),
    (
        "spoken",
        "Revelation chapter twenty one verse four",
        Some("Revelation 21:4"),
    ),
    ("spoken", "James chapter one verse five", Some("James 1:5")),
    (
        "spoken",
        "Joshua chapter one verse nine",
        Some("Joshua 1:9"),
    ),
    (
        "spoken",
        "second Timothy chapter three verse sixteen",
        Some("2 Timothy 3:16"),
    ),
    (
        "spoken",
        "first John chapter four verse eight",
        Some("1 John 4:8"),
    ),
    // --- quote: verbatim KJV, varied verses ---
    (
        "quote",
        "But the fruit of the Spirit is love, joy, peace, longsuffering",
        Some("Galatians 5:22"),
    ),
    (
        "quote",
        "All scripture is given by inspiration of God and is profitable for doctrine",
        Some("2 Timothy 3:16"),
    ),
    (
        "quote",
        "I am the resurrection and the life",
        Some("John 11:25"),
    ),
    (
        "quote",
        "Come unto me all ye that labour and are heavy laden and I will give you rest",
        Some("Matthew 11:28"),
    ),
    (
        "quote",
        "The Lord bless thee and keep thee",
        Some("Numbers 6:24"),
    ),
    (
        "quote",
        "Have not I commanded thee Be strong and of a good courage",
        Some("Joshua 1:9"),
    ),
    (
        "quote",
        "He hath shewed thee O man what is good",
        Some("Micah 6:8"),
    ),
    (
        "quote",
        "And God shall wipe away all tears from their eyes",
        Some("Revelation 21:4"),
    ),
    (
        "quote",
        "If any of you lack wisdom let him ask of God",
        Some("James 1:5"),
    ),
    (
        "quote",
        "In my Father's house are many mansions",
        Some("John 14:2"),
    ),
    ("quote", "Let not your heart be troubled", Some("John 14:1")),
    (
        "quote",
        "Greater love hath no man than this than to lay down his life",
        Some("John 15:13"),
    ),
    (
        "quote",
        "Study to shew thyself approved unto God",
        Some("2 Timothy 2:15"),
    ),
    (
        "quote",
        "Thy word is a lamp unto my feet and a light unto my path",
        Some("Psalms 119:105"),
    ),
    // --- para: looser paraphrases (the hard category) ---
    (
        "para",
        "the spirit produces love joy and peace in our lives",
        Some("Galatians 5:22"),
    ),
    (
        "para",
        "every part of scripture is breathed out by God and useful for teaching",
        Some("2 Timothy 3:16"),
    ),
    (
        "para",
        "come to me everyone who is tired and burdened and I will give you rest",
        Some("Matthew 11:28"),
    ),
    (
        "para",
        "God will wipe away every tear and there will be no more death or pain",
        Some("Revelation 21:4"),
    ),
    (
        "para",
        "what does God require of you but to act justly love mercy and walk humbly",
        Some("Micah 6:8"),
    ),
    (
        "para",
        "don't be anxious about anything but pray about everything",
        Some("Philippians 4:6"),
    ),
    (
        "para",
        "we live by faith and not by what we can see",
        Some("2 Corinthians 5:7"),
    ),
    (
        "para",
        "seek first the kingdom of God and his righteousness",
        Some("Matthew 6:33"),
    ),
    (
        "para",
        "if we confess our sins he is faithful and just to forgive us",
        Some("1 John 1:9"),
    ),
    (
        "para",
        "whatever you do work at it with all your heart as for the Lord",
        Some("Colossians 3:23"),
    ),
    // --- noise: theme-laden sentences that must NOT fire a verse ---
    (
        "noise",
        "this morning I want us to think about what faith really means in our daily lives",
        None,
    ),
    (
        "noise",
        "the choir will sing two songs before the message today",
        None,
    ),
    (
        "noise",
        "let us pray together as we open God's word this morning",
        None,
    ),
    (
        "noise",
        "god has been so good to our church family this whole year",
        None,
    ),
    (
        "noise",
        "we believe in grace and mercy and the love of our savior every day",
        None,
    ),
    (
        "noise",
        "turn and tell your neighbour good morning and God bless you",
        None,
    ),
    (
        "noise",
        "the building fund is almost at our goal praise the Lord",
        None,
    ),
    (
        "noise",
        "next week we begin a new series on the life of David",
        None,
    ),
    (
        "noise",
        "i remember when i first gave my heart to the Lord as a child",
        None,
    ),
    (
        "noise",
        "there is real power in prayer and in coming together as one",
        None,
    ),
    (
        "noise",
        "let me share a quick story about a missionary i met overseas",
        None,
    ),
    (
        "noise",
        "please stand as we welcome our visitors and first time guests",
        None,
    ),
];

const AFRIKAANS_CASES: &[Case] = &[
    (
        "af-direct",
        "Deuteronomium 16 vers 18",
        Some("Deuteronomium 16:18"),
    ),
    ("af-direct", "Matteus 20 vers 25", Some("Matteus 20:25")),
    (
        "af-direct",
        "Eerste Samuel 8 vers 7",
        Some("1 Samuel 8:7"),
    ),
    (
        "af-quote",
        "Regters en opsigters moet jy vir jou aanstel in al jou poorte wat die HERE jou God jou sal gee",
        Some("Deuteronomium 16:18"),
    ),
    (
        "af-quote",
        "Toe sê die HERE vir Samuel Luister na die volk in alles wat hulle aan jou sê want nie jou het hulle verwerp nie maar My het hulle verwerp om nie koning oor hulle te wees nie",
        Some("1 Samuel 8:7"),
    ),
    (
        "af-noise",
        "die poort was natuurlik 'n plek waar die volk gekom het om regspraak te ontvang",
        None,
    ),
];

/// Paraphrases whose intended verse is visible and useful to an operator, but
/// whose wording is not specific enough to justify automatic live output.
const BUILT_IN_HINT_UTTERANCES: &[&str] = &[
    "God loved us so much that he sent his only son to save everyone who believes",
    "let your light shine so people can see your good deeds",
    "I have learned to be content no matter what situation I am in",
    "And God shall wipe away all tears from their eyes",
    "Let not your heart be troubled",
    "the spirit produces love joy and peace in our lives",
    "every part of scripture is breathed out by God and useful for teaching",
    "God will wipe away every tear and there will be no more death or pain",
    "don't be anxious about anything but pray about everything",
    "we live by faith and not by what we can see",
    "whatever you do work at it with all your heart as for the Lord",
];

/// Inputs that are either non-unique paraphrases or require source/model data
/// absent from the public runtime. They remain safety cases: no verse may go
/// live, while an operator hint is optional.
const BUILT_IN_ABSTAIN_UTTERANCES: &[&str] = &[
    "the Lord looks after me like a shepherd so I never go without",
    "the joy of the Lord is my strength",
];

#[expect(
    clippy::too_many_lines,
    reason = "diagnostic bin keeps the benchmark flow in one readable script"
)]
#[expect(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "benchmark case counts are small enough for exact f64 metric math"
)]
fn main() {
    // Off unless RUST_LOG asks for it. The per-candidate scoring lines in
    // `live_fts_candidate_confidence` are the only way to see why one verse
    // outranked another, and without this they were unreachable from the
    // harness.
    env_logger::init();
    let args: Vec<String> = std::env::args().collect();
    let threshold: f64 = arg(&args, "--threshold")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0.90);
    let min_precision = arg(&args, "--min-precision").and_then(|s| s.parse::<f64>().ok());
    let min_recall = arg(&args, "--min-recall").and_then(|s| s.parse::<f64>().ok());
    let model = arg(&args, "--model")
        .unwrap_or_else(|| "models/minilm-l6-v2-int8/onnx/model_quantized.onnx".into());
    let tokenizer =
        arg(&args, "--tokenizer").unwrap_or_else(|| "models/minilm-l6-v2/tokenizer.json".into());
    let embeddings =
        arg(&args, "--embeddings").unwrap_or_else(|| "embeddings/public-minilm-l6-v2.bin".into());
    let ids =
        arg(&args, "--ids").unwrap_or_else(|| "embeddings/public-minilm-l6-v2-ids.bin".into());
    let verses = arg(&args, "--verses").unwrap_or_else(|| "data/verses-for-embedding.json".into());
    let db_path = arg(&args, "--db").unwrap_or_else(|| "data/rhema.db".into());
    let cases_path = arg(&args, "--cases").unwrap_or_else(|| DEFAULT_EXTERNAL_CASES.into());

    let embedder = OnnxEmbedder::load(&PathBuf::from(&model), &PathBuf::from(&tokenizer))
        .expect("load ONNX model + tokenizer");
    let dim = TextEmbedder::dimension(&embedder);
    let index = HnswVectorIndex::load(&PathBuf::from(&embeddings), &PathBuf::from(&ids), dim)
        .expect("load embeddings index");
    let refs = load_refs(&verses);
    let db = BibleDb::open_readonly(&PathBuf::from(&db_path)).expect("open bible db (read-only)");

    let mut semantic = SemanticDetector::new(Box::new(embedder), Box::new(index));
    semantic.set_use_synonyms(true);
    let mut pipeline = DetectionPipeline::new();
    pipeline.set_semantic(semantic);

    let mut cases = built_in_cases();
    if cases_path != "none" {
        let external = load_fixture_cases(Path::new(&cases_path))
            .unwrap_or_else(|e| panic!("load detection cases from {cases_path}: {e}"));
        eprintln!("Loaded {} external cases from {cases_path}", external.len());
        cases.extend(external);
    }

    // Per-category tallies and global confusion counts.
    let mut tp = 0usize; // expected verse, correct verse fired
    let mut fp = 0usize; // something fired that was wrong (noise or wrong verse)
    let mut fn_ = 0usize; // expected verse, nothing correct fired
    let mut tn = 0usize; // noise, nothing fired
    let mut hint_hits = 0usize;
    let mut hint_total = 0usize;
    let mut abstain_hits = 0usize;
    let mut abstain_total = 0usize;
    let mut case_hits = 0usize;
    let mut by_cat: HashMap<String, (usize, usize)> = HashMap::new(); // cat -> (hits, total)
    let mut calibration = [(0usize, 0usize); 10];
    let mut selector = AutoLiveSelector::default();
    let mut detection_latencies_ms = Vec::with_capacity(cases.len());

    println!("Per-case outcome at threshold {:.0}%:\n", threshold * 100.0);
    let mut current_language: Option<String> = None;
    for case in &cases {
        // Mirror the live auto-live path: direct parsing on the fragment, plus
        // hybrid FTS5 + vector search (the explicit-reference and quote/paraphrase
        // strategies the real STT pipeline runs). The live session sets the STT
        // language once per session, not per fragment — switching it resets the
        // detector's pending-reference state, so only switch on actual change.
        if current_language.as_deref() != Some(case.language.as_str()) {
            pipeline.direct_mut().set_stt_language(&case.language);
            current_language = Some(case.language.clone());
        }
        let detection_started = Instant::now();
        let mut detections = pipeline.process_direct(&case.text);
        let fts = db.search_verses_bm25(&case.text, 10).unwrap_or_default();
        detections.extend(pipeline.process_hybrid_with_fts(&case.text, &fts));
        detection_latencies_ms.push(detection_started.elapsed().as_secs_f64() * 1_000.0);
        // A case represents a stable partial/final event pair. Auto-live fires
        // the strongest eligible candidate after applying production semantic
        // confirmation to that pair.
        let fired = select_stable_case(&mut selector, &detections, threshold, &refs);

        let fired_ref = fired.and_then(|d| detection_ref(&d.detection, &refs));
        let fired_conf = fired.map(|d| d.detection.confidence);
        let detected_refs = detections
            .iter()
            .filter_map(|d| detection_ref(&d.detection, &refs))
            .collect::<Vec<_>>();
        let expected_hint = case
            .expected_refs
            .iter()
            .find(|expected| detected_refs.iter().any(|got| ref_eq(expected, got)));
        let forbidden_hit = case
            .forbidden_refs
            .iter()
            .find(|forbidden| detected_refs.iter().any(|got| ref_eq(forbidden, got)));
        let held = held_summary(&detections, &refs);

        let entry = by_cat.entry(case.category.clone()).or_insert((0, 0));
        entry.1 += 1;

        let (case_correct, outcome) = match case.mode {
            // In fire mode, `forbidden` means "must not go live". A forbidden
            // ref sitting in the held candidate list next to a correctly fired
            // verse is acceptable operator-panel behavior, not a failure.
            CaseMode::Fire => match (&fired_ref, forbidden_hit) {
                (Some(got), _) if expected_ref_matches(&case.expected_refs, got) => {
                    tp += 1;
                    (
                        true,
                        format!("OK  fired {} ({:.0}%)", got, fired_conf.unwrap() * 100.0),
                    )
                }
                (Some(got), _) => {
                    fp += 1;
                    fn_ += 1;
                    (
                        false,
                        format!("WRONG fired {} ({:.0}%)", got, fired_conf.unwrap() * 100.0),
                    )
                }
                (None, Some(forbidden)) => {
                    fn_ += 1;
                    (false, format!("FORBIDDEN-HINT {forbidden}"))
                }
                (None, None) => {
                    fn_ += 1;
                    (false, "miss (held for review, nothing fired)".to_string())
                }
            },
            CaseMode::Hint => {
                hint_total += 1;
                match (&fired_ref, expected_hint, forbidden_hit) {
                    (Some(got), _, _) => {
                        fp += 1;
                        (
                            false,
                            format!("FALSE-FIRE {} ({:.0}%)", got, fired_conf.unwrap() * 100.0),
                        )
                    }
                    // As in fire mode, a forbidden ref in the held candidate
                    // list next to the expected hint is acceptable
                    // operator-panel behavior — the expected hint wins.
                    (None, Some(expected), _) => {
                        tn += 1;
                        hint_hits += 1;
                        (true, format!("OK  hint {expected} held for review"))
                    }
                    (None, None, Some(forbidden)) => {
                        tn += 1;
                        (false, format!("FORBIDDEN-HINT {forbidden}"))
                    }
                    (None, None, None) => {
                        tn += 1;
                        (false, "hint miss (nothing fired)".to_string())
                    }
                }
            }
            CaseMode::Abstain => {
                abstain_total += 1;
                match (&fired_ref, forbidden_hit) {
                    (Some(got), _) => {
                        fp += 1;
                        (
                            false,
                            format!("FALSE-FIRE {} ({:.0}%)", got, fired_conf.unwrap() * 100.0),
                        )
                    }
                    (None, None) => {
                        tn += 1;
                        abstain_hits += 1;
                        (true, "OK  safely abstained".to_string())
                    }
                    (None, Some(forbidden)) => {
                        tn += 1;
                        (false, format!("FORBIDDEN-HINT {forbidden}"))
                    }
                }
            }
            CaseMode::Silent => match &fired_ref {
                Some(got) => {
                    fp += 1;
                    (
                        false,
                        format!("FALSE-FIRE {} ({:.0}%)", got, fired_conf.unwrap() * 100.0),
                    )
                }
                None if forbidden_hit.is_none() => {
                    tn += 1;
                    (true, "OK  silent".to_string())
                }
                None => {
                    tn += 1;
                    (false, format!("FORBIDDEN-HINT {}", forbidden_hit.unwrap()))
                }
            },
        };

        if let Some(confidence) = fired_conf {
            let bin = ((confidence * 10.0).floor() as usize).min(9);
            calibration[bin].1 += 1;
            if case_correct {
                calibration[bin].0 += 1;
            }
        }

        if case_correct {
            case_hits += 1;
            entry.0 += 1;
        }
        let want = case.want_label();
        let outcome = append_held(outcome, &held);
        let timestamp = case
            .timestamp
            .as_deref()
            .map_or_else(String::new, |value| format!(" @{value}"));
        println!(
            "[{:>12}{timestamp:<10}] want {want:<28} -> {outcome}",
            case.category
        );
    }

    let total = cases.len();
    let precision = if tp + fp == 0 {
        1.0
    } else {
        tp as f64 / (tp + fp) as f64
    };
    let positives = cases
        .iter()
        .filter(|case| case.mode == CaseMode::Fire)
        .count();
    let recall = if positives == 0 {
        0.0
    } else {
        tp as f64 / positives as f64
    };
    let accuracy = (tp + tn) as f64 / total as f64;
    let case_success = case_hits as f64 / total as f64;

    println!("\nBy category (correct / total):");
    let mut categories = by_cat.keys().collect::<Vec<_>>();
    categories.sort();
    for cat in categories {
        if let Some((hit, tot)) = by_cat.get(cat) {
            println!("  {cat:>12}  {hit}/{tot}");
        }
    }

    println!("\nConfusion at {:.0}% threshold:", threshold * 100.0);
    println!("  true positives (correct verse live): {tp}");
    println!("  false positives (wrong/noise live):  {fp}");
    println!("  false negatives (missed, held back):  {fn_}");
    println!("  true negatives (correctly silent):   {tn}");
    if hint_total > 0 {
        println!("  semantic hints found / expected:      {hint_hits}/{hint_total}");
    }
    if abstain_total > 0 {
        println!("  safe abstentions / expected:          {abstain_hits}/{abstain_total}");
    }
    println!(
        "\nHeadline metrics ({total} cases, threshold {:.0}%):",
        threshold * 100.0
    );
    println!(
        "  Precision (of what goes live, % correct): {:.1}%",
        precision * 100.0
    );
    println!(
        "  Recall    (of spoken verses, % caught):   {:.1}%",
        recall * 100.0
    );
    println!(
        "  Accuracy  (correct live/silent decisions): {:.1}%",
        accuracy * 100.0
    );
    println!(
        "  Case pass (including hint expectations):   {:.1}%",
        case_success * 100.0
    );

    println!("\nMatch-strength calibration (correct / fired):");
    for (bin, (correct, fired)) in calibration.iter().enumerate() {
        if *fired > 0 {
            println!("  {:>2}-{:>3}%  {correct}/{fired}", bin * 10, bin * 10 + 9);
        }
    }

    detection_latencies_ms.sort_by(f64::total_cmp);
    if let Some(max) = detection_latencies_ms.last() {
        println!("\nDetection processing latency (transcript received to ranked candidates):");
        println!(
            "  p50: {:.1} ms  p95: {:.1} ms  max: {:.1} ms",
            percentile(&detection_latencies_ms, 0.50),
            percentile(&detection_latencies_ms, 0.95),
            max
        );
    }

    let precision_failed = min_precision.is_some_and(|minimum| precision < minimum);
    let recall_failed = min_recall.is_some_and(|minimum| recall < minimum);
    if precision_failed || recall_failed {
        eprintln!(
            "Accuracy gate failed: precision={precision:.3} recall={recall:.3} minimum_precision={min_precision:?} minimum_recall={min_recall:?}"
        );
        std::process::exit(1);
    }
}

#[expect(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the bounded sample length and 0..=1 percentile produce a valid index"
)]
fn percentile(sorted_values: &[f64], percentile: f64) -> f64 {
    if sorted_values.is_empty() {
        return 0.0;
    }
    let index = ((sorted_values.len() - 1) as f64 * percentile).round() as usize;
    sorted_values[index]
}

fn select_stable_case<'a>(
    selector: &mut AutoLiveSelector,
    detections: &'a [MergedDetection],
    threshold: f64,
    refs: &HashMap<i64, String>,
) -> Option<&'a MergedDetection> {
    // The authored corpus contains utterances rather than raw provider events.
    // Replay the same ranked candidates as a stable partial/final pair so a
    // moderate semantic winner can satisfy the production two-event rule.
    selector
        .select(detections, threshold, refs)
        .or_else(|| selector.select(detections, threshold, refs))
}

#[derive(Default)]
struct AutoLiveSelector {
    pending_semantic: Option<String>,
}

impl AutoLiveSelector {
    fn select<'a>(
        &mut self,
        detections: &'a [MergedDetection],
        threshold: f64,
        refs: &HashMap<i64, String>,
    ) -> Option<&'a MergedDetection> {
        let best = |source_is_direct: bool| {
            detections
                .iter()
                .filter(|result| {
                    matches!(
                        result.detection.source,
                        rhema_detection::types::DetectionSource::DirectReference
                    ) == source_is_direct
                        && result.detection.confidence >= threshold
                        && !result.detection.is_chapter_only
                })
                .max_by(|a, b| detection_ordering_cmp(&a.detection, &b.detection))
        };

        if let Some(direct) = best(true) {
            self.pending_semantic = None;
            return Some(direct);
        }

        let semantic = best(false)?;
        let runner_up = detections
            .iter()
            .filter(|candidate| {
                matches!(
                    candidate.detection.source,
                    rhema_detection::types::DetectionSource::Semantic { .. }
                ) && !candidate.detection.is_chapter_only
                    && !std::ptr::eq(*candidate, semantic)
            })
            .max_by(|a, b| detection_ordering_cmp(&a.detection, &b.detection));
        if runner_up.is_some_and(|runner| {
            detection_ordering_gap(&semantic.detection, &runner.detection)
                < SEMANTIC_AUTO_LIVE_MIN_MARGIN
        }) {
            return None;
        }
        if semantic.detection.confidence >= 0.95 {
            self.pending_semantic = None;
            return Some(semantic);
        }

        let key = detection_ref(&semantic.detection, refs)?;
        if self.pending_semantic.as_deref() == Some(key.as_str()) {
            self.pending_semantic = None;
            Some(semantic)
        } else {
            self.pending_semantic = Some(key);
            None
        }
    }
}

fn detection_ordering_cmp(
    a: &rhema_detection::types::Detection,
    b: &rhema_detection::types::Detection,
) -> std::cmp::Ordering {
    a.confidence
        .partial_cmp(&b.confidence)
        .unwrap_or(std::cmp::Ordering::Equal)
        .then_with(|| {
            a.rank_score()
                .partial_cmp(&b.rank_score())
                .unwrap_or(std::cmp::Ordering::Equal)
        })
}

fn detection_ordering_gap(
    strongest: &rhema_detection::types::Detection,
    runner_up: &rhema_detection::types::Detection,
) -> f64 {
    let confidence_gap = strongest.confidence - runner_up.confidence;
    if confidence_gap.abs() > f64::EPSILON {
        confidence_gap
    } else {
        strongest.rank_score() - runner_up.rank_score()
    }
}

impl BenchCase {
    fn want_label(&self) -> String {
        if self.expected_refs.is_empty() {
            "(noise)".to_string()
        } else {
            self.expected_refs.join(" / ")
        }
    }
}

impl FixtureCase {
    fn into_bench_case(self, path: &Path, index: usize) -> Result<BenchCase, String> {
        let language = non_empty_or_default(self.language.as_deref(), "en");
        let category = non_empty(&self.category, path, index, "category")?;
        let text = non_empty(&self.text, path, index, "text")?;
        let mut expected_refs = self
            .expected
            .into_iter()
            .chain(self.expected_any.unwrap_or_default())
            .filter_map(|value| trim_non_empty(&value))
            .collect::<Vec<_>>();
        expected_refs.dedup_by(|a, b| ref_eq(a, b));
        let forbidden_refs = self
            .forbidden
            .unwrap_or_default()
            .into_iter()
            .filter_map(|value| trim_non_empty(&value))
            .collect::<Vec<_>>();
        let mode = self.mode.unwrap_or({
            if expected_refs.is_empty() {
                CaseMode::Silent
            } else {
                CaseMode::Fire
            }
        });
        if matches!(mode, CaseMode::Fire | CaseMode::Hint) && expected_refs.is_empty() {
            return Err(format!(
                "{} case #{index} is {:?} but has no expected reference",
                path.display(),
                mode
            ));
        }

        Ok(BenchCase {
            language,
            category,
            timestamp: self.timestamp.and_then(|value| trim_non_empty(&value)),
            text,
            mode,
            expected_refs,
            forbidden_refs,
        })
    }
}

fn built_in_cases() -> Vec<BenchCase> {
    CASES
        .iter()
        .map(|case| built_in_case("en", case))
        .chain(AFRIKAANS_CASES.iter().map(|case| built_in_case("af", case)))
        .collect()
}

fn built_in_case(language: &str, case: &Case) -> BenchCase {
    let expected_refs = case
        .2
        .map_or_else(Vec::new, |expected| vec![expected.to_string()]);
    let mode = built_in_mode(language, case, expected_refs.is_empty());
    BenchCase {
        language: language.to_string(),
        category: case.0.to_string(),
        timestamp: None,
        text: case.1.to_string(),
        mode,
        expected_refs,
        forbidden_refs: vec![],
    }
}

fn built_in_mode(language: &str, case: &Case, expected_refs_empty: bool) -> CaseMode {
    if expected_refs_empty {
        CaseMode::Silent
    } else if (language == "af" && case.0 == "af-quote")
        || BUILT_IN_ABSTAIN_UTTERANCES.contains(&case.1)
    {
        CaseMode::Abstain
    } else if BUILT_IN_HINT_UTTERANCES.contains(&case.1) {
        CaseMode::Hint
    } else {
        CaseMode::Fire
    }
}

fn load_fixture_cases(path: &Path) -> Result<Vec<BenchCase>, String> {
    let json = std::fs::read_to_string(path)
        .map_err(|e| format!("failed to read {}: {e}", path.display()))?;
    serde_json::from_str::<Vec<FixtureCase>>(&json)
        .map_err(|e| format!("failed to parse {}: {e}", path.display()))?
        .into_iter()
        .enumerate()
        .map(|(index, case)| case.into_bench_case(path, index + 1))
        .collect()
}

fn non_empty_or_default(value: Option<&str>, default: &str) -> String {
    value
        .and_then(trim_non_empty)
        .unwrap_or_else(|| default.to_string())
}

fn non_empty(value: &str, path: &Path, index: usize, field: &str) -> Result<String, String> {
    trim_non_empty(value).ok_or_else(|| {
        format!(
            "{} case #{index} has an empty {field} field",
            path.display()
        )
    })
}

fn trim_non_empty(value: &str) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

fn expected_ref_matches(expected_refs: &[String], got: &str) -> bool {
    expected_refs.iter().any(|expected| ref_eq(expected, got))
}

/// Build a (book, chapter, verse) reference string for a detection: direct
/// detections carry the `verse_ref`; semantic detections carry only a
/// `verse_id`, which we resolve through the verses-for-embedding map.
fn detection_ref(
    detection: &rhema_detection::types::Detection,
    refs: &HashMap<i64, String>,
) -> Option<String> {
    // Direct and FTS5 detections carry a populated verse_ref; pure vector
    // semantic detections carry only a verse_id resolved via the verse map.
    if detection.verse_ref.book_number > 0 {
        let verse = detection.verse_ref.verse_end.map_or_else(
            || detection.verse_ref.verse_start.to_string(),
            |verse_end| format!("{}-{verse_end}", detection.verse_ref.verse_start),
        );
        return Some(format!(
            "{} {}:{verse}",
            detection.verse_ref.book_name, detection.verse_ref.chapter
        ));
    }
    detection.verse_id.and_then(|id| refs.get(&id).cloned())
}

fn held_summary(
    detections: &[rhema_detection::MergedDetection],
    refs: &HashMap<i64, String>,
) -> Vec<String> {
    let mut entries = detections
        .iter()
        .filter_map(|result| {
            detection_ref(&result.detection, refs).map(|reference| {
                let source = match result.detection.source {
                    rhema_detection::types::DetectionSource::DirectReference => "direct",
                    rhema_detection::types::DetectionSource::Semantic { .. } => "semantic",
                };
                (
                    reference.clone(),
                    result.detection.confidence,
                    format!(
                        "{} {:.0}% {}",
                        reference,
                        result.detection.confidence * 100.0,
                        source
                    ),
                )
            })
        })
        .collect::<Vec<_>>();
    entries.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.cmp(&b.0))
    });

    let mut seen = Vec::<String>::new();
    entries
        .into_iter()
        .filter_map(|(reference, _confidence, summary)| {
            if seen.iter().any(|value| ref_eq(value, &reference)) {
                None
            } else {
                seen.push(reference);
                Some(summary)
            }
        })
        .take(5)
        .collect()
}

fn append_held(outcome: String, held: &[String]) -> String {
    if held.is_empty() {
        outcome
    } else {
        format!("{outcome} | held {}", held.join(", "))
    }
}

/// Compare two references for the same verse, tolerant of book-name spelling
/// (`1`/`I`/`First` Corinthians, `Psalm`/`Psalms`).
fn ref_eq(a: &str, b: &str) -> bool {
    normalize_ref(a) == normalize_ref(b)
}

fn normalize_ref(s: &str) -> Option<(String, String)> {
    // Split "Book Chapter:Verse" into (normalized book, "chapter:verse").
    let (book, loc) = s.rsplit_once(' ')?;
    let book = book.trim().to_lowercase();
    let mut parts = book.splitn(2, ' ');
    let first = parts.next().unwrap_or("");
    let rest = parts.next();
    let num = match first {
        "1" | "i" | "first" => Some("1"),
        "2" | "ii" | "second" => Some("2"),
        "3" | "iii" | "third" => Some("3"),
        _ => None,
    };
    let normalized_book = match (num, rest) {
        (Some(n), Some(r)) => format!("{n} {}", singularize(r)),
        _ => singularize(&book),
    };
    Some((normalized_book, loc.trim().to_string()))
}

fn singularize(book: &str) -> String {
    // Only Psalms varies between singular/plural in practice here.
    match book {
        "revelation of john" => "revelation".to_string(),
        "psalm" => "psalms".to_string(),
        other => other.to_string(),
    }
}

fn load_refs(path: &str) -> HashMap<i64, String> {
    #[derive(serde::Deserialize)]
    struct Entry {
        id: i64,
        r#ref: String,
    }
    let Ok(json) = std::fs::read_to_string(path) else {
        eprintln!("warning: could not read {path}; semantic matches cannot be scored");
        return HashMap::new();
    };
    serde_json::from_str::<Vec<Entry>>(&json)
        .map(|entries| entries.into_iter().map(|e| (e.id, e.r#ref)).collect())
        .unwrap_or_default()
}

fn arg(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| i.checked_add(1))
        .and_then(|i| args.get(i))
        .cloned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rhema_detection::types::{Detection, DetectionSource, VerseRef};

    #[test]
    fn fixture_case_defaults_to_fire_when_expected_is_present() {
        let case = FixtureCase {
            language: None,
            category: "fixture".to_string(),
            timestamp: None,
            text: "Galatians 2:20".to_string(),
            mode: None,
            expected: Some("Galatians 2:20".to_string()),
            expected_any: None,
            forbidden: None,
        }
        .into_bench_case(Path::new("fixture.json"), 1)
        .unwrap();

        assert_eq!(case.mode, CaseMode::Fire);
    }

    #[test]
    fn fixture_case_rejects_hint_without_expected_reference() {
        let err = FixtureCase {
            language: None,
            category: "fixture".to_string(),
            timestamp: None,
            text: "ordinary speech".to_string(),
            mode: Some(CaseMode::Hint),
            expected: None,
            expected_any: None,
            forbidden: None,
        }
        .into_bench_case(Path::new("fixture.json"), 1)
        .unwrap_err();

        assert!(err.contains("has no expected reference"));
    }

    #[test]
    fn load_fixture_cases_reads_sermon_regression_fixture() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../data/detection-fixtures/sermon-transcript-cases.json");
        let cases = load_fixture_cases(&path).unwrap();

        assert!(cases.iter().any(|case| case.mode == CaseMode::Hint));
    }

    #[test]
    fn load_fixture_cases_reads_closing_sermon_fixture() {
        let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../data/detection-fixtures");
        let closing = load_fixture_cases(&base.join("closing-sermon-cases.json")).unwrap();
        assert_eq!(closing.len(), 20);
        assert!(closing.iter().any(|c| c.category == "closing-noise"));
        assert!(closing.iter().any(|c| c.category == "closing-fragment"));

        let probe = load_fixture_cases(&base.join("span-window-probe.json")).unwrap();
        assert_eq!(probe.len(), 6);
        assert!(probe.iter().any(|c| c.category == "probe-mid-window"));
    }

    #[test]
    fn fixture_case_accepts_optional_timestamp_metadata() {
        let json = r#"[
          {
            "language": "en",
            "category": "fixture",
            "timestamp": "01:07:37",
            "text": "Revelation 14:4",
            "mode": "hint",
            "expectedAny": ["Revelation 14:4", "Revelation 14:1"]
          },
          {
            "language": "en",
            "category": "fixture",
            "text": "ordinary speech",
            "mode": "silent"
          }
        ]"#;
        let fixture_cases = serde_json::from_str::<Vec<FixtureCase>>(json).unwrap();
        let cases = fixture_cases
            .into_iter()
            .enumerate()
            .map(|(index, case)| case.into_bench_case(Path::new("fixture.json"), index + 1))
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        assert_eq!(cases[0].timestamp.as_deref(), Some("01:07:37"));
        assert_eq!(cases[1].timestamp, None);
    }

    #[test]
    fn ref_eq_accepts_revelation_of_john_alias() {
        assert!(ref_eq("Revelation 5:12", "Revelation of John 5:12"));
    }

    #[test]
    fn fire_expectation_accepts_any_labeled_reference() {
        let expected = vec!["Daniel 5:23".to_string(), "Daniel 5:22".to_string()];

        assert!(expected_ref_matches(&expected, "Daniel 5:22"));
    }

    #[test]
    fn built_in_policy_keeps_reviewable_paraphrase_as_hint() {
        let case = CASES
            .iter()
            .find(|case| {
                case.1 == "every part of scripture is breathed out by God and useful for teaching"
            })
            .unwrap();

        assert_eq!(built_in_case("en", case).mode, CaseMode::Hint);
    }

    #[test]
    fn built_in_policy_marks_non_unique_paraphrase_as_safe_abstention() {
        let case = CASES
            .iter()
            .find(|case| case.1 == "the joy of the Lord is my strength")
            .unwrap();

        assert_eq!(built_in_case("en", case).mode, CaseMode::Abstain);
    }

    #[test]
    fn built_in_policy_marks_unavailable_afrikaans_quote_as_safe_abstention() {
        let case = AFRIKAANS_CASES
            .iter()
            .find(|case| case.0 == "af-quote")
            .unwrap();

        assert_eq!(built_in_case("af", case).mode, CaseMode::Abstain);
    }

    #[test]
    fn detection_ref_preserves_verse_range() {
        let detection = Detection {
            verse_ref: VerseRef {
                book_number: 66,
                book_name: "Revelation".to_string(),
                chapter: 12,
                verse_start: 7,
                verse_end: Some(12),
            },
            verse_id: None,
            confidence: 1.0,
            source: DetectionSource::DirectReference,
            transcript_snippet: "Revelation 12, 7 to 12".to_string(),
            detected_at: 0,
            is_chapter_only: false,
        };

        assert_eq!(
            detection_ref(&detection, &HashMap::new()).as_deref(),
            Some("Revelation 12:7-12")
        );
    }

    #[test]
    fn percentile_reports_ordered_latency_sample() {
        let samples = [10.0, 20.0, 30.0, 40.0, 50.0];

        assert!((percentile(&samples, 0.50) - 30.0).abs() < f64::EPSILON);
        assert!((percentile(&samples, 0.95) - 50.0).abs() < f64::EPSILON);
        assert!(percentile(&[], 0.95).abs() < f64::EPSILON);
    }

    #[test]
    fn stable_case_replay_can_confirm_a_moderate_semantic_winner() {
        let detection = MergedDetection {
            detection: Detection {
                verse_ref: VerseRef {
                    book_number: 43,
                    book_name: "John".to_string(),
                    chapter: 3,
                    verse_start: 16,
                    verse_end: None,
                },
                verse_id: None,
                confidence: 0.92,
                source: DetectionSource::Semantic { similarity: 0.92 },
                transcript_snippet: "For God so loved the world".to_string(),
                detected_at: 0,
                is_chapter_only: false,
            },
            auto_queued: false,
        };
        let detections = [detection];

        let fired = select_stable_case(
            &mut AutoLiveSelector::default(),
            &detections,
            0.90,
            &HashMap::new(),
        );

        assert!(fired.is_some(), "stable replay must satisfy confirmation");
    }

    #[test]
    fn stable_case_replay_holds_an_ambiguous_semantic_winner() {
        let detection = |verse_start, confidence| MergedDetection {
            detection: Detection {
                verse_ref: VerseRef {
                    book_number: 66,
                    book_name: "Revelation".to_string(),
                    chapter: 21,
                    verse_start,
                    verse_end: None,
                },
                verse_id: None,
                confidence,
                source: DetectionSource::Semantic {
                    similarity: confidence,
                },
                transcript_snippet: "God shall wipe away all tears".to_string(),
                detected_at: 0,
                is_chapter_only: false,
            },
            auto_queued: false,
        };
        let detections = [detection(4, 0.92), detection(17, 0.92)];

        let fired = select_stable_case(
            &mut AutoLiveSelector::default(),
            &detections,
            0.90,
            &HashMap::new(),
        );

        assert!(
            fired.is_none(),
            "ambiguous semantic results stay review-only"
        );
    }

    #[test]
    fn stable_case_replay_counts_a_visible_runner_up_below_auto_live_threshold() {
        let detection = |verse_start, confidence| MergedDetection {
            detection: Detection {
                verse_ref: VerseRef {
                    book_number: 45,
                    book_name: "Romans".to_string(),
                    chapter: 8,
                    verse_start,
                    verse_end: None,
                },
                verse_id: None,
                confidence,
                source: DetectionSource::Semantic {
                    similarity: confidence,
                },
                transcript_snippet: "Nothing can separate us from God's love".to_string(),
                detected_at: 0,
                is_chapter_only: false,
            },
            auto_queued: false,
        };
        let detections = [detection(39, 0.91), detection(38, 0.891)];

        let fired = select_stable_case(
            &mut AutoLiveSelector::default(),
            &detections,
            0.90,
            &HashMap::new(),
        );

        assert!(
            fired.is_none(),
            "a close visible alternative must prevent an ambiguous auto-live"
        );
    }
}
