#!/usr/bin/env python3
"""Streaming Vosk worker for SabbathCue.

Reads raw little-endian 16-bit PCM from stdin and emits line-delimited JSON:
{"type":"partial","text":"..."} and {"type":"final","text":"..."}.
"""

from __future__ import annotations

import argparse
import json
import sys


def emit(payload: dict) -> None:
    print(json.dumps(payload, ensure_ascii=False), flush=True)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--model", required=True)
    parser.add_argument("--sample-rate", type=int, default=16000)
    args = parser.parse_args()

    try:
        from vosk import KaldiRecognizer, Model
    except Exception as exc:
        emit({"type": "error", "message": f"Python package 'vosk' is not installed: {exc}"})
        return 1

    try:
        model = Model(args.model)
        recognizer = KaldiRecognizer(model, args.sample_rate)
        recognizer.SetWords(True)
        emit({"type": "ready"})

        while True:
            chunk = sys.stdin.buffer.read(1600)
            if not chunk:
                break
            if recognizer.AcceptWaveform(chunk):
                result = json.loads(recognizer.Result())
                text = (result.get("text") or "").strip()
                if text:
                    emit({"type": "final", "text": text})
            else:
                partial = json.loads(recognizer.PartialResult())
                text = (partial.get("partial") or "").strip()
                if text:
                    emit({"type": "partial", "text": text})

        final = json.loads(recognizer.FinalResult())
        text = (final.get("text") or "").strip()
        if text:
            emit({"type": "final", "text": text})
    except Exception as exc:
        emit({"type": "error", "message": f"Vosk worker failed: {exc}"})
        return 1

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
