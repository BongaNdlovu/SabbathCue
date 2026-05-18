/**
 * Downloads Bible source data: a pre-built zip of all 10 translations from
 * Google Drive, plus the cross-references dataset from openbible.info.
 *
 * Run: bun run data/download-sources.ts
 */

import { mkdir, rm } from "node:fs/promises"
import { existsSync } from "node:fs"
import { join } from "node:path"

const DATA_DIR = import.meta.dir
const SOURCES_DIR = join(DATA_DIR, "sources")
const CROSS_REFS_DIR = join(DATA_DIR, "cross-refs")

// Pre-built zip of all 10 translations (~74 MB uncompressed). The `confirm=t`
// query param bypasses Google Drive's "can't scan for viruses" interstitial
// that would otherwise return an HTML page for files >100 MB.
const SOURCES_ZIP_URL =
  "https://drive.google.com/uc?export=download&id=1HQiNf_nCVRQrMbdmzVG7vq-Fvfqh1nzW&confirm=t"

const EXPECTED_TRANSLATIONS = [
  "KJV.json",
  "NIV.json",
  "ESV.json",
  "NASB.json",
  "NKJV.json",
  "NLT.json",
  "AMP.json",
  "SpaRV.json",
  "FreJND.json",
  "PorBLivre.json",
]

const CROSS_REFS_URL = "https://a.openbible.info/data/cross-references.zip"

async function downloadFile(url: string, dest: string): Promise<void> {
  console.log(`  Downloading ${url}...`)
  const res = await fetch(url)
  if (!res.ok) throw new Error(`Failed to download ${url}: ${res.status}`)
  const buffer = await res.arrayBuffer()
  await Bun.write(dest, buffer)
  const sizeMB = (buffer.byteLength / 1024 / 1024).toFixed(1)
  console.log(`  ✓ Saved ${dest} (${sizeMB} MB)`)
}

function isZip(bytes: Uint8Array): boolean {
  return (
    bytes.length >= 4 &&
    bytes[0] === 0x50 && // P
    bytes[1] === 0x4b && // K
    bytes[2] === 0x03 &&
    bytes[3] === 0x04
  )
}

async function spawnChecked(cmd: string[]): Promise<number> {
  const proc = Bun.spawn(cmd, { stdout: "inherit", stderr: "inherit" })
  return proc.exited
}

/** Extract a zip without requiring the Unix `unzip` binary (missing on Windows by default). */
async function extractZip(zipPath: string, destDir: string): Promise<void> {
  // `tar` ships with Windows 10+, macOS, and most Linux distros.
  try {
    const tarExit = await spawnChecked(["tar", "-xf", zipPath, "-C", destDir])
    if (tarExit === 0) return
  } catch {
    // tar not on PATH — try fallbacks below
  }

  try {
    const unzipExit = await spawnChecked(["unzip", "-o", zipPath, "-d", destDir])
    if (unzipExit === 0) return
  } catch {
    // unzip not on PATH
  }

  if (process.platform === "win32") {
    const escapedZip = zipPath.replace(/'/g, "''")
    const escapedDest = destDir.replace(/'/g, "''")
    const psExit = await spawnChecked([
      "powershell",
      "-NoProfile",
      "-Command",
      `Expand-Archive -LiteralPath '${escapedZip}' -DestinationPath '${escapedDest}' -Force`,
    ])
    if (psExit === 0) return
  }

  throw new Error(
    `Failed to extract ${zipPath}. Ensure tar, unzip, or PowerShell Expand-Archive is available.`,
  )
}

async function downloadSourcesZip(): Promise<void> {
  const allPresent = EXPECTED_TRANSLATIONS.every((f) =>
    existsSync(join(SOURCES_DIR, f))
  )
  if (allPresent) {
    console.log("  ⏭ All translations already present, skipping zip download")
    return
  }

  // Stage the zip in DATA_DIR so it extracts as `data/sources/...` (the zip's
  // internal layout has a top-level `sources/` directory).
  const zipDest = join(DATA_DIR, "sources.zip")
  await downloadFile(SOURCES_ZIP_URL, zipDest)

  // Validate that we got a real zip and not Google Drive's HTML interstitial
  // (happens if the file ID is wrong, the file is private, or quota exceeded).
  const head = new Uint8Array(await Bun.file(zipDest).slice(0, 4).arrayBuffer())
  if (!isZip(head)) {
    await rm(zipDest, { force: true })
    throw new Error(
      "Google Drive returned a non-zip response (likely an HTML page). " +
        "Verify the file's sharing is set to 'Anyone with the link' and that " +
        "the daily download quota has not been exceeded."
    )
  }

  console.log("  📦 Extracting sources.zip...")
  await extractZip(zipDest, DATA_DIR)
  await rm(zipDest, { force: true })
  await rm(join(DATA_DIR, "__MACOSX"), { recursive: true, force: true })

  const missing = EXPECTED_TRANSLATIONS.filter(
    (f) => !existsSync(join(SOURCES_DIR, f))
  )
  if (missing.length > 0) {
    throw new Error(
      `Zip extracted but missing expected files: ${missing.join(", ")}`
    )
  }
  console.log(`  ✓ Extracted ${EXPECTED_TRANSLATIONS.length} translations`)
}

async function downloadCrossRefs(): Promise<void> {
  const crossRefFile = join(CROSS_REFS_DIR, "cross_references.txt")
  const existing = Bun.file(crossRefFile)
  if ((await existing.exists()) && existing.size > 1000) {
    console.log("  ⏭ cross_references.txt already exists, skipping")
    return
  }

  const zipDest = join(CROSS_REFS_DIR, "cross-references.zip")
  await downloadFile(CROSS_REFS_URL, zipDest)
  await extractZip(zipDest, CROSS_REFS_DIR)
  await rm(zipDest, { force: true })
}

async function main() {
  await mkdir(SOURCES_DIR, { recursive: true })
  await mkdir(CROSS_REFS_DIR, { recursive: true })

  console.log("\n📖 Downloading Bible translations (pre-built zip)...\n")
  await downloadSourcesZip()

  console.log("\n🔗 Downloading cross-references...\n")
  await downloadCrossRefs()

  console.log("\n✅ All source data downloaded!\n")
}

main().catch((err) => {
  console.error("❌ Download failed:", err)
  process.exit(1)
})
