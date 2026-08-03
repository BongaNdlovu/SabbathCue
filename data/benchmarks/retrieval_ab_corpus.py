"""Controlled retrieval A/B: corpus composition vs English hit quality.

Uses the real all-MiniLM-L6-v2 ONNX (INT8) + MiniLM tokenizer + rhema.db.
Builds small in-memory indexes for three document compositions on the same
verse sample, then queries with:
  - KJV verbatim (should be trivial for KJV-containing docs)
  - WEB modern English (same verse_id target)
  - short English paraphrase-ish snippets built by lowercasing/ truncating KJV
    (weak proxy for spoken paraphrase — labeled as such)

Reports hit@1 / hit@5 / MRR for target verse_id.
This is NOT the full detection_accuracy harness; it isolates the vector index
leg under composition changes.
"""
from __future__ import annotations

import random
import sqlite3
import time
from pathlib import Path

import numpy as np
import onnxruntime as ort
from tokenizers import Tokenizer

ROOT = Path(__file__).resolve().parents[2]
DB = ROOT / "data" / "rhema.db"
TOK = ROOT / "models" / "minilm-l6-v2" / "tokenizer.json"
ONNX = ROOT / "models" / "minilm-l6-v2-int8" / "onnx" / "model_quantized.onnx"
DIM = 384
CAP = 128
N_VERSES = 800
SEED = 7


def mean_pool(hidden: np.ndarray, mask: np.ndarray) -> np.ndarray:
    # hidden: [1, seq, dim], mask: [1, seq]
    m = mask.astype(np.float32)[..., None]
    summed = (hidden * m).sum(axis=1)
    counts = np.clip(m.sum(axis=1), 1e-9, None)
    v = summed / counts
    v = v / np.clip(np.linalg.norm(v, axis=1, keepdims=True), 1e-9, None)
    return v[0]


def main() -> None:
    tok = Tokenizer.from_file(str(TOK))
    tok.no_padding()
    tok.no_truncation()
    sess = ort.InferenceSession(str(ONNX), providers=["CPUExecutionProvider"])
    in_names = {i.name for i in sess.get_inputs()}

    def embed(text: str, pad_to: int | None = CAP) -> np.ndarray:
        enc = tok.encode(text, add_special_tokens=True)
        ids = list(enc.ids)
        if len(ids) > CAP:
            ids = ids[:CAP]
        mask = [1] * len(ids)
        if pad_to is not None:
            if len(ids) < pad_to:
                pad_n = pad_to - len(ids)
                ids = ids + [0] * pad_n
                mask = mask + [0] * pad_n
        feed = {
            "input_ids": np.array([ids], dtype=np.int64),
            "attention_mask": np.array([mask], dtype=np.int64),
            "token_type_ids": np.array([[0] * len(ids)], dtype=np.int64),
        }
        feed = {k: v for k, v in feed.items() if k in in_names}
        outs = sess.run(None, feed)
        # last_hidden_state
        hidden = outs[0]
        return mean_pool(hidden, np.array(mask)[None, :])

    db = sqlite3.connect(str(DB))
    cur = db.cursor()
    abbr_to_id = {
        a: i
        for i, a in cur.execute(
            "SELECT id, abbreviation FROM translations WHERE abbreviation IN (?,?,?,?,?)",
            ("KJV", "SpaRV", "FreJND", "PorBLivre", "WEB"),
        )
    }
    print("translations", abbr_to_id)

    def load(abbr: str) -> dict[tuple[int, int, int], tuple[int, str]]:
        tid = abbr_to_id[abbr]
        m = {}
        for vid, bn, ch, v, text in cur.execute(
            "SELECT id, book_number, chapter, verse, text FROM verses WHERE translation_id=?",
            (tid,),
        ):
            m[(bn, ch, v)] = (vid, text)
        return m

    kjv = load("KJV")
    sparv = load("SpaRV")
    fre = load("FreJND")
    por = load("PorBLivre")
    web = load("WEB")

    keys = sorted(kjv.keys())
    random.seed(SEED)
    sample_keys = random.sample(keys, N_VERSES)

    def blend(parts: list[str]) -> str:
        return " ".join(p.strip() for p in parts if p and p.strip())

    # Document sets: list of (verse_id, text) — may have multiple vectors per verse_id
    def docs_current():
        out = []
        for k in sample_keys:
            vid, kt = kjv[k]
            parts = [kt]
            if k in sparv:
                parts.append(sparv[k][1])
            if k in fre:
                parts.append(fre[k][1])
            if k in por:
                parts.append(por[k][1])
            out.append((vid, blend(parts)))
            if k in web:
                out.append((vid, web[k][1]))
        return out

    def docs_drop_es_pt():
        out = []
        for k in sample_keys:
            vid, kt = kjv[k]
            parts = [kt]
            if k in fre:
                parts.append(fre[k][1])
            out.append((vid, blend(parts)))
            if k in web:
                out.append((vid, web[k][1]))
        return out

    def docs_split():
        out = []
        for k in sample_keys:
            vid, kt = kjv[k]
            out.append((vid, kt))
            for m in (sparv, fre, por, web):
                if k in m:
                    out.append((vid, m[k][1]))
        return out

    def docs_kjv_only():
        return [(kjv[k][0], kjv[k][1]) for k in sample_keys]

    configs = {
        "current_blend+WEB": docs_current,
        "drop_ES+PT+WEB": docs_drop_es_pt,
        "all_separate": docs_split,
        "KJV_only": docs_kjv_only,
    }

    # Prebuild matrices
    indexes = {}
    for name, builder in configs.items():
        docs = builder()
        print(f"embedding {name}: {len(docs)} vectors ...")
        t0 = time.perf_counter()
        mat = np.stack([embed(t) for _, t in docs]).astype(np.float32)
        ids = np.array([vid for vid, _ in docs], dtype=np.int64)
        print(f"  done in {time.perf_counter()-t0:.1f}s shape={mat.shape}")
        indexes[name] = (mat, ids)

    # Query sets
    def queries_kjv_verbatim():
        return [(kjv[k][0], kjv[k][1]) for k in sample_keys]

    def queries_web():
        out = []
        for k in sample_keys:
            if k in web:
                out.append((kjv[k][0], web[k][1]))
        return out

    def queries_kjv_first_half():
        # weak spoken-ish: first half of KJV words, lowercased
        out = []
        for k in sample_keys:
            vid, t = kjv[k]
            words = t.split()
            if len(words) < 6:
                continue
            half = " ".join(words[: max(4, len(words) // 2)]).lower()
            out.append((vid, half))
        return out

    def queries_spanish():
        out = []
        for k in sample_keys:
            if k in sparv:
                out.append((kjv[k][0], sparv[k][1]))
        return out

    query_sets = {
        "KJV_verbatim": queries_kjv_verbatim,
        "WEB_text": queries_web,
        "KJV_first_half_lower": queries_kjv_first_half,
        "SpaRV_verbatim": queries_spanish,
    }

    def eval_index(mat, ids, queries, k_list=(1, 5)):
        hits = {k: 0 for k in k_list}
        mrr = 0.0
        n = 0
        for target, qtext in queries:
            q = embed(qtext)
            scores = mat @ q
            # for each unique verse_id, take max score across its vectors
            # simpler: rank all vectors, then first time we see target id
            order = np.argsort(-scores)
            rank = None
            seen = set()
            unique_rank = 0
            for idx in order:
                vid = int(ids[idx])
                if vid in seen:
                    continue
                seen.add(vid)
                unique_rank += 1
                if vid == target:
                    rank = unique_rank
                    break
            if rank is None:
                continue
            n += 1
            mrr += 1.0 / rank
            for k in k_list:
                if rank <= k:
                    hits[k] += 1
        return {
            "n": n,
            "hit@1": hits[1] / n if n else None,
            "hit@5": hits[5] / n if n else None,
            "mrr": mrr / n if n else None,
        }

    print("\n=== Retrieval results (target=KJV verse_id) ===")
    rows = []
    for qname, qbuilder in query_sets.items():
        qs = qbuilder()
        print(f"\n-- queries: {qname} (n={len(qs)}) --")
        for iname, (mat, ids) in indexes.items():
            r = eval_index(mat, ids, qs)
            rows.append((qname, iname, r))
            print(
                f"  {iname:20} hit@1={r['hit@1']*100:5.1f}% hit@5={r['hit@5']*100:5.1f}% MRR={r['mrr']:.3f} n={r['n']}"
            )

    # Also measure doc token trunc rates on this sample
    print("\n=== Sample doc true token lengths (no pad) ===")
    for name, builder in configs.items():
        docs = builder()
        lengths = [len(tok.encode(t, add_special_tokens=True).ids) for _, t in docs]
        trunc = sum(1 for L in lengths if L > CAP) / len(lengths)
        print(
            f"{name:20} n={len(docs)} mean={sum(lengths)/len(lengths):.1f} trunc={trunc*100:.1f}%"
        )


if __name__ == "__main__":
    main()
