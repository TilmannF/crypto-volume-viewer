/**
 * Path helpers for the Tauri E2E suite: the release binary the embedded
 * WebDriver provider launches, the committed static fixtures (resolved
 * from the repo root so specs never depend on the process's cwd), and
 * fresh OS-temp-directory extraction destinations. See docs/gui-testing.md
 * for fixture/temp-output policy.
 */

import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const SUPPORT_DIR = fileURLToPath(new URL(".", import.meta.url));

/** apps/cryptovol-gui/e2e/support -> apps/cryptovol-gui/e2e -> apps/cryptovol-gui -> apps -> repo root. */
export const REPO_ROOT = path.resolve(SUPPORT_DIR, "..", "..", "..", "..");

const STATIC_FIXTURES_DIR = path.join(REPO_ROOT, "testdata", "static");

/** Committed static TC/VC-compatible test containers (see docs/test-containers.md). */
export const FIXTURES = {
  fat: path.join(STATIC_FIXTURES_DIR, "tcvc-aes-sha512-fat-lfn-unicode.hc"),
  exfat: path.join(STATIC_FIXTURES_DIR, "tcvc-aes-sha512-exfat-lfn-unicode.hc"),
  ntfs: path.join(STATIC_FIXTURES_DIR, "tcvc-aes-sha512-ntfs-lfn-unicode.hc"),
} as const;

/** Ground-truth plaintext files the FAT fixture's contents must extract to byte-for-byte. */
export const FAT_GROUND_TRUTH_DIR = path.join(STATIC_FIXTURES_DIR, "fs-fat-lfn-original");

/** Public test password documented in docs/test-containers.md; not a real secret. */
export const TEST_PASSWORD = "test-password";

const BINARY_NAME = process.platform === "win32" ? "cryptovol-gui.exe" : "cryptovol-gui";

/**
 * The cryptovol-gui binary the E2E harness builds (via wdio.conf.ts's
 * onPrepare hook) with the e2e-webdriver Cargo feature enabled. Cargo
 * workspaces share one target/ directory at the repo root regardless of
 * which member crate is built.
 */
export const APP_BINARY_PATH = path.join(REPO_ROOT, "target", "debug", BINARY_NAME);

/** Creates a fresh OS-temp-directory extraction destination for one test. */
export async function createExtractionTempDir(): Promise<string> {
  return mkdtemp(path.join(tmpdir(), "cryptovol-gui-e2e-"));
}

/** Removes a directory created by createExtractionTempDir, ignoring missing paths. */
export async function cleanupExtractionTempDir(dir: string): Promise<void> {
  await rm(dir, { recursive: true, force: true });
}
