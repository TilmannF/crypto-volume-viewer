# GUI Testing

This document covers the test strategy for `apps/cryptovol-gui`: what each of the three test layers actually proves, how to install/run/debug them, and the platform-specific caveats found while building this on macOS. See [gui-mvp.md](gui-mvp.md) for the GUI's architecture, Tauri command surface, and manual smoke test procedure (still current and not replaced by anything here).

## Why This Exists

The previous GUI milestone found a real bug — `VolumeSession::stat_via_parent_listing` (FAT's stat fallback) never setting `FileEntry.path`, silently disabling extraction for every FAT file — only through manual GUI interaction. The full automated Rust suite (hundreds of tests) missed it because those tests called Tauri command implementation functions directly with hand-built arguments; none of them exercised the real chain:

```text
React UI
  -> shared/api (Tauri invoke/listen)
  -> DTO serialization/deserialization
  -> Tauri command
  -> cryptovol-app
  -> Tauri events
  -> frontend state update
```

This milestone adds two new, durable test layers so that chain is actually covered and version-controlled, on top of the existing Rust suite.

## Test Strategy: Three Layers

| Layer | Drives | Speed | What it can catch |
|---|---|---|---|
| Rust (existing) | `cryptovol-app`, Tauri command `*_impl` functions, DTOs, registries | Fast (seconds) | Business logic, DTO mapping, error mapping, registry behavior — everything below the IPC boundary |
| Frontend integration (`apps/cryptovol-gui/src/**/*.test.tsx`, Vitest) | Real React components/pages, mocked `shared/api/commands` | Fast (seconds) | UI state transitions, selector presence, whether a component *calls* the right API function with the right DTO shape — everything above the IPC boundary, without a real Tauri runtime |
| Tauri E2E (`apps/cryptovol-gui/e2e/**/*.e2e.ts`, WebdriverIO) | The real compiled app, real Tauri IPC, real WKWebView | Slow (minutes) | The full chain above, end to end, against real fixture containers — the only layer that would have caught the FAT `stat` bug as originally found |

Neither of the first two layers replaces the E2E layer: the frontend-integration layer stops at the `shared/api/commands` boundary (it never touches real `invoke`/`listen`), and the Rust layer never renders a real page or drives real IPC. Only the E2E layer proves the seam between them actually works.

**None of the three layers can catch main-thread/UI-responsiveness bugs** (e.g. a Tauri command that blocks the main thread and freezes the whole window while it runs). Rust tests call command functions directly without a real event loop; the frontend-integration layer never touches real IPC; and even the E2E layer only asserts on element state/text, not on whether the window stayed responsive while a command was in flight. This class of bug (see `select_container_file`/`select_extract_destination` in `apps/cryptovol-gui/src-tauri/src/commands/dialogs.rs` and their doc comments for a real example that shipped and was only caught by manual interaction) has to be verified manually and guarded against via doc comments/code review, not an automated test.

### What Rust Tests Cover

Unchanged by this milestone. `cargo test --workspace --all-targets` covers `cryptovol-core`/`cryptovol-tcvc`/`cryptovol-fs-*`/`cryptovol-app`/`cryptovol-cli` business logic, plus `apps/cryptovol-gui/src-tauri/tests/*` (DTO mapping, KDF/PIM parsing, error mapping, session/job registry behavior, cancellation semantics, password-DTO non-`Debug`/non-`Clone` properties). These call `*_impl` functions or app-core APIs directly — no real IPC, no real UI.

**Fixture-gated tests need explicit env vars that `cargo test --workspace` does not set.** `crates/cryptovol-app/tests/list_dir_stat_fixtures.rs`, for example, asserts `stat()` fills in `FileEntry.path` correctly for FAT, exFAT, *and* NTFS via a shared helper — but each of its three tests is `#[ignore]`-gated behind `CRYPTOVOL_STATIC_{FAT,EXFAT,NTFS}_LFN_FIXTURE`. A routine `cargo test --workspace --all-targets` run (including this repository's own past full-workspace-check passes) silently reports these as "ignored," not failing — which is exactly how a real regression (exFAT/NTFS `stat()` never returning a path, only caught later via manual GUI use) sat unnoticed even though a test for it already existed. Whenever the static fixtures are available locally (they are, in this repo, under `testdata/static/`), run fixture-gated tests explicitly as part of a milestone's final verification:

```bash
CRYPTOVOL_STATIC_FAT_LFN_FIXTURE=$(pwd)/testdata/static/tcvc-aes-sha512-fat-lfn-unicode.hc \
CRYPTOVOL_STATIC_EXFAT_LFN_FIXTURE=$(pwd)/testdata/static/tcvc-aes-sha512-exfat-lfn-unicode.hc \
CRYPTOVOL_STATIC_NTFS_LFN_FIXTURE=$(pwd)/testdata/static/tcvc-aes-sha512-ntfs-lfn-unicode.hc \
  cargo test --workspace --all-targets -- --ignored
```

### What Frontend Integration Tests Cover

Located under `apps/cryptovol-gui/src/**` (co-located `*.test.tsx`/`*.integration.test.tsx` next to the code they test). They render real pages/widgets with React Testing Library and mock only `apps/cryptovol-gui/src/shared/api/commands.ts` via Vitest's `__mocks__` convention (`src/shared/api/__mocks__/commands.ts`) — no component, widget, or feature internals are mocked. Covered behaviors:

* Open Volume page: required field rendering, PIM validation (empty accepted, invalid rejected), KDF option list, wrong-password sanitized error, password never rendered, successful-open transition to Volume Browser with safe volume info, password cleared on success.
* Directory browser: entries rendered from a mocked `listDir`, directory selection shows the extraction-unsupported message.
* **The FAT stat-path regression**, explicitly: `selectingFatFileWithStatPathEnablesExtraction` in `src/pages/volume-browser/ui/VolumeBrowserPage.test.tsx` asserts that selecting a file whose mocked `stat` response has a path enables extraction, and that a `stat` response with no path keeps it disabled with a clear message.
* Extraction wiring: `extractFile` called with the expected DTO, progress/cancel/success/error state transitions, progress rendered as byte counts only (no raw content, no stack traces).

Shared DTO builders live in `apps/cryptovol-gui/src/shared/testing/dto-builders.ts` (a `shared/` sub-segment, not a new top-level `src/` folder).

**One deliberate, narrow exception:** `apps/cryptovol-gui/src/shared/api/commands.test.ts` does *not* follow the "mock only `shared/api/commands`" rule above -- it is the one file that exercises `shared/api/commands.ts`'s own real implementation, mocking only the lower `@tauri-apps/api/event`/`@tauri-apps/api/core` layer beneath it. This exists because a real bug (the extraction event race, see [known-issues.md](known-issues.md)) lived *inside* `subscribeToExtractionEvents` itself, in the seam between the "mocked commands" layer and the "real IPC" layer -- neither of the two layers above would ever have caught it: the frontend-integration layer mocks `commands.ts` away entirely, and the Tauri E2E layer can't force a same-tick-fast completion deterministically without an artificial delay (see "Extraction Cancellation Coverage" below for the same constraint applied to a different scenario). Do not extend this pattern to other commands without a similarly concrete reason -- the two-layer split above is still the default for everything else in `shared/api/`.

### What Tauri E2E Tests Cover

Located under `apps/cryptovol-gui/e2e/`. Uses WebdriverIO + `@wdio/tauri-service`'s **embedded** WebDriver provider to launch the real compiled `cryptovol-gui` binary and drive it through real Tauri IPC — not `*_impl` Rust calls, not a browser-only frontend render. Specs:

* `specs/gui-smoke.e2e.ts`: app starts (Open Volume page visible), wrong password (sanitized error, password never rendered), and a documented rationale for why extraction cancellation isn't covered here (see below).
* `specs/fixtures.e2e.ts`: for each of FAT, exFAT, and NTFS — open/browse/select/extract regression (`{fat,exfat,ntfs}FixtureSelectionExtractionRegression`, sharing an `extractRocketFileAndVerify()` helper since all three static fixtures document the same file tree and ground truth — extracting a real file and comparing bytes against its ground-truth original) plus root-listing assertions; FAT additionally covers directory-extraction-unsupported.

## Why E2E Is Local-Only, And Why No CI

This milestone intentionally does not add any CI configuration (GitHub Actions, GitLab CI, Forgejo, Woodpecker, Buildkite, CircleCI, or any other hosting-specific automation) — repository hosting has not been decided yet, and adding CI config for an undecided host would need to be redone or removed later. The E2E harness is designed to be fully reproducible on a developer's own machine (it builds the app itself via `onPrepare`; it does not depend on a human having run `npm run tauri dev` or `npm run build` beforehand) so that whichever hosting platform is chosen later can wire the same `npm run test:e2e` command into CI without any changes to this harness.

## Installing And Running

From `apps/cryptovol-gui/`:

```bash
npm install                # installs both the app and its Vitest/WebdriverIO devDependencies
npm run typecheck          # tsc --noEmit
npm run build               # tsc && vite build

npm test                    # Vitest, frontend integration tests, single run
npm run test:watch          # Vitest, watch mode

npm run test:e2e             # WebdriverIO, builds the app then runs every e2e/specs/*.e2e.ts (window hidden; see caveat 10)
npm run test:e2e:debug       # same, with --logLevel debug (verbose WebDriver command/response logging)
npm run test:e2e:headed      # same as test:e2e, but the app window is visible for the whole run (see caveat 10)
```

`npm run test:e2e` (and `:debug`) builds the frontend (`npm run build`) and then the Rust binary (`cargo build --package cryptovol-gui --features e2e-webdriver`) as part of `wdio.conf.ts`'s `onPrepare` hook — no manual pre-build step is required. The first run compiles from scratch and is slow; subsequent runs are incremental.

### Debugging A Failing E2E Spec

* `npm run test:e2e:debug` prints every WebDriver command and its raw JSON request/response — the fastest way to see exactly what the harness sent and what came back.
* To iterate on a single spec file without re-running the whole suite: `npx wdio run e2e/wdio.conf.ts --spec e2e/specs/fixtures.e2e.ts`.
* To iterate on a single test by name: add `--mochaOpts.grep "some test name substring"`.
* To inspect the app/plugin directly without WebdriverIO at all (useful for isolating whether a failure is in the harness or the app): build with the feature and launch the binary manually with the WebDriver port set, then talk to it with plain HTTP:

  ```bash
  cargo build -p cryptovol-gui --features e2e-webdriver
  TAURI_WEBDRIVER_PORT=4445 ./target/debug/cryptovol-gui &
  curl -s http://127.0.0.1:4445/status
  curl -s -X POST http://127.0.0.1:4445/session -H "Content-Type: application/json" -d '{"capabilities":{}}'
  ```

  This is how the platform caveats below were actually diagnosed.

## Fixture Requirements

E2E and manual testing share the same committed static fixtures (see [test-containers.md](test-containers.md) for full contents/hashes):

```text
testdata/static/tcvc-aes-sha512-fat-lfn-unicode.hc
testdata/static/tcvc-aes-sha512-exfat-lfn-unicode.hc
testdata/static/tcvc-aes-sha512-ntfs-lfn-unicode.hc
testdata/static/fs-fat-lfn-original/           (ground truth for byte-for-byte extraction checks)
```

Password for all of them: `test-password`. `apps/cryptovol-gui/e2e/support/paths.ts` resolves every fixture path from the repo root, so specs never depend on the process's working directory. No VeraCrypt CLI and no generated fixtures are required — only these four committed paths.

## Temporary Output Behavior

Each E2E test that extracts a file creates its own fresh OS-temp-directory destination via `support/paths.ts`'s `createExtractionTempDir()` (`node:fs/promises.mkdtemp` under `os.tmpdir()`), and removes it in an `afterEach` via `cleanupExtractionTempDir()`, regardless of pass/fail. Nothing is written into the repository, and no fixture file under `testdata/static/` is ever modified — every test run confirms this with `git status --short testdata/`.

## Stable Test Selector Policy

Selectors use `data-testid` on stable interaction points (see `apps/cryptovol-gui/e2e/support/selectors.ts` for the full list, mirrored 1:1 with the `data-testid` values in `src/`).

**Directory entries use the generic row-plus-attributes pattern, not literal per-filename testids**: every row has `data-testid="directory-entry-row"` plus `data-entry-name={entry.name}` and `data-entry-type={entry.isDir ? "directory" : "file"}`. Tests query by combining all three (`[data-testid="directory-entry-row"][data-entry-name="..."][data-entry-type="..."]`). This was a deliberate choice over encoding every fixture filename as its own testid: it scales to any directory contents without adding a new selector per file, it doesn't leak filenames into the selector layer as a second source of truth, and it stays stable across styling/layout changes. It does not depend on any MUI-generated class name.

Rules carried over from the frontend policy: selectors never contain secrets or expose decrypted file contents, `data-testid` is added sparingly (not to every element), and no randomly-generated IDs are used.

## Known macOS Platform Caveats

These were all found and fixed while building this milestone; documented here so they aren't rediscovered.

1. **The embedded WebDriver provider needs dev-only plugins, feature-gated.** `@wdio/tauri-service`'s `driverProvider: "embedded"` requires `tauri-plugin-wdio-webdriver` registered in the Rust app, and `browser.tauri.execute()`/mocking/window-focus polling additionally requires `tauri-plugin-wdio` (see item 7). Both are compiled in *only* under the `e2e-webdriver` Cargo feature (`apps/cryptovol-gui/src-tauri/Cargo.toml`), which is never enabled by the default build, `npm run tauri dev`, or `npm run tauri build` — confirmed by `cargo tree -p cryptovol-gui` (feature off: both absent; feature on: both present).

2. **The plugins also need capability entries that would break the default build if added directly.** Their READMEs say to add `"wdio-webdriver:default"`/`"wdio:default"` to `capabilities/default.json` — but referencing those permissions when the plugins aren't linked in fails Tauri's build-time ACL resolution outright (confirmed: `Permission wdio-webdriver:default not found`). `build.rs` writes a *separate* `capabilities/e2e-webdriver.json` file with both permissions, and only while `CARGO_FEATURE_E2E_WEBDRIVER` is set; every other build removes it if present. This file is gitignored (`apps/cryptovol-gui/src-tauri/.gitignore`) since it is always build-generated, never a static source file.

3. **A plain `cargo build` never embeds the frontend.** Tauri only serves the bundled `dist/` (vs. `tauri.conf.json`'s `devUrl`) when the `tauri` crate's own `custom-protocol` feature is enabled — normally set implicitly by the `tauri` CLI's `dev`/`build` wrappers, which this harness bypasses by calling `cargo build` directly. The `e2e-webdriver` Cargo feature also enables `tauri/custom-protocol` for exactly this reason, and `wdio.conf.ts`'s `onPrepare` runs `npm run build` (Vite) before `cargo build`, mirroring what `tauri build`'s `beforeBuildCommand` would otherwise do. Without this, the app window is blank (confirmed via screenshot) because it's trying to load a dev server that isn't running.

4. **`@wdio/tauri-service@1.2.0` has a broken dependency pin.** It imports `installMockSyncOverride` from `@wdio/native-utils@2.4.0`, but that export only exists starting `2.5.0` (published a day later). Worked around with an npm `overrides` entry in `apps/cryptovol-gui/package.json` (`"@wdio/native-utils": "2.5.0"`). If this milestone's `@wdio/tauri-service` version is ever bumped, check whether the override is still needed.

5. **This WebView's synthetic clicks never produce a native `dblclick` event.** Confirmed by instrumenting `click`/`dblclick` listeners directly: two rapid synthetic clicks report `event.detail: 0` and no `dblclick` fires. `DirectoryBrowser`'s navigate-into relies on `onDoubleClick`, so directory navigation in E2E dispatches a real `dblclick` `MouseEvent` via `browser.execute(...)` instead of `element.doubleClick()` (see `dispatchDoubleClick` in `specs/fixtures.e2e.ts`). React's delegated listener picks up the programmatically dispatched event correctly.

6. **MUI's Select doesn't open on a native WebDriver click either.** A click focuses the trigger but never opens the popup (confirmed via `aria-expanded` staying `"false"`). Sending `Enter` after the click does open it — matching MUI's own keyboard-accessible behavior. See the KDF-select pattern repeated in every spec that opens a container.

7. **(Resolved 2026-07-03) Every WebDriver command used to pay a real ~5 second tax.** `@wdio/tauri-service`'s internal `ensureActiveWindowFocus` housekeeping runs before each command (for `getTitle`/`findElement`/`findElements`/`$`/`$$`/`elementClick`) and falls back to a 5-second internal-eval timeout when `window.__wdio_original_core__` isn't set — which happens only when the `@wdio/tauri-plugin` frontend companion isn't loaded. This was originally treated as "optional tooling for richer mocking features this milestone doesn't need" and left unaddressed, with `wdio.conf.ts`'s generous `mochaOpts.timeout` (300s) absorbing the cost instead. That assumption was wrong: the same plugin also eliminates this per-command tax, which was the actual cause of the suite feeling like it "just sits" during a run. Fixed by installing the full plugin pair:
   - Rust: `tauri-plugin-wdio` (execute/mocking API) registered in `register_e2e_plugins` (`src/lib.rs`) alongside the existing `tauri-plugin-wdio-webdriver`, both gated behind `e2e-webdriver` in `Cargo.toml`; its `wdio:default` capability added to `build.rs`'s generated `capabilities/e2e-webdriver.json` (see item 2).
   - Frontend: `@wdio/tauri-plugin` added as a devDependency. It must never ship in production, and there was no existing Vite-build-time signal distinguishing an E2E build from a normal one, so `wdio.conf.ts`'s `buildAppForE2e()` now passes `VITE_E2E_WEBDRIVER=1` to the `npm run build` step, and `src/app/main.tsx` dynamically `import("@wdio/tauri-plugin")`s before rendering only when that env var is set (dynamic import keeps it out of the production bundle's static graph).

   Confirmed fixed: a full fresh `npm run test:e2e:debug` run (both spec files, 8 tests) now completes in ~64 seconds total with zero occurrences of `Tauri core.invoke not available after 5s timeout` in the log, versus several minutes before. `mochaOpts.timeout` in `wdio.conf.ts` was lowered from 300s to 60s accordingly (still a wide margin over the ~6s average per-test time observed).

8. **The embedded app process is not restarted between spec files.** WebdriverIO gives each spec file its own WebDriver *session*, but `@wdio/tauri-service`'s embedded provider reuses the same underlying app *process* (and therefore its React navigation state) across every spec file in one `npm run test:e2e` run — even after a fully passing test. A test that reaches the Volume Browser page and doesn't navigate back leaves the *next* spec file's session starting from that same state. Every spec in `fixtures.e2e.ts` returns to the Open Volume page in an `afterEach` (`returnToOpenVolumeIfNeeded()`, using the `volume-browser-back-button` testid) so specs stay independently composable.

9. **(2026-07-03, believed resolved by item 7's fix) The item-7 "~5 second tax" could occasionally become an unbounded hang.** On at least one local run (before the item-7 fix), `ensureActiveWindowFocus`'s `Tauri core.invoke not available after 5s timeout` warning repeated indefinitely (15+ minutes, across two independent `npm run test:e2e` invocations) without ever reaching a single spec's first WebDriver command — no pass, no fail, no mocha per-test timeout ever fired. This was confirmed at the time to be a harness issue, not an app regression: launching the same `e2e-webdriver`-featured binary directly and opening a *plain* WebDriver session against it worked immediately. Every individual timeout inside `@wdio/tauri-service` (5s inner poll, 30s/35s outer HTTP abort, 30s window-handle wait, 60s embedded-driver start) is bounded on its own, so the true unbounded hang was most likely some outer retry (e.g. WebdriverIO's own `waitForDisplayed`/`$` polling) re-triggering the same broken fallback path with nothing capping the total iteration count. Since this fallback path only exists when `window.__wdio_original_core__` is unset, installing `@wdio/tauri-plugin` (item 7) removes the only known trigger. Multiple fresh `npm run test:e2e`/`test:e2e:debug` runs after that fix completed normally with no stale-process interference and no recurrence. If this ever recurs: verify no stale `cryptovol-gui`/`wdio` process is still running from a prior interrupted run first (a stale process alone reproduces the same symptom); if a clean run still hangs even with the plugin installed, that means this and item 7 were only coincidentally related, and `@wdio/tauri-service`'s session/capability negotiation needs its own investigation.

10. **The suite runs with the app window hidden by default -- and disabling it uncovered a real macOS App Nap interaction.** `npm run test:e2e` launched a real, visible, focus-stealing window on every run, which blocked using the machine for anything else while it ran in the background. Fixed in two parts:

    **Part A -- hide the window.** `wdio.conf.ts`'s `buildTauriConfigOverlay()` overlays `visible: false` onto `app.windows[0]` in the `TAURI_CONFIG` build-time overlay (the same mechanism already used for `withGlobalTauri`, see item 1/7 above). This is derived from the real base window object in `tauri.conf.json` (`{ ...readBaseWindowConfig(), visible: ... }`), not a hardcoded second copy of `title`/`width`/`height` -- `TAURI_CONFIG` is applied via RFC 7386 JSON Merge Patch (`tauri-build`'s `try_build` calls `json_patch::merge`), which **replaces arrays wholesale, never merges elements by index**. An overlay that set `app.windows` to a partial object (e.g. just `{ visible: false }`) would silently drop `title`/`width`/`height` for e2e builds only -- do not "simplify" this back into that bug. Scoped strictly to the `e2e-webdriver` build: `npm run tauri dev`, `npm run tauri build`, and the raw manual-debug binary launch a few paragraphs up (`cargo build -p cryptovol-gui --features e2e-webdriver` with no `TAURI_CONFIG` set) are all unaffected and stay visible, since none of them set `TAURI_CONFIG`. Confirmed via `@wdio/tauri-service`'s own window-selection code (`ensureActiveWindowFocus` in `node_modules/@wdio/tauri-service/dist/esm/index.js`): it uses a window's `is_visible`/`is_focused` state only to disambiguate *which* window to drive when an app has several; its final fallback is unconditionally `states[0]`, and this app has exactly one window, so hiding it doesn't change which window gets driven. To watch the suite run (e.g. debugging a flaky visual issue), use `npm run test:e2e:headed` (sets `CRYPTOVOL_E2E_HEADED=1`) instead.

    **Part B -- disable macOS App Nap for the process.** Hiding the window surfaced a real, reproducible regression: `specs/gui-smoke.e2e.ts`'s "Wrong password" test (which clicks the KDF `Select`, presses Enter, clicks the `SHA-512` option, then clicks Submit) started intermittently failing with `WebDriverError: Script execution timed out when running "element/.../click"`. Measured across 5 full `npm run test:e2e` runs before any fix: 3 passed (~1m51s-1m52s each), 2 failed with that exact error (taking 2m00s-2m46s before finally timing out) -- always the same test. In contrast, 3 `npm run test:e2e:headed` runs (window visible) all passed in ~51-52s with zero failures. Isolating just the failing spec (`--mochaOpts.grep "Wrong password"`) and re-running it alone 4 times in a row passed every time, which pointed away from a bug in that spec itself and toward something specific to the *combination* of a hidden window and a long-lived process (this harness reuses one app process across both spec files -- see item 8).

    Root cause: **macOS App Nap.** Confirmed via `tauri-plugin-wdio-webdriver`'s own source (`~/.cargo/registry/src/.../tauri-plugin-wdio-webdriver-1.2.0/src/platform/executor.rs`, `click_element`) that the WebDriver `click` command is implemented purely as a JS `el.click()` evaluated in the WKWebView via `evaluateJavaScript` -- not any native mouse-event synthesis -- so a native input-routing bug wasn't the cause; the timeout had to be in the JS-evaluation bridge itself. Apple's own App Nap documentation states it throttles a background process's timers/I-O once the process "isn't the foreground app" and "hasn't recently updated content in the *visible* portion of a window" -- our hidden window has no visible portion, ever, so the e2e-webdriver process is a permanent App Nap candidate, while a real visible window (headed mode) is exempt. This also explains why the failure correlated with whether the machine was otherwise in active use at the time (observed directly during this investigation): App Nap's heuristics factor in overall system activity, so a napped background process is throttled harder when competing with other foreground work, and barely throttled at all on an otherwise-idle machine.

    Fix: `apps/cryptovol-gui/src-tauri/src/app_nap.rs` (macOS-only, compiled in only under the `e2e-webdriver` feature) holds an `NSProcessInfo` activity assertion (`beginActivityWithOptions(_:reason:)` with `NSActivityUserInitiated | NSActivityLatencyCritical`, via the `objc2`/`objc2-foundation` crates -- already present in `Cargo.lock` as a transitive dependency of `tauri`/`wry`/`tao` themselves, so this adds no new crate to the dependency tree, only a direct reference to what's already compiled in) for the lifetime of the process, in `lib.rs`'s `run()`. This runs unconditionally whenever `e2e-webdriver` is compiled in (headed or headless) since disabling App Nap is harmless for an already-visible window.

    **Confirmed fixed:** 6 further `npm run test:e2e` runs after adding the `NSProcessInfo` fix. 2 with no artificial load both passed in ~51-52s (now matching the headed baseline exactly, down from ~1m51s+ before the fix). 4 more were run under deliberately extreme concurrent CPU load (two `yes > /dev/null` loops pinning cores, well beyond normal laptop-use levels) to stress-test the fix: the "Wrong password" test itself passed in all 4 (11-14s each, vs previously up to 60s+ before eventually failing), and one of those 4 runs still failed elsewhere (`ntfsFixtureSelectionExtractionRegression`, a different, unrelated extraction test) under that synthetic double-`yes` saturation -- a genuine "not enough CPU exists for any process, regardless of App Nap" condition rather than an App-Nap-specific one, and not representative of normal concurrent laptop use (browsing/editing/etc.). Across all 6 post-fix runs the originally-flaky "Wrong password" test passed 6/6 times.

    **Known dormant risk, not yet triggered:** no current spec calls a screenshot/snapshot API. `tauri-plugin-wdio-webdriver`'s macOS backend uses `WKSnapshotConfiguration` for screenshots; its behavior against a window that has never been shown/composited on screen has not been verified. If a future spec adds a screenshot-based assertion, check this first rather than assuming it behaves the same as a visible window.

    **Explicit non-goal:** this is unrelated to, and does not change, the separate manual AppleScript/System-Events-driven visual-audit technique documented in `docs/gui-mvp.md`'s "Beta Visual Audit" section -- that technique still requires (and still gets) a real, visible, frontmost window.

## How To Add A New GUI Regression Test

For a bug found through manual testing (or any UI-observable behavior not yet covered):

1. **Frontend integration test first.** Find or create the `*.test.tsx` file next to the component involved. Mock only `@/shared/api/commands`, using the builders in `@/shared/testing`. Give the test a name that states the regression, e.g. `selectingFatFileWithStatPathEnablesExtraction`. This is the fast layer — run it in a tight loop with `npm run test:watch`.
2. **Add stable selectors if needed.** Add `data-testid` only to the specific element(s) the test needs; follow the generic row-plus-attributes convention for any list/table row. Update `apps/cryptovol-gui/e2e/support/selectors.ts` if the new testid should also be usable from E2E.
3. **E2E test if the bug is specifically about the real IPC/event chain** (i.e. a frontend-integration test with a correctly-shaped mock wouldn't have caught it — which was exactly the case for the original FAT `stat` bug). Add it to `specs/fixtures.e2e.ts` if it needs an open container, or `specs/gui-smoke.e2e.ts` for simpler flows. Reuse `openContainer`/`waitForRow`/`waitForTestId`/`returnToOpenVolumeIfNeeded` rather than duplicating setup.
4. Run the new test in isolation first (`--mochaOpts.grep` for E2E, a filename argument for Vitest), then the full suite, before committing.

## Avoiding Secret Leakage In Tests And Logs

* Test passwords are the public, documented `test-password` (see [test-containers.md](test-containers.md)) — never a real secret — but the same rules apply so the pattern stays safe if fixtures ever change.
* Frontend integration tests that exercise a wrong-password flow assert the entered password is *absent* from `container.textContent` (see `OpenVolumePage.test.tsx`), not merely that an error appeared.
* E2E wrong-password specs assert the same against the full rendered page (`browser.$("body").getText()`), and pick an explicit KDF hint rather than leaving it on "Auto" — Auto forces an exhaustive KDF autoprobe on a wrong password (every KDF/header-candidate combination has to fail first), which is both slow and encourages leaving a password on screen for an extended, disproportionate wait.
* Do not add `console.log`/`captureFrontendLogs`/`captureBackendLogs` around code paths that see passwords, derived keys, or decrypted content — none of the specs in this milestone enable those `@wdio/tauri-service` options.
* Do not commit E2E debug output, screenshots, or `--logLevel debug` transcripts — they are not written to the repository by anything in this harness, and should stay that way.

## Extraction Cancellation Coverage

Deliberately not covered by a real E2E spec — see the comment block above the E2E7 note in `apps/cryptovol-gui/e2e/specs/gui-smoke.e2e.ts` for the full rationale. Summary: the largest file across every committed static fixture (89,489 bytes) extracts in ~0.26 seconds end to end, measured directly against the real app — far faster than the ~5 second per-command tax documented above, so a click-extract-then-click-cancel race cannot land mid-extraction reliably without adding an artificial delay to production code, which is out of scope for this milestone. Cancellation is instead covered deterministically at:

* **Frontend wiring**: `ExtractionPanel.integration.test.tsx` asserts clicking `extract-cancel` calls `cancelExtract(jobId)` with the running job's id.
* **`cryptovol-app`**: `crates/cryptovol-app/tests/cancellation_token.rs` (token semantics), `progress_and_cancellation.rs` and `extract_file_fixtures.rs` (`cancellation_before_copy_starts_returns_cancelled_with_no_destination`, `cancellation_mid_copy_stops_early_and_leaves_no_destination`, `cancellation_mid_extraction_leaves_no_destination_file` — exercised via an injected cancellation point, not a timing race).
* **Tauri command layer**: `apps/cryptovol-gui/src-tauri/tests/job_registry.rs`, `extract_job.rs` (`run_extraction_job_cancelled_mid_copy_emits_cancelled_not_finished`, `cancel_extract_impl_on_unknown_job_returns_job_not_found`), and `session_registry.rs` (`close_session_cancels_active_jobs_then_removes_the_session`).
