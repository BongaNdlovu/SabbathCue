# MiniLM command-classification experiment

Last verified: 2026-07-26

This non-executing experiment trains a small linear command head over the
bundled MiniLM ONNX model and compares it with high-precision deterministic
phrases. It is not registered with Tauri and cannot execute operator commands.
Receipts: `src-tauri/crates/detection/src/bin/command_benchmark.rs:1`,
`src-tauri/tauri.conf.json:41`.

## Synthetic training transcripts

`data/command-classification/generate-command-transcripts.mjs` deterministically
generates 100 synthetic sermon transcripts from 50 synthetic speakers:

- 80 transcripts / 40 speakers for training;
- 20 transcripts / 10 speakers for validation;
- 12 ordinary sermon utterances and five operator commands per transcript;
- one ordinary utterance and one rotating command sampled from every transcript
  for the benchmark corpus;
- 16 synthetic training examples for each supported command intent, alongside
  the original authored training and validation cases.

Synthetic speakers never cross partitions. The generator then appends only the
authored cases from `data/command-classification/command-cases.json`.
Synthetic examples cannot enter the final test or safety evaluation. The full
1,700-utterance transcript set is stored in
`data/command-classification/synthetic-command-transcripts.json`; the sampled
298-case benchmark corpus is stored in
`data/command-classification/command-cases.generated.json`.

Generate and verify it with:

```powershell
npm.cmd run generate:command-transcripts
npm.cmd run test:command-classifier
```

Synthetic transcripts cover deterministic vocabulary, paraphrases, sermon-like
collisions, and a small number of spelling/STT-style errors. They do not model
real microphones, accents, speakers, congregations, or speech-to-text provider
behavior. Real multi-speaker transcripts remain required before command
execution can be considered.

## Benchmark

Run from the repository root:

```powershell
npm.cmd run benchmark:commands
```

The npm pre-hook regenerates the corpus before the Rust benchmark. The benchmark
writes:

- `src-tauri/target/minilm-command-head.json` — the trained linear head;
- `src-tauri/target/command-benchmark-report.json` — ignored machine-local
  metrics and the safety recommendation.

The 2026-07-26 controlled run produced 83.3% authored test accuracy, 77.8%
macro-F1, zero false commands across 30 authored safety cases, and 9.27 ms p95
latency. The earlier authored-only MiniLM run produced 66.7% accuracy and 68.9%
macro-F1, so the balanced synthetic training sample improved the held-out seed
result without weakening its safety score.

MiniLM predictions also pass through a conservative command-shape gate. The gate
requires a presentation-specific operator request and abstains on declarative or
negated sermon speech.

## Shadow replay

Replay transcript lines without executing predictions:

```powershell
cargo run --manifest-path src-tauri/Cargo.toml -p rhema-detection `
  --features onnx --release --bin command_benchmark -- `
  --shadow-input C:\absolute\path\to\transcript-lines.txt
```

Each non-empty line is classified independently. Results are written to
`src-tauri/target/command-shadow-report.json`.

This experiment remains disconnected from command execution until it passes a
fresh, held-out, multi-church transcript evaluation.
