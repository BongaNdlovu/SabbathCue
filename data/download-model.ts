/**
 * Downloads the all-MiniLM-L6-v2 ONNX model exported for feature-extraction.
 *
 * This script automatically:
 *   1. Verifies Python >= 3.9.0 is available
 *   2. Creates a .venv if one doesn't exist
 *   3. Installs sentence-transformers + ONNX Runtime into the venv
 *   4. Runs the sentence-transformers ONNX exporter and quantizer
 *
 * Run: bun run download:model
 */

import { join } from "node:path"
import {
  ensurePythonEnv,
  getVenvBin,
  PROJECT_ROOT,
} from "./lib/python-env"

const MODELS_DIR = join(PROJECT_ROOT, "models", "minilm-l6-v2")
const MODELS_DIR_INT8 = join(
  PROJECT_ROOT,
  "models",
  "minilm-l6-v2-int8"
)

async function main() {
  await ensurePythonEnv([
    "optimum-onnx[onnxruntime]",
    "sentence-transformers",
    "accelerate",
  ])

  const python = getVenvBin(process.platform === "win32" ? "python" : "python3")

  console.log(
    "\nExporting all-MiniLM-L6-v2 to ONNX with sentence-transformers...\n"
  )
  console.log(
    "  This downloads the model from HuggingFace and converts it to ONNX format."
  )
  console.log("  This may take a few minutes on first run.\n")

  const proc = Bun.spawn(
    [python, join(PROJECT_ROOT, "data", "export-minilm-onnx.py")],
    {
      stdout: "inherit",
      stderr: "inherit",
      env: { ...process.env, PYTHONUTF8: "1" },
    }
  )

  const exitCode = await proc.exited
  if (exitCode !== 0) {
    console.error("\nExport failed.")
    process.exit(1)
  }

  console.log(`\nModel exported to ${MODELS_DIR}`)
  console.log(`INT8 model exported to ${MODELS_DIR_INT8}\n`)

  console.log("  Files created:")
  console.log("  - onnx/model.onnx (FP32, sentence-transformers feature output)")
  console.log("  - onnx/model.onnx_data (external ONNX weights)")
  console.log("  - onnx/model_quantized.onnx (INT8)")
  console.log("  - tokenizer.json")
}

main().catch((err) => {
  console.error("Failed:", err)
  process.exit(1)
})
