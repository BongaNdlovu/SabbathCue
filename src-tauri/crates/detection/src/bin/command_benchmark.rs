//! Offline command-classification benchmark and shadow replay.
//!
//! This binary never executes a command. It compares deterministic rules and
//! a trained `MiniLM` head against isolated evaluation partitions.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use rhema_detection::command_eval::{
    score_predictions, validate_cases, CommandCase, CommandIntent, CommandLabel, CommandMetrics,
    CommandPrediction, DatasetSplit, DeterministicCommandClassifier, LinearCommandHead,
};
use rhema_detection::semantic::embedder::TextEmbedder;
use rhema_detection::OnnxEmbedder;
use serde::Serialize;

const DEFAULT_CASES: &str = "data/command-classification/command-cases.generated.json";
const DEFAULT_MODEL: &str = "models/minilm-l6-v2-int8/onnx/model_quantized.onnx";
const DEFAULT_TOKENIZER: &str = "models/minilm-l6-v2/tokenizer.json";
const DEFAULT_HEAD: &str = "src-tauri/target/minilm-command-head.json";
const DEFAULT_REPORT: &str = "src-tauri/target/command-benchmark-report.json";
const DEFAULT_SHADOW_REPORT: &str = "src-tauri/target/command-shadow-report.json";

#[derive(Debug)]
struct Options {
    cases: PathBuf,
    model: PathBuf,
    tokenizer: PathBuf,
    head_output: PathBuf,
    report: PathBuf,
    shadow_input: Option<PathBuf>,
    shadow_output: PathBuf,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BenchmarkReport {
    corpus: CorpusSummary,
    runners: Vec<RunnerReport>,
    recommendation: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CorpusSummary {
    path: PathBuf,
    train: usize,
    validation: usize,
    test: usize,
    safety: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct RunnerReport {
    name: String,
    test: CommandMetrics,
    safety: CommandMetrics,
    latency: LatencySummary,
    invalid_outputs: usize,
    failed_requests: usize,
    startup_ms: Option<f64>,
    artifact_bytes: Option<u64>,
    working_set_mib: Option<f64>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct LatencySummary {
    samples: usize,
    p50_ms: f64,
    p95_ms: f64,
    maximum_ms: f64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ShadowRow {
    line: usize,
    text: String,
    deterministic: CommandPrediction,
    minilm: CommandPrediction,
}

#[derive(Debug)]
struct RunnerOutcome {
    report: RunnerReport,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("command benchmark failed: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let options = Options::parse(&args);
    let cases = load_cases(&options.cases)?;
    validate_cases(&cases)?;
    let corpus = summarize_corpus(&options.cases, &cases);
    let evaluation_cases = evaluation_cases(&cases);

    let deterministic = run_deterministic(&evaluation_cases);

    let startup = Instant::now();
    let embedder = OnnxEmbedder::load(&options.model, &options.tokenizer)
        .map_err(|error| format!("load MiniLM: {error}"))?;
    let startup_ms = duration_ms(startup.elapsed());
    let case_embeddings = embed_cases(&embedder, &cases)?;
    let head = train_head(&case_embeddings, &cases)?;
    write_json(&options.head_output, &head)?;
    let minilm = run_minilm(&head, &evaluation_cases, &case_embeddings, startup_ms)?;

    let recommendation = recommendation(&minilm.report);
    let report = BenchmarkReport {
        corpus,
        runners: vec![deterministic.report, minilm.report],
        recommendation,
    };
    write_json(&options.report, &report)?;
    print_summary(&report, &options.report, &options.head_output);

    if let Some(input) = &options.shadow_input {
        run_shadow_replay(input, &options.shadow_output, &embedder, &head)?;
    }
    Ok(())
}

impl Options {
    fn parse(args: &[String]) -> Self {
        if args.iter().any(|value| value == "--help" || value == "-h") {
            println!(
                "Usage: command_benchmark [options]\n\
                 --cases PATH              labeled JSON corpus\n\
                 --model PATH              MiniLM ONNX model\n\
                 --tokenizer PATH          MiniLM tokenizer\n\
                 --head-output PATH        serialized MiniLM head\n\
                 --report PATH             JSON benchmark report\n\
                 --shadow-input PATH       optional transcript text file\n\
                 --shadow-output PATH      non-executing prediction report"
            );
            std::process::exit(0);
        }
        Self {
            cases: value(args, "--cases")
                .map_or_else(|| PathBuf::from(DEFAULT_CASES), PathBuf::from),
            model: value(args, "--model")
                .map_or_else(|| PathBuf::from(DEFAULT_MODEL), PathBuf::from),
            tokenizer: value(args, "--tokenizer")
                .map_or_else(|| PathBuf::from(DEFAULT_TOKENIZER), PathBuf::from),
            head_output: value(args, "--head-output")
                .map_or_else(|| PathBuf::from(DEFAULT_HEAD), PathBuf::from),
            report: value(args, "--report")
                .map_or_else(|| PathBuf::from(DEFAULT_REPORT), PathBuf::from),
            shadow_input: value(args, "--shadow-input").map(PathBuf::from),
            shadow_output: value(args, "--shadow-output")
                .map_or_else(|| PathBuf::from(DEFAULT_SHADOW_REPORT), PathBuf::from),
        }
    }
}

fn value<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter()
        .position(|argument| argument == name)
        .and_then(|index| args.get(index + 1))
        .map(String::as_str)
}

fn load_cases(path: &Path) -> Result<Vec<CommandCase>, String> {
    let json = std::fs::read_to_string(path)
        .map_err(|error| format!("read command corpus {}: {error}", path.display()))?;
    serde_json::from_str(&json)
        .map_err(|error| format!("parse command corpus {}: {error}", path.display()))
}

fn summarize_corpus(path: &Path, cases: &[CommandCase]) -> CorpusSummary {
    CorpusSummary {
        path: path.to_path_buf(),
        train: count_split(cases, DatasetSplit::Train),
        validation: count_split(cases, DatasetSplit::Validation),
        test: count_split(cases, DatasetSplit::Test),
        safety: count_split(cases, DatasetSplit::Safety),
    }
}

fn count_split(cases: &[CommandCase], split: DatasetSplit) -> usize {
    cases.iter().filter(|case| case.split == split).count()
}

fn evaluation_cases(cases: &[CommandCase]) -> Vec<CommandCase> {
    cases
        .iter()
        .filter(|case| matches!(case.split, DatasetSplit::Test | DatasetSplit::Safety))
        .cloned()
        .collect()
}

fn run_deterministic(cases: &[CommandCase]) -> RunnerOutcome {
    let classifier = DeterministicCommandClassifier;
    let mut latencies = Vec::with_capacity(cases.len());
    let predictions = cases
        .iter()
        .map(|case| {
            let started = Instant::now();
            let prediction = classifier.predict(&case.text);
            latencies.push(duration_ms(started.elapsed()));
            (case.id.clone(), prediction)
        })
        .collect::<BTreeMap<_, _>>();
    outcome_from_predictions(
        "deterministic",
        cases,
        &predictions,
        &latencies,
        0,
        0,
        None,
        None,
        None,
    )
}

fn embed_cases(
    embedder: &dyn TextEmbedder,
    cases: &[CommandCase],
) -> Result<BTreeMap<String, (Vec<f32>, f64)>, String> {
    cases
        .iter()
        .map(|case| {
            let started = Instant::now();
            let embedding = embedder
                .embed(&case.text)
                .map_err(|error| format!("embed case {}: {error}", case.id))?;
            Ok((case.id.clone(), (embedding, duration_ms(started.elapsed()))))
        })
        .collect()
}

fn train_head(
    embedded: &BTreeMap<String, (Vec<f32>, f64)>,
    cases: &[CommandCase],
) -> Result<LinearCommandHead, String> {
    let samples = |split| {
        cases
            .iter()
            .filter(|case| case.split == split)
            .map(|case| {
                embedded
                    .get(&case.id)
                    .map(|(embedding, _)| (embedding.clone(), case.expected.intent))
                    .ok_or_else(|| format!("missing embedding for {}", case.id))
            })
            .collect::<Result<Vec<_>, _>>()
    };
    let train = samples(DatasetSplit::Train)?;
    let validation = samples(DatasetSplit::Validation)?;
    LinearCommandHead::train(&train, &validation)
        .map_err(|error| format!("train MiniLM command head: {error}"))
}

fn run_minilm(
    head: &LinearCommandHead,
    cases: &[CommandCase],
    embedded: &BTreeMap<String, (Vec<f32>, f64)>,
    startup_ms: f64,
) -> Result<RunnerOutcome, String> {
    let mut latencies = Vec::with_capacity(cases.len());
    let predictions = cases
        .iter()
        .map(|case| {
            let (embedding, latency) = embedded
                .get(&case.id)
                .ok_or_else(|| format!("missing embedding for {}", case.id))?;
            latencies.push(*latency);
            let prediction = head
                .predict_embedding_for_text(embedding, &case.text)
                .map_err(|error| format!("classify case {}: {error}", case.id))?;
            Ok((case.id.clone(), prediction))
        })
        .collect::<Result<BTreeMap<_, _>, String>>()?;
    let serialized_head = serde_json::to_vec(head)
        .map_err(|error| format!("serialize MiniLM command head: {error}"))?;
    Ok(outcome_from_predictions(
        "minilm-linear-head",
        cases,
        &predictions,
        &latencies,
        0,
        0,
        Some(startup_ms),
        Some(serialized_head.len() as u64),
        None,
    ))
}

fn invalid_prediction(raw: Option<String>) -> CommandPrediction {
    CommandPrediction {
        label: CommandLabel::intent(CommandIntent::None),
        confidence: 0.0,
        raw,
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "benchmark report assembly keeps runner measurements explicit"
)]
fn outcome_from_predictions(
    name: &str,
    cases: &[CommandCase],
    predictions: &BTreeMap<String, CommandPrediction>,
    latencies: &[f64],
    invalid_outputs: usize,
    failed_requests: usize,
    startup_ms: Option<f64>,
    artifact_bytes: Option<u64>,
    working_set_mib: Option<f64>,
) -> RunnerOutcome {
    let cases_for = |split| {
        cases
            .iter()
            .filter(|case| case.split == split)
            .cloned()
            .collect::<Vec<_>>()
    };
    let predictions_for = |selected: &[CommandCase]| {
        selected
            .iter()
            .map(|case| {
                predictions
                    .get(&case.id)
                    .cloned()
                    .unwrap_or_else(|| invalid_prediction(Some("missing prediction".into())))
            })
            .collect::<Vec<_>>()
    };
    let test_cases = cases_for(DatasetSplit::Test);
    let safety_cases = cases_for(DatasetSplit::Safety);
    let test_predictions = predictions_for(&test_cases);
    let safety_predictions = predictions_for(&safety_cases);

    RunnerOutcome {
        report: RunnerReport {
            name: name.to_string(),
            test: score_predictions(&test_cases, &test_predictions),
            safety: score_predictions(&safety_cases, &safety_predictions),
            latency: summarize_latency(latencies),
            invalid_outputs,
            failed_requests,
            startup_ms,
            artifact_bytes,
            working_set_mib,
        },
    }
}

fn summarize_latency(samples: &[f64]) -> LatencySummary {
    if samples.is_empty() {
        return LatencySummary::default();
    }
    let mut sorted = samples.to_vec();
    sorted.sort_by(f64::total_cmp);
    LatencySummary {
        samples: sorted.len(),
        p50_ms: percentile(&sorted, 0.50),
        p95_ms: percentile(&sorted, 0.95),
        maximum_ms: sorted.last().copied().unwrap_or_default(),
    }
}

#[expect(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "small benchmark sample index is bounded by the vector length"
)]
fn percentile(sorted: &[f64], percentile: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let index = ((sorted.len() - 1) as f64 * percentile.clamp(0.0, 1.0)).ceil() as usize;
    sorted[index]
}

fn duration_ms(duration: std::time::Duration) -> f64 {
    duration.as_secs_f64() * 1_000.0
}

fn recommendation(minilm: &RunnerReport) -> String {
    if minilm.safety.false_commands > 0 {
        return "Keep MiniLM in non-executing shadow mode: the safety corpus contains false commands."
            .into();
    }
    "MiniLM cleared the authored seed safety gate; continue non-executing validation on real multi-speaker transcripts before enabling commands.".into()
}

fn print_summary(report: &BenchmarkReport, report_path: &Path, head_path: &Path) {
    println!(
        "Command corpus: train={} validation={} test={} safety={}",
        report.corpus.train, report.corpus.validation, report.corpus.test, report.corpus.safety
    );
    for runner in &report.runners {
        println!(
            "{}: test accuracy={:.1}% macro-F1={:.1}% safety false commands={} p95={:.2}ms",
            runner.name,
            runner.test.accuracy * 100.0,
            runner.test.macro_f1 * 100.0,
            runner.safety.false_commands,
            runner.latency.p95_ms
        );
    }
    println!("Decision: {}", report.recommendation);
    println!("Report: {}", report_path.display());
    println!("MiniLM head: {}", head_path.display());
}

fn run_shadow_replay(
    input: &Path,
    output: &Path,
    embedder: &dyn TextEmbedder,
    head: &LinearCommandHead,
) -> Result<(), String> {
    let text = std::fs::read_to_string(input)
        .map_err(|error| format!("read shadow input {}: {error}", input.display()))?;
    let deterministic = DeterministicCommandClassifier;
    let rows = text
        .lines()
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty())
        .map(|(index, text)| {
            let deterministic_prediction = deterministic.predict(text);
            let minilm = head
                .predict_text(embedder, text)
                .map_err(|error| format!("shadow MiniLM line {}: {error}", index + 1))?;
            Ok(ShadowRow {
                line: index + 1,
                text: text.to_string(),
                deterministic: deterministic_prediction,
                minilm,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    write_json(output, &rows)
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("create directory {}: {error}", parent.display()))?;
    }
    let json = serde_json::to_string_pretty(value)
        .map_err(|error| format!("serialize {}: {error}", path.display()))?;
    std::fs::write(path, format!("{json}\n"))
        .map_err(|error| format!("write {}: {error}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentile_uses_nearest_rank() {
        let values = [10.0, 20.0, 30.0, 40.0, 50.0];

        assert!((percentile(&values, 0.95) - 50.0).abs() < f64::EPSILON);
    }
}
