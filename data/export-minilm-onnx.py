#!/usr/bin/env python3
"""Export all-MiniLM-L6-v2 to the ONNX layout used by SabbathCue."""

from pathlib import Path

from sentence_transformers import SentenceTransformer
from sentence_transformers.backend import export_dynamic_quantized_onnx_model

ROOT = Path(__file__).resolve().parent.parent
MODEL_NAME = "sentence-transformers/all-MiniLM-L6-v2"
MODELS_DIR = ROOT / "models" / "minilm-l6-v2"
MODELS_DIR_INT8 = ROOT / "models" / "minilm-l6-v2-int8"


def main() -> None:
    fp32_path = MODELS_DIR / "onnx" / "model.onnx"
    int8_path = MODELS_DIR_INT8 / "onnx" / "model_quantized.onnx"

    if fp32_path.exists() and int8_path.exists():
        print("ONNX exports already exist; nothing to do.")
        return

    print(f"Exporting {MODEL_NAME} with sentence-transformers ONNX backend...")
    model = SentenceTransformer(MODEL_NAME, backend="onnx")
    model.save(str(MODELS_DIR))
    print(f"FP32 ONNX model saved to {fp32_path}")

    print("Quantizing ONNX model to INT8...")
    local_model = SentenceTransformer(str(MODELS_DIR), backend="onnx")
    export_dynamic_quantized_onnx_model(
        local_model,
        "avx2",
        str(MODELS_DIR_INT8),
        file_suffix="quantized",
    )
    print(f"INT8 ONNX model saved to {int8_path}")


if __name__ == "__main__":
    main()
