/**
 * Local, persisted Tauri E2E harness configuration. Uses
 * @wdio/tauri-service's embedded WebDriver provider (tauri-plugin-wdio-
 * webdriver, compiled in only under the e2e-webdriver Cargo feature -- see
 * apps/cryptovol-gui/src-tauri/Cargo.toml and src/lib.rs), so this drives
 * the real built GUI through real Tauri IPC rather than calling Rust
 * command implementation functions directly. No CI/hosting-specific
 * settings are configured here; see docs/gui-testing.md for why E2E is
 * local-only in this milestone.
 */

import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import path from "node:path";
import { APP_BINARY_PATH, REPO_ROOT } from "./support/paths.js";

const GUI_APP_DIR = path.join(REPO_ROOT, "apps", "cryptovol-gui");
const TAURI_CONF_PATH = path.join(GUI_APP_DIR, "src-tauri", "tauri.conf.json");

/**
 * Whether to build/run the E2E suite with the app window visible. Default
 * is invisible so the suite never appears on screen or steals foreground
 * focus while it runs; set CRYPTOVOL_E2E_HEADED=1 (or run
 * `npm run test:e2e:headed`) to watch it run -- e.g. debugging a flaky
 * visual issue. Scoped entirely to this harness's own e2e-webdriver build;
 * production/dev builds never read this var. See docs/gui-testing.md
 * caveat 10. Distinct from, and does not affect, the separate manual
 * AppleScript/System-Events visual-audit technique used for one-off GUI
 * checks, which still requires (and still gets) a real, visible window.
 */
const CRYPTOVOL_E2E_HEADED = process.env.CRYPTOVOL_E2E_HEADED === "1";

/**
 * Reads the single window entry from the committed tauri.conf.json so the
 * E2E TAURI_CONFIG overlay never hardcodes a second, driftable copy of
 * title/width/height. Required because TAURI_CONFIG is applied via RFC
 * 7386 JSON Merge Patch (tauri-build's `try_build` calls `json_patch::
 * merge`), which replaces arrays wholesale rather than merging elements by
 * index -- overlaying a partial window object (e.g. just
 * `{ visible: false }`) would silently drop title/width/height for e2e
 * builds only.
 */
function readBaseWindowConfig(): Record<string, unknown> {
  const parsed = JSON.parse(readFileSync(TAURI_CONF_PATH, "utf-8"));
  const windows = parsed?.app?.windows;
  if (!Array.isArray(windows) || windows.length !== 1) {
    throw new Error(
      `Expected exactly one window in ${TAURI_CONF_PATH} to derive the ` +
        `e2e TAURI_CONFIG overlay from; found ${windows?.length ?? 0}. ` +
        `Update readBaseWindowConfig() in wdio.conf.ts if this app ever ` +
        `has more than one window.`,
    );
  }
  return windows[0] as Record<string, unknown>;
}

/**
 * Builds the TAURI_CONFIG overlay for the e2e-webdriver build. Merges in
 * `withGlobalTauri` -- @wdio/tauri-service's own window-state polling
 * calls `window.__TAURI__.core.invoke(...)`, which only exists when that
 * flag is enabled; our app doesn't need it (it only ever imports `invoke`
 * as an ES module) and must not ship it in production, so it's merged in
 * only for this E2E build, never written into the committed
 * tauri.conf.json -- and the real base window config with only `visible`
 * overridden per CRYPTOVOL_E2E_HEADED (see readBaseWindowConfig above for
 * why the whole object must be re-supplied, not just `visible`).
 */
function buildTauriConfigOverlay(): string {
  return JSON.stringify({
    app: {
      withGlobalTauri: true,
      windows: [{ ...readBaseWindowConfig(), visible: CRYPTOVOL_E2E_HEADED }],
    },
  });
}

/**
 * Builds the app with the e2e-webdriver feature so the harness never
 * depends on a human pre-building it. Runs the Vite build first: a plain
 * `cargo build` only embeds whatever is currently in apps/cryptovol-gui/
 * dist/ (unlike `tauri build`, it does not run tauri.conf.json's
 * beforeBuildCommand itself).
 *
 * `VITE_E2E_WEBDRIVER=1` tells `src/app/main.tsx` to import
 * `@wdio/tauri-plugin` before rendering -- see that file for why this is a
 * build-time flag rather than always importing it.
 */
function buildAppForE2e(): void {
  execFileSync("npm", ["run", "build"], {
    cwd: GUI_APP_DIR,
    stdio: "inherit",
    env: { ...process.env, VITE_E2E_WEBDRIVER: "1" },
  });
  execFileSync(
    "cargo",
    ["build", "--package", "cryptovol-gui", "--features", "e2e-webdriver"],
    {
      cwd: REPO_ROOT,
      stdio: "inherit",
      env: { ...process.env, TAURI_CONFIG: buildTauriConfigOverlay() },
    },
  );
}

export const config: WebdriverIO.Config = {
  runner: "local",
  specs: ["./specs/**/*.e2e.ts"],

  maxInstances: 1,
  maxInstancesPerCapability: 1,

  services: [
    [
      "@wdio/tauri-service",
      {
        appBinaryPath: APP_BINARY_PATH,
        driverProvider: "embedded",
      },
    ],
  ],

  capabilities: [
    {
      browserName: "tauri",
      "tauri:options": {
        application: APP_BINARY_PATH,
      },
    },
  ],

  logLevel: "warn",
  bail: 0,
  waitforTimeout: 15_000,
  connectionRetryTimeout: 90_000,
  connectionRetryCount: 3,

  framework: "mocha",
  mochaOpts: {
    ui: "bdd",
    // With @wdio/tauri-plugin installed (see main.tsx/buildAppForE2e above),
    // @wdio/tauri-service's per-command window-focus check no longer falls
    // back to its slow internal-eval path, so per-test time is now a few
    // seconds, not minutes -- see docs/gui-testing.md caveat 7. This still
    // leaves a wide margin over the largest individual element-wait ceiling
    // used in specs/ (30s, e.g. the wrong-password error wait).
    timeout: 60_000,
  },

  reporters: ["spec"],

  onPrepare: () => {
    buildAppForE2e();
  },
};
