import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import test from "node:test";

test("setup reaches gated-model guard with its default path logic intact", () => {
  const experimentRoot = mkdtempSync(join(tmpdir(), "functiongemma-setup-"));
  const environment = { ...process.env };
  delete environment.HF_TOKEN;

  try {
    const result = spawnSync(
      "powershell",
      [
        "-NoProfile",
        "-ExecutionPolicy",
        "Bypass",
        "-File",
        resolve("scripts/setup-functiongemma-benchmark.ps1"),
        "-ExperimentRoot",
        experimentRoot,
      ],
      { encoding: "utf8", env: environment },
    );
    const output = `${result.stdout}\n${result.stderr}`;

    assert.notEqual(result.status, 0);
    assert.match(output, /accept Google's Gemma license/);
    assert.doesNotMatch(output, /Join-Path.*empty string/);
  } finally {
    rmSync(experimentRoot, { recursive: true, force: true });
  }
});
