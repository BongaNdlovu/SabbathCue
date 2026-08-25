/**
 * Verifies the public Cloudflare R2 Windows installer URL configured for the site.
 * Run: node scripts/verify-windows-installer-download.mjs
 */

const R2_BASE = "https://pub-f00266e4b83341dea437c0114752f536.r2.dev";
const OBJECT_KEY = "SabbathCue.Personal_0.1.9_x64-setup (1).exe";
const URL = `${R2_BASE}/${OBJECT_KEY.split("/").map(encodeURIComponent).join("/")}`;
const MIN_BYTES = 200 * 1024 * 1024;
const EXPECTED_BYTES = 243588694;

async function main() {
  console.log("Checking Windows installer on Cloudflare R2…");
  console.log(`URL: ${URL}`);

  const res = await fetch(URL, { method: "HEAD" });
  if (!res.ok) {
    console.error(`FAIL: HTTP ${res.status}`);
    process.exit(1);
  }

  const contentType = res.headers.get("content-type") ?? "";
  const contentLength = res.headers.get("content-length");
  const bytes = contentLength ? Number(contentLength) : NaN;

  console.log(`Content-Type: ${contentType}`);
  console.log(`Content-Length: ${contentLength} (${(bytes / (1024 * 1024)).toFixed(1)} MB)`);

  if (!Number.isFinite(bytes) || bytes < MIN_BYTES) {
    console.error(`FAIL: expected at least ${MIN_BYTES} bytes`);
    process.exit(1);
  }

  if (bytes !== EXPECTED_BYTES) {
    console.warn(
      `WARN: byte count ${bytes} differs from recorded baseline ${EXPECTED_BYTES} (update test if intentional)`
    );
  }

  if (
    !contentType.includes("application/x-msdownload") &&
    !contentType.includes("application/octet-stream")
  ) {
    console.error(`FAIL: unexpected content-type ${contentType}`);
    process.exit(1);
  }

  console.log("OK: R2 installer is reachable and looks like a valid Windows setup binary.");
}

main().catch((err) => {
  console.error("FAIL:", err);
  process.exit(1);
});
