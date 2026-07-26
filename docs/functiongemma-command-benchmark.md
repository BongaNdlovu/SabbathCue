# FunctionGemma command-classification experiment

Last verified: 2026-07-26

This experiment compares three non-executing command classifiers against one
labeled corpus:

1. high-precision deterministic phrases;
2. a linear classification head trained over the bundled MiniLM ONNX model;
3. optional FunctionGemma responses from a loopback llama.cpp server.

The experiment is not registered with Tauri, cannot perform an operator
command, and adds no model or runtime to the installer. Receipts:
`src-tauri/crates/detection/src/bin/command_benchmark.rs:1`,
`src-tauri/tauri.conf.json:41`.

## Corpus and decision rule

`data/command-classification/command-cases.json` has isolated train,
validation, test, and safety partitions. A family cannot cross partitions.
Safety cases are ordinary sermon speech and must always predict `none`.
Receipts: `src-tauri/crates/detection/src/command_eval.rs`,
`src-tauri/crates/detection/tests/command_corpus.rs`.

The provisional FunctionGemma adoption gate is:

- no failed or structurally invalid responses;
- no safety false-command regression compared with MiniLM;
- at least five percentage points more test macro-F1 than MiniLM.

These gates are an experiment default, not a production approval. A release
decision requires a much larger, multi-speaker held-out transcript corpus.

The verified 2026-07-26 seed-corpus run produced:

| Runner | Test accuracy | Test macro-F1 | Safety false commands | p95 |
|---|---:|---:|---:|---:|
| Deterministic | 16.7% | 4.8% | 0 / 30 | under 0.01 ms |
| MiniLM linear head + command-shape gate | 66.7% | 68.9% | 0 / 30 | 16.48 ms |
| FunctionGemma 270M Q8 | 38.9% | 36.6% | 22 / 30 | 1,175.09 ms |

The MiniLM path now abstains before intent output when an utterance is not
shaped like an explicit operator request. This removed all four seed-corpus
safety false commands without changing held-out test accuracy or macro-F1.
The authored corpus remains too small for production command execution; the
gate and classifier are still benchmark/shadow-only.
After correcting the FunctionGemma activation message, native-call parsing, and
response stop sequence, the model still missed the adoption gate: it produced
five failed responses, fired on 22 ordinary sermon phrases, and remained over
one second at p95. The Q8 artifact measured 291,557,792 bytes and the worker used
about 375 MiB. Neither classifier is connected to command execution.
Receipt: `src-tauri/target/command-benchmark-report.json` from
`npm.cmd run benchmark:commands:gemma` (machine-local ignored output).

## Run the MiniLM baseline

From the repository root:

```powershell
npm.cmd run benchmark:commands
```

The command writes:

- `src-tauri/target/minilm-command-head.json` — the small trained
  classification head;
- `src-tauri/target/command-benchmark-report.json` — ignored machine-local
  measurements and the provisional recommendation.

To replay a controlled transcript without executing predictions:

```powershell
cargo run --manifest-path src-tauri/Cargo.toml -p rhema-detection `
  --features onnx --release --bin command_benchmark -- `
  --shadow-input C:\absolute\path\to\transcript-lines.txt
```

Each non-empty input line is classified independently. The resulting
`src-tauri/target/command-shadow-report.json` records deterministic and MiniLM
predictions and, when enabled, FunctionGemma disagreements.

## Run FunctionGemma

FunctionGemma is gated by Google's Gemma license. The account owner must accept
the terms at:

<https://huggingface.co/google/functiongemma-270m-it>

Then set a read-capable Hugging Face token only in the current PowerShell
session:

```powershell
$credential = Get-Credential -UserName token -Message "Paste a read-capable Hugging Face token"
$env:HF_TOKEN = $credential.GetNetworkCredential().Password
```

This keeps the token out of terminal history and sets it only for the current
PowerShell process. Never put the token in a command committed to the
repository.

Run:

```powershell
npm.cmd run benchmark:commands:gemma
```

`scripts/setup-functiongemma-benchmark.ps1` downloads the official Windows
CPU llama.cpp runtime and Q8 FunctionGemma artifact into ignored `.tmp`
storage, verifies their published SHA-256 hashes, retries failed or
incomplete downloads with automatic cleanup, and refuses to continue
without an authenticated token. `scripts/run-functiongemma-benchmark.ps1`
starts a hidden loopback-only server with two inference threads and a
512-token context, waits for readiness, runs the same corpus, measures model
file size and worker memory, and always terminates the worker.

No downloaded experiment asset appears in `src-tauri/tauri.conf.json`.

## Prepare supervised fine-tuning data

Export FunctionGemma conversational tool-call records:

```powershell
npm.cmd run export:functiongemma-training
npm.cmd run test:functiongemma-experiment
```

The generated JSONL files are written under
`src-tauri/target/functiongemma-training`. They follow Google's documented
`messages` plus `tools` structure and retain `none` examples as assistant
responses without tool calls.

Training itself should run in the official FunctionGemma Colab/Kaggle workflow
or another controlled GPU environment. Do not fine-tune on test or safety
partitions. After exporting the fine-tuned checkpoint to GGUF, start the same
local server with that artifact and rerun this benchmark; the scoring contract
does not change.

Official references:

- <https://ai.google.dev/gemma/docs/functiongemma/finetuning-with-functiongemma>
- <https://huggingface.co/ggml-org/functiongemma-270m-it-GGUF>
- <https://github.com/ggml-org/llama.cpp/releases>
