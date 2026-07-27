/** Public Cloudflare R2 bucket for Windows installer binaries. */
export const R2_PUBLIC_BASE_URL =
  "https://pub-f00266e4b83341dea437c0114752f536.r2.dev" as const;

export const WINDOWS_INSTALLER_VERSION = "0.1.9" as const;

export const WINDOWS_INSTALLER_R2_OBJECT_KEY =
  "SabbathCue Personal_0.1.9_x64-setup.exe" as const;

/** Filename users see when saving the installer (not the R2 object name). */
export const WINDOWS_INSTALLER_SAVE_AS = "SabbathCue-Setup.exe" as const;

/** Measured via R2 HEAD on 2026-07-27 (build with Paddle checkout enabled). */
export const WINDOWS_INSTALLER_EXPECTED_BYTES = 209799413;

export const WINDOWS_INSTALLER_MIN_BYTES = 200 * 1024 * 1024;

export type WindowsInstallerDownloadConfig = {
  url: string;
  saveAsFilename: string;
  version: typeof WINDOWS_INSTALLER_VERSION;
  objectKey: typeof WINDOWS_INSTALLER_R2_OBJECT_KEY;
};

export function buildR2PublicUrl(objectKey: string): string {
  return `${R2_PUBLIC_BASE_URL}/${objectKey
    .split("/")
    .map((segment) => encodeURIComponent(segment))
    .join("/")}`;
}

export function getWindowsInstallerDownloadConfig(): WindowsInstallerDownloadConfig {
  return {
    url: buildR2PublicUrl(WINDOWS_INSTALLER_R2_OBJECT_KEY),
    saveAsFilename: WINDOWS_INSTALLER_SAVE_AS,
    version: WINDOWS_INSTALLER_VERSION,
    objectKey: WINDOWS_INSTALLER_R2_OBJECT_KEY,
  };
}

export function windowsInstallerDownloadLinkProps(): {
  href: string;
  download: string;
  target: "_self";
} {
  const config = getWindowsInstallerDownloadConfig();
  return {
    href: config.url,
    download: config.saveAsFilename,
    target: "_self",
  };
}

export type InstallerDownloadVerification =
  | { ok: true; bytes: number; contentType: string }
  | { ok: false; reason: string };

/** HEAD-check the live R2 object (used in tests and release verification). */
export async function verifyWindowsInstallerDownload(
  fetchFn: typeof fetch = fetch
): Promise<InstallerDownloadVerification> {
  const { url } = getWindowsInstallerDownloadConfig();

  try {
    const res = await fetchFn(url, { method: "HEAD" });
    if (!res.ok) {
      return { ok: false, reason: `HTTP ${res.status}` };
    }

    const contentLength = res.headers.get("content-length");
    const bytes = contentLength ? Number(contentLength) : NaN;
    if (!Number.isFinite(bytes) || bytes < WINDOWS_INSTALLER_MIN_BYTES) {
      return {
        ok: false,
        reason: `content-length ${contentLength ?? "missing"} (min ${WINDOWS_INSTALLER_MIN_BYTES})`,
      };
    }

    const contentType = res.headers.get("content-type") ?? "";
    const validType =
      contentType.includes("application/x-msdownload") ||
      contentType.includes("application/octet-stream");
    if (!validType) {
      return { ok: false, reason: `content-type ${contentType}` };
    }

    return { ok: true, bytes, contentType };
  } catch (error) {
    return { ok: false, reason: String(error) };
  }
}
