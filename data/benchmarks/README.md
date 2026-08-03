# Embedding corpus benchmarks

Tracked re-runnable checks for verse embedding composition and retrieval quality.
These replace the session-only scripts that previously lived under gitignored `.tmp/`.

## Requirements

- `data/rhema.db` (from `bun run build:bible` / `setup:all`)
- `models/minilm-l6-v2/tokenizer.json`
- For retrieval A/B: `models/minilm-l6-v2-int8/onnx/model_quantized.onnx`
- Python packages: `tokenizers`, `numpy`, `onnxruntime` (retrieval only)

## Token audit (blend truncation)

```bash
python data/benchmarks/measure_blend_tokens.py
```

Reports mean/p90 token length and truncation rate at 128 for composition variants.
Uses the MiniLM tokenizer with padding/truncation **disabled** so lengths are true.

## Retrieval A/B (composition)

```bash
python data/benchmarks/retrieval_ab_corpus.py
```

Builds small in-memory indexes for blend vs split vs KJV-only on a sample of verses
and reports hit@1 / MRR for English and Spanish query styles.

Not part of default CI (ONNX cost). Run before any change to
`BLENDED_TRANSLATIONS` / `SEPARATE_VECTOR_TRANSLATIONS` or padding policy.
