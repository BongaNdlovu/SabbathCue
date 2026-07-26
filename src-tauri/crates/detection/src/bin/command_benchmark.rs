//! Offline command-classification benchmark and shadow replay.
//!
//! This binary never executes a command. It compares deterministic rules and
//! a trained `MiniLM` head, and optionally queries a `FunctionGemma` model through
//! a local OpenAI-compatible endpoint.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Instant;

use rhema_detection::command_eval::{
    score_predictions, validate_cases, CommandCase, CommandIntent, CommandLabel, CommandMetrics,
    CommandPrediction, DatasetSplit, DeterministicCommandClassifier, LinearCommandHead,
};
use rhema_detection::semantic::embedder::TextEmbedder;
use rhema_detection::OnnxEmbedder;
use serde::{Deserialize, Serialize};

const DEFAULT_CASES: &str = "data/command-classification/command-cases.json";
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
    gemma_url: Option<String>,
    gemma_model: String,
    gemma_model_path: Option<PathBuf>,
    gemma_pid: Option<u32>,
    gemma_startup_ms: Option<f64>,
    curl: PathBuf,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BenchmarkReport {
    corpus: CorpusSummary,
    runners: Vec<RunnerReport>,
    disagreements: Vec<Disagreement>,
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
    gemma: Option<CommandPrediction>,
    models_disagree: bool,
}

#[derive(Debug)]
struct RunnerOutcome {
    report: RunnerReport,
    predictions: BTreeMap<String, CommandPrediction>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Disagreement {
    id: String,
    text: String,
    expected: CommandLabel,
    minilm: CommandPrediction,
    gemma: CommandPrediction,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("command benchmark failed: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let options = Options::parse(&args)?;
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

    let gemma = options
        .gemma_url
        .as_deref()
        .map(|url| {
            run_gemma(
                &evaluation_cases,
                &GemmaClient {
                    url,
                    model: &options.gemma_model,
                    curl: &options.curl,
                },
                options.gemma_model_path.as_deref(),
                options.gemma_pid,
                options.gemma_startup_ms,
            )
        })
        .transpose()?;

    let disagreements = gemma.as_ref().map_or_else(Vec::new, |gemma_outcome| {
        collect_disagreements(&evaluation_cases, &minilm, gemma_outcome)
    });
    let recommendation = recommendation(&minilm.report, gemma.as_ref().map(|value| &value.report));
    let mut runners = vec![deterministic.report, minilm.report];
    if let Some(outcome) = &gemma {
        runners.push(outcome.report.clone());
    }
    let report = BenchmarkReport {
        corpus,
        runners,
        disagreements,
        recommendation,
    };
    write_json(&options.report, &report)?;
    print_summary(&report, &options.report, &options.head_output);

    if let Some(input) = &options.shadow_input {
        run_shadow_replay(
            input,
            &options.shadow_output,
            &embedder,
            &head,
            gemma.as_ref().map(|_| GemmaClient {
                url: options.gemma_url.as_deref().unwrap_or_default(),
                model: &options.gemma_model,
                curl: &options.curl,
            }),
        )?;
    }
    Ok(())
}

impl Options {
    fn parse(args: &[String]) -> Result<Self, String> {
        if args.iter().any(|value| value == "--help" || value == "-h") {
            println!(
                "Usage: command_benchmark [options]\n\
                 --cases PATH              labeled JSON corpus\n\
                 --model PATH              MiniLM ONNX model\n\
                 --tokenizer PATH          MiniLM tokenizer\n\
                 --head-output PATH        serialized MiniLM head\n\
                 --report PATH             JSON benchmark report\n\
                 --shadow-input PATH       optional transcript text file\n\
                 --shadow-output PATH      non-executing prediction report\n\
                 --gemma-url URL           optional llama-server /v1/chat/completions URL\n\
                 --gemma-model NAME        model field sent to the endpoint\n\
                 --gemma-model-path PATH   optional artifact size measurement\n\
                 --gemma-pid PID           optional worker memory measurement\n\
                 --gemma-startup-ms MS     optional measured server startup\n\
                 --curl PATH               curl executable"
            );
            std::process::exit(0);
        }
        Ok(Self {
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
            gemma_url: value(args, "--gemma-url").map(str::to_string),
            gemma_model: value(args, "--gemma-model")
                .unwrap_or("functiongemma")
                .to_string(),
            gemma_model_path: value(args, "--gemma-model-path").map(PathBuf::from),
            gemma_pid: value(args, "--gemma-pid")
                .map(str::parse)
                .transpose()
                .map_err(|error| format!("invalid --gemma-pid: {error}"))?,
            gemma_startup_ms: value(args, "--gemma-startup-ms")
                .map(str::parse)
                .transpose()
                .map_err(|error| format!("invalid --gemma-startup-ms: {error}"))?,
            curl: value(args, "--curl").map_or_else(|| PathBuf::from("curl"), PathBuf::from),
        })
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
        predictions,
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
        predictions,
        &latencies,
        0,
        0,
        Some(startup_ms),
        Some(serialized_head.len() as u64),
        None,
    ))
}

#[derive(Debug, Clone, Copy)]
struct GemmaClient<'a> {
    url: &'a str,
    model: &'a str,
    curl: &'a Path,
}

fn run_gemma(
    cases: &[CommandCase],
    client: &GemmaClient<'_>,
    model_path: Option<&Path>,
    pid: Option<u32>,
    startup_ms: Option<f64>,
) -> Result<RunnerOutcome, String> {
    let mut predictions = BTreeMap::new();
    let mut latencies = Vec::with_capacity(cases.len());
    let mut invalid_outputs = 0;
    let mut failed_requests = 0;

    for case in cases {
        let started = Instant::now();
        let prediction = match client.predict(&case.text) {
            Ok(prediction) => {
                if prediction.label.is_valid() {
                    prediction
                } else {
                    invalid_outputs += 1;
                    invalid_prediction(prediction.raw)
                }
            }
            Err(error) => {
                failed_requests += 1;
                eprintln!("FunctionGemma case {} failed: {error}", case.id);
                invalid_prediction(Some(error))
            }
        };
        latencies.push(duration_ms(started.elapsed()));
        predictions.insert(case.id.clone(), prediction);
    }

    let artifact_bytes = model_path
        .map(std::fs::metadata)
        .transpose()
        .map_err(|error| format!("read FunctionGemma model metadata: {error}"))?
        .map(|metadata| metadata.len());
    Ok(outcome_from_predictions(
        "functiongemma",
        cases,
        predictions,
        &latencies,
        invalid_outputs,
        failed_requests,
        startup_ms,
        artifact_bytes,
        pid.and_then(process_working_set_mib),
    ))
}

impl GemmaClient<'_> {
    fn predict(&self, text: &str) -> Result<CommandPrediction, String> {
        let payload = gemma_payload(self.model, text);
        let mut child = Command::new(self.curl)
            .args([
                "--silent",
                "--show-error",
                "--fail-with-body",
                "--max-time",
                "30",
                "-H",
                "Content-Type: application/json",
                "--data-binary",
                "@-",
                self.url,
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| format!("start curl: {error}"))?;
        child
            .stdin
            .take()
            .ok_or_else(|| "curl stdin unavailable".to_string())?
            .write_all(payload.to_string().as_bytes())
            .map_err(|error| format!("write FunctionGemma request: {error}"))?;
        let output = child
            .wait_with_output()
            .map_err(|error| format!("wait for FunctionGemma request: {error}"))?;
        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
        }
        let raw = String::from_utf8(output.stdout)
            .map_err(|error| format!("FunctionGemma response is not UTF-8: {error}"))?;
        parse_gemma_response(&raw)
    }
}

fn gemma_payload(model: &str, text: &str) -> serde_json::Value {
    let tools = [
        tool(
            "hide_output",
            "Hide or clear the current projected content",
            None,
        ),
        tool(
            "show_output",
            "Show or restore the current projected content",
            None,
        ),
        tool(
            "next_item",
            "Advance to the next verse or presentation item",
            None,
        ),
        tool(
            "previous_item",
            "Return to the previous verse or presentation item",
            None,
        ),
        tool(
            "switch_translation",
            "Change the Bible translation",
            Some(serde_json::json!({
                "translation": {
                    "type": "string",
                    "enum": ["NIV", "ESV", "KJV", "NKJV", "NLT", "NET", "AMP", "MSG", "SPARV", "AFR83"]
                }
            })),
        ),
    ];
    serde_json::json!({
        "model": model,
        "temperature": 0,
        "max_tokens": 48,
        "messages": [
            {
                "role": "system",
                "content": "Classify only explicit operator commands. Ordinary sermon speech must produce no tool call. Never infer or emit a Bible reference."
            },
            {"role": "user", "content": text}
        ],
        "tools": tools,
        "tool_choice": "auto"
    })
}

fn tool(name: &str, description: &str, properties: Option<serde_json::Value>) -> serde_json::Value {
    let properties = properties.unwrap_or_else(|| serde_json::json!({}));
    let required = if name == "switch_translation" {
        serde_json::json!(["translation"])
    } else {
        serde_json::json!([])
    };
    serde_json::json!({
        "type": "function",
        "function": {
            "name": name,
            "description": description,
            "parameters": {
                "type": "object",
                "properties": properties,
                "required": required,
                "additionalProperties": false
            }
        }
    })
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChatMessage,
}

#[derive(Debug, Deserialize)]
struct ChatMessage {
    content: Option<String>,
    #[serde(default)]
    tool_calls: Vec<ToolCall>,
}

#[derive(Debug, Deserialize)]
struct ToolCall {
    function: ToolFunction,
}

#[derive(Debug, Deserialize)]
struct ToolFunction {
    name: String,
    arguments: serde_json::Value,
}

fn parse_gemma_response(raw: &str) -> Result<CommandPrediction, String> {
    let response: ChatResponse =
        serde_json::from_str(raw).map_err(|error| format!("parse response JSON: {error}"))?;
    let message = &response
        .choices
        .first()
        .ok_or_else(|| "response contains no choices".to_string())?
        .message;

    if message.tool_calls.is_empty() {
        return Ok(CommandPrediction {
            label: CommandLabel::intent(CommandIntent::None),
            confidence: 1.0,
            raw: message.content.clone(),
        });
    }
    if message.tool_calls.len() != 1 {
        return Err(format!(
            "expected at most one tool call, received {}",
            message.tool_calls.len()
        ));
    }

    let function = &message.tool_calls[0].function;
    let arguments = if let Some(serialized) = function.arguments.as_str() {
        serde_json::from_str(serialized)
            .map_err(|error| format!("parse tool arguments JSON: {error}"))?
    } else {
        function.arguments.clone()
    };
    let label = match function.name.as_str() {
        "hide_output" => CommandLabel::intent(CommandIntent::Hide),
        "show_output" => CommandLabel::intent(CommandIntent::Show),
        "next_item" => CommandLabel::intent(CommandIntent::Next),
        "previous_item" => CommandLabel::intent(CommandIntent::Previous),
        "switch_translation" => {
            let translation = arguments
                .get("translation")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| "switch_translation is missing translation".to_string())?;
            CommandLabel::translation(translation)
        }
        other => return Err(format!("unknown tool call: {other}")),
    };
    if !label.is_valid() {
        return Err(format!("tool call has invalid arguments: {arguments}"));
    }
    Ok(CommandPrediction {
        label,
        confidence: 1.0,
        raw: Some(raw.to_string()),
    })
}

fn collect_disagreements(
    cases: &[CommandCase],
    minilm: &RunnerOutcome,
    gemma: &RunnerOutcome,
) -> Vec<Disagreement> {
    cases
        .iter()
        .filter_map(|case| {
            let minilm_prediction = minilm.predictions.get(&case.id)?;
            let gemma_prediction = gemma.predictions.get(&case.id)?;
            (minilm_prediction.label != gemma_prediction.label).then(|| Disagreement {
                id: case.id.clone(),
                text: case.text.clone(),
                expected: case.expected.clone(),
                minilm: minilm_prediction.clone(),
                gemma: gemma_prediction.clone(),
            })
        })
        .collect()
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
    predictions: BTreeMap<String, CommandPrediction>,
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
        predictions,
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

fn process_working_set_mib(pid: u32) -> Option<f64> {
    #[cfg(target_os = "windows")]
    {
        let command = format!("(Get-Process -Id {pid} -ErrorAction Stop).WorkingSet64");
        let output = Command::new("powershell")
            .args(["-NoProfile", "-Command", &command])
            .output()
            .ok()?;
        let bytes = String::from_utf8(output.stdout)
            .ok()?
            .trim()
            .parse::<f64>()
            .ok()?;
        Some(bytes / 1_048_576.0)
    }
    #[cfg(not(target_os = "windows"))]
    {
        let status = std::fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
        let kib = status
            .lines()
            .find(|line| line.starts_with("VmRSS:"))?
            .split_whitespace()
            .nth(1)?
            .parse::<f64>()
            .ok()?;
        Some(kib / 1024.0)
    }
}

fn recommendation(minilm: &RunnerReport, gemma: Option<&RunnerReport>) -> String {
    let Some(gemma) = gemma else {
        return "FunctionGemma was not evaluated. Start a local endpoint and rerun before choosing a production classifier.".into();
    };
    if gemma.failed_requests > 0 || gemma.invalid_outputs > 0 {
        return "Do not adopt FunctionGemma yet: the run contained failed or invalid model responses.".into();
    }
    if gemma.safety.false_commands > minilm.safety.false_commands {
        return "Prefer MiniLM: FunctionGemma produced more false commands on the safety corpus."
            .into();
    }
    if gemma.test.macro_f1 >= minilm.test.macro_f1 + 0.05 {
        return "FunctionGemma cleared the provisional five-point hard-command improvement gate without a safety regression; proceed to controlled in-app shadow testing.".into();
    }
    "Prefer MiniLM for now: FunctionGemma did not improve test macro-F1 by the provisional five-point margin.".into()
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
    gemma: Option<GemmaClient<'_>>,
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
            let gemma_prediction = gemma
                .map(|client| client.predict(text))
                .transpose()
                .map_err(|error| format!("shadow FunctionGemma line {}: {error}", index + 1))?;
            let models_disagree = gemma_prediction
                .as_ref()
                .is_some_and(|prediction| prediction.label != minilm.label);
            Ok(ShadowRow {
                line: index + 1,
                text: text.to_string(),
                deterministic: deterministic_prediction,
                minilm,
                gemma: gemma_prediction,
                models_disagree,
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
    fn gemma_response_accepts_one_known_tool() {
        let response = r#"{
          "choices": [{
            "message": {
              "content": null,
              "tool_calls": [{
                "function": {"name": "hide_output", "arguments": {}}
              }]
            }
          }]
        }"#;

        let prediction = parse_gemma_response(response).unwrap();

        assert_eq!(prediction.label.intent, CommandIntent::Hide);
    }

    #[test]
    fn gemma_response_rejects_unknown_translation() {
        let response = r#"{
          "choices": [{
            "message": {
              "content": null,
              "tool_calls": [{
                "function": {
                  "name": "switch_translation",
                  "arguments": {"translation": "MADE_UP"}
                }
              }]
            }
          }]
        }"#;

        let error = parse_gemma_response(response).unwrap_err();

        assert!(error.contains("invalid"));
    }

    #[test]
    fn gemma_response_treats_no_tool_as_none() {
        let response = r#"{"choices":[{"message":{"content":"NONE","tool_calls":[]}}]}"#;

        let prediction = parse_gemma_response(response).unwrap();

        assert_eq!(prediction.label.intent, CommandIntent::None);
    }

    #[test]
    fn percentile_uses_nearest_rank() {
        let values = [10.0, 20.0, 30.0, 40.0, 50.0];

        assert!((percentile(&values, 0.95) - 50.0).abs() < f64::EPSILON);
    }

    #[test]
    fn options_accept_measured_gemma_startup() {
        let args = vec!["--gemma-startup-ms".to_string(), "412.5".to_string()];

        let options = Options::parse(&args).unwrap();

        assert_eq!(options.gemma_startup_ms, Some(412.5));
    }
}
