"""Primary measurement: MiniLM token lengths of blended verse corpora.

Reads rhema.db + models/minilm-l6-v2/tokenizer.json.
Sample: 4000 KJV references, seed=42.
No padding; lengths include special tokens (add_special_tokens=True).
"""
from __future__ import annotations

import random
import sqlite3
from pathlib import Path

from tokenizers import Tokenizer

ROOT = Path(__file__).resolve().parents[2]
TOK_PATH = ROOT / "models" / "minilm-l6-v2" / "tokenizer.json"
DB_PATH = ROOT / "data" / "rhema.db"
WANTED = ["KJV", "SpaRV", "FreJND", "PorBLivre", "WEB"]
SAMPLE_N = 4000
SEED = 42
CAP = 128


def main() -> None:
    assert TOK_PATH.exists(), TOK_PATH
    assert DB_PATH.exists(), DB_PATH
    tok = Tokenizer.from_file(str(TOK_PATH))
    # tokenizer.json ships with Fixed(128) pad + trunc; strip them so we
    # measure true sequence length (what truncation would cut).
    tok.no_padding()
    tok.no_truncation()

    db = sqlite3.connect(str(DB_PATH))
    cur = db.cursor()
    rows = cur.execute(
        f"SELECT id, abbreviation FROM translations WHERE abbreviation IN ({','.join('?' * len(WANTED))})",
        WANTED,
    ).fetchall()
    abbr_to_id = {a: i for i, a in rows}
    print("translations found:", abbr_to_id)

    kjv_id = abbr_to_id["KJV"]
    kjv = cur.execute(
        "SELECT book_number, chapter, verse, text, id FROM verses WHERE translation_id=? ORDER BY book_number, chapter, verse",
        (kjv_id,),
    ).fetchall()
    print("KJV verses:", len(kjv))

    random.seed(SEED)
    sample = [kjv[i] for i in sorted(random.sample(range(len(kjv)), SAMPLE_N))]

    def load_trans(abbr: str) -> dict[tuple[int, int, int], str]:
        tid = abbr_to_id.get(abbr)
        if tid is None:
            return {}
        m: dict[tuple[int, int, int], str] = {}
        for bn, ch, v, text in cur.execute(
            "SELECT book_number, chapter, verse, text FROM verses WHERE translation_id=?",
            (tid,),
        ):
            m[(bn, ch, v)] = text
        return m

    texts = {a: load_trans(a) for a in WANTED}
    print("verse counts per translation:", {a: len(m) for a, m in texts.items()})

    # special-token overhead check
    with_sp = tok.encode("hello world", add_special_tokens=True)
    no_sp = tok.encode("hello world", add_special_tokens=False)
    print(
        "special-token overhead example:",
        len(with_sp.ids) - len(no_sp.ids),
        "tokens=",
        with_sp.tokens,
    )

    def token_len(s: str) -> int:
        return len(tok.encode(s, add_special_tokens=True).ids)

    def content_len(s: str) -> int:
        return len(tok.encode(s, add_special_tokens=False).ids)

    def specials_for(s: str) -> int:
        return token_len(s) - content_len(s)

    def stats_for(langs: list[str]) -> dict:
        lengths: list[int] = []
        trunc = 0
        kjv_shares: list[float] = []
        for bn, ch, v, _, _ in sample:
            parts: list[str] = []
            for a in langs:
                t = texts[a].get((bn, ch, v))
                if t and t.strip():
                    parts.append(t.strip())
            text = " ".join(parts)
            L = token_len(text)
            lengths.append(L)
            if L > CAP:
                trunc += 1

            # KJV share of tokens that survive the 128-cap (order-preserving budget)
            specials = specials_for(text)
            budget = max(0, CAP - specials)
            prev = 0
            cum: list[str] = []
            kjv_seen = 0
            total_seen = 0
            for a in langs:
                t = texts[a].get((bn, ch, v))
                if not t or not t.strip():
                    continue
                cum.append(t.strip())
                clen = content_len(" ".join(cum))
                this = clen - prev
                take = min(this, budget)
                if a == "KJV":
                    kjv_seen += take
                total_seen += take
                budget -= take
                prev = clen
            if total_seen > 0:
                kjv_shares.append(kjv_seen / total_seen)

        lengths.sort()

        def pct(p: float) -> int:
            i = int(round((p / 100) * (len(lengths) - 1)))
            return lengths[i]

        return {
            "n": len(lengths),
            "mean": sum(lengths) / len(lengths),
            "p50": pct(50),
            "p90": pct(90),
            "p99": pct(99),
            "max": lengths[-1],
            "trunc_rate": trunc / len(lengths),
            "kjv_share_mean": sum(kjv_shares) / len(kjv_shares) if kjv_shares else None,
        }

    def pt_starved_rate(langs: list[str]) -> tuple[int, int, float | None]:
        """How often PorBLivre receives 0 tokens after 128-cap under given order."""
        zero = 0
        total = 0
        for bn, ch, v, _, _ in sample:
            ordered: list[tuple[str, str]] = []
            for a in langs:
                t = texts[a].get((bn, ch, v))
                if t and t.strip():
                    ordered.append((a, t.strip()))
            if not any(a == "PorBLivre" for a, _ in ordered):
                continue
            text = " ".join(p for _, p in ordered)
            specials = specials_for(text)
            budget = max(0, CAP - specials)
            prev = 0
            cum: list[str] = []
            pt_tokens = 0
            for a, p in ordered:
                cum.append(p)
                clen = content_len(" ".join(cum))
                this = clen - prev
                take = min(this, budget)
                if a == "PorBLivre":
                    pt_tokens = take
                budget -= take
                prev = clen
            total += 1
            if pt_tokens == 0:
                zero += 1
        return zero, total, (zero / total if total else None)

    print(
        f"\n=== Token length stats (n={SAMPLE_N}, seed={SEED}, MiniLM tokenizer, specials included) ==="
    )
    print(
        f"{'corpus':28} {'mean':>7} {'p50':>5} {'p90':>5} {'p99':>5} {'max':>5} {'trunc%':>8} {'kjv_share%':>10}"
    )
    configs = [
        ("current (EN+ES+FR+PT)", ["KJV", "SpaRV", "FreJND", "PorBLivre"]),
        ("drop PT", ["KJV", "SpaRV", "FreJND"]),
        ("drop ES+PT", ["KJV", "FreJND"]),
        ("drop FR+ES+PT (KJV alone)", ["KJV"]),
        ("EN+ES only", ["KJV", "SpaRV"]),
        ("EN+ES+FR+PT+WEB blended", ["KJV", "SpaRV", "FreJND", "PorBLivre", "WEB"]),
    ]
    results = {}
    for name, langs in configs:
        s = stats_for(langs)
        results[name] = s
        ks = s["kjv_share_mean"] * 100 if s["kjv_share_mean"] is not None else float("nan")
        print(
            f"{name:28} {s['mean']:7.1f} {s['p50']:5d} {s['p90']:5d} {s['p99']:5d} {s['max']:5d} {s['trunc_rate']*100:7.1f}% {ks:9.1f}%"
        )

    z, t, r = pt_starved_rate(["KJV", "SpaRV", "FreJND", "PorBLivre"])
    print(f"\nPT starved (0 tokens after trunc) under EN+ES+FR+PT: {z}/{t} = {r*100:.1f}%")
    z2, t2, r2 = pt_starved_rate(["KJV", "SpaRV", "FreJND"])  # no PT in langs — N/A

    # Mean tokens per language alone (for split scenario)
    print("\n=== Per-translation alone (same sample) ===")
    for a in WANTED:
        if a not in abbr_to_id:
            continue
        s = stats_for([a])
        print(
            f"{a:12} mean={s['mean']:.1f} p90={s['p90']} trunc={s['trunc_rate']*100:.1f}% max={s['max']}"
        )

    # Index growth for split scenario
    # unique refs = sample universe is full KJV; count how many separate vectors
    # Full corpus: each of SpaRV, FreJND, PorBLivre, WEB that exist + 1 blended/KJV
    print("\n=== Full-corpus vector count projection ===")
    n_kjv = len(kjv)
    counts = {}
    for a in ["SpaRV", "FreJND", "PorBLivre", "WEB"]:
        counts[a] = sum(
            1
            for bn, ch, v, _, _ in kjv
            if texts[a].get((bn, ch, v), "").strip()
        )
    current = n_kjv + counts["WEB"]  # blended one + WEB separate
    # actual may differ if some blended missing langs but still one entry per KJV
    print(f"KJV refs: {n_kjv}")
    print(f"WEB present: {counts['WEB']}")
    print(f"SpaRV present: {counts['SpaRV']}")
    print(f"FreJND present: {counts['FreJND']}")
    print(f"PorBLivre present: {counts['PorBLivre']}")
    print(f"current expected vectors (1 blended + WEB): {n_kjv + counts['WEB']}")
    split = n_kjv + counts["SpaRV"] + counts["FreJND"] + counts["PorBLivre"] + counts["WEB"]
    print(f"if all separate (KJV+SpaRV+FreJND+PorBLivre+WEB): {split}")
    print(f"ratio split/current: {split / (n_kjv + counts['WEB']):.2f}")

    out = ROOT / ".tmp" / "blend_token_stats.json"
    import json

    payload = {
        "sample_n": SAMPLE_N,
        "seed": SEED,
        "configs": {
            k: {
                **v,
                "kjv_share_mean": v["kjv_share_mean"],
            }
            for k, v in results.items()
        },
        "pt_starved": {"zero": z, "total": t, "rate": r},
        "vector_projection": {
            "kjv": n_kjv,
            "counts": counts,
            "current_expected": n_kjv + counts["WEB"],
            "all_separate": split,
        },
    }
    out.write_text(json.dumps(payload, indent=2))
    print(f"\nWrote {out}")


if __name__ == "__main__":
    main()
