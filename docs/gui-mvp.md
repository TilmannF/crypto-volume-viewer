# GUI MVP

`apps/cryptovol-gui` is a desktop GUI frontend for `cryptovol`, built as a spike on top of the existing framework-neutral `cryptovol-app` core. It reuses exactly the same TC/VC opening, directory listing, and extraction logic as `cryptovol-cli` — no container/volume/filesystem logic is duplicated in the GUI layer. See [architecture.md](architecture.md) and [app-core.md](app-core.md) for how `cryptovol-app` fits into the crate layout.

## Stack And Why Tauri

The GUI stack is Tauri 2, React, TypeScript, Vite, and Material UI (MUI).

Tauri was chosen for this first spike because:

* It wraps the OS's native WebView (WKWebView on macOS, WebView2 on Windows, WebKitGTK on Linux) instead of bundling a full Chromium runtime, keeping binary size and memory footprint small compared to Electron.
* Its Rust backend can depend on `cryptovol-app` directly as a normal path dependency, with no FFI or IPC boundary needed between the GUI process and the application core.
* Its command/event model (`#[tauri::command]` + `emit`/`listen`) maps cleanly onto `cryptovol-app`'s existing request/response and progress-callback shapes.

This is a reversible choice: `cryptovol-app` has no knowledge of Tauri (see [app-core.md](app-core.md)'s "Why GUI Framework Choice Is Deferred" section), so a future switch to a different Rust-native GUI toolkit would only require rewriting `apps/cryptovol-gui`, not any lower layer.

## Frontend Folder Structure

`apps/cryptovol-gui/src/` follows a layered, one-way import direction (enforced by `policies/30-frontend-policy.md`):

```text
app/       top-level bootstrap: App.tsx, main.tsx, MUI theme
pages/     route-level screens that compose widgets + features
widgets/   composed UI regions (props/hooks in, no direct backend calls)
features/  one user-facing action per slice, wraps shared/api calls
entities/  pure display/model helpers for backend DTOs, no backend calls
shared/    api client (commands.ts/dto.ts/errors.ts), config, generic UI
```

Import direction is one-way: `app -> pages -> widgets -> features -> entities -> shared`. `shared/api/commands.ts` is the *only* file in the frontend that imports `invoke`/`listen` from `@tauri-apps/api`; every other layer reaches the backend only through typed functions re-exported from `commands.ts` via a feature's `index.ts`.

Notable widgets: `directory-browser` (entry listing/navigation/selection; also owns the toolbar's Close button and scrolls its table internally so the page itself never scrolls), `extraction-panel` (destination field, Browse/Extract/Cancel, and progress/result display in a single dense row), and `status-bar` (a one-line strip showing volume facts on the left and, on the right, the selected entry's name and, for a file with a known path, that path — or a clear warning when a non-directory entry has no usable path; replaced the former `volume-info-panel` and `selected-entry-panel` widgets). `VolumeBrowserPage`'s `canStartExtraction` requires the selected entry to be a non-directory *with* a known `path`, not just a non-empty destination path — this closes the gap the FAT `stat` bug (below) originally exploited, where a path-less selection could otherwise leave the Extract button enabled and only fail after the click.

### Dense UI

The MUI theme (`apps/cryptovol-gui/src/app/theme/theme.ts`) is deliberately dense — 13px base font, a 4px spacing unit, and ~22-24px table rows — with Total Commander as the reference design direction rather than default Material spacing. The Volume Browser page is a fixed-height layout (toolbar, table, extraction row, status bar) where only the directory table scrolls internally; the page itself never scrolls.

## Tauri Commands

Nine `#[tauri::command]` functions are registered in `apps/cryptovol-gui/src-tauri/src/lib.rs`:

| Command | Purpose |
| --- | --- |
| `inspect_container` | Password-free container inspection (file size, header candidate state) |
| `open_container` | Opens a TC/VC volume with a password (+ optional PIM/KDF hint), returns a new session |
| `list_dir` | Lists a directory's entries for an open session |
| `stat` | Metadata for a single file/directory for an open session |
| `close_session` | Cancels the session's active extraction jobs, then closes it |
| `extract_file` | Validates and starts a background single-file extraction job |
| `cancel_extract` | Cancels an in-flight extraction job |
| `select_container_file` | Native open-file picker for the container path |
| `select_extract_destination` | Native save-file picker for the extraction destination |

Every command function is a thin wrapper around a plain, independently testable `*_impl` function of the same name in `apps/cryptovol-gui/src-tauri/src/commands/{container,session,extraction}.rs`, so command logic is covered by ordinary `cargo test` without booting a real Tauri runtime.

## DTOs

`apps/cryptovol-gui/src-tauri/src/dto/` defines the wire types (`#[serde(rename_all = "camelCase")]`, so the frontend sees `camelCase` field names): `GuiContainerInfoDto`, `OpenContainerRequestDto`/`OpenContainerResponseDto`, `VolumeInfoDto`, `FileEntryDto`/`AppTimestampDto`, `ExtractFileRequestDto`/`ExtractStartedDto`, and `GuiErrorDto`. `OpenContainerRequestDto` derives only `Deserialize` — never `Debug`/`Clone`/`Copy` — so a stray `{:?}` on it can't leak the password into logs; this is enforced by a guard test (`no_password_leak.rs`). The frontend mirrors every DTO field-for-field in `apps/cryptovol-gui/src/shared/api/dto.ts`.

`GuiErrorDto` is the single error shape every command returns on failure: a stable `code` and a short, sanitized `message`. The stable codes (`apps/cryptovol-gui/src-tauri/src/dto/error.rs`) are: `auth_failed`, `unsupported_format`, `filesystem_not_recognized`, `path_not_found`, `directory_extraction_unsupported`, `unsupported_feature`, `cancelled`, `io_error`, `invalid_input`, `session_not_found`, `job_not_found`, `internal_error`. `apps/cryptovol-gui/src-tauri/src/error.rs` implements `From<cryptovol_app::AppError> for GuiErrorDto`, mapping every `AppError` variant to one of these codes with a short, secret-free message (`AppError::Io`/`ExtractionFailed` both map to `io_error`).

## Session And Extraction Job Registries

`apps/cryptovol-gui/src-tauri/src/state.rs` defines `GuiState`, Tauri-managed application state holding two in-memory maps behind separate mutexes:

* `sessions: HashMap<SessionId, cryptovol_app::VolumeSession>`
* `extraction_jobs: HashMap<JobId, ExtractionJob>` (each job holds its owning `SessionId` and a `cryptovol_app::CancellationToken`)

`SessionId`/`JobId` are opaque, randomly generated UUIDs (`uuid::Uuid::new_v4()`) — never derived from the container path, password, PIM, KDF, or a timestamp — so they carry no information about what they open. `GuiState` never stores the password, derived keys, or raw decrypted data; a source-text guard test asserts `state.rs` never contains the substring for the secret it must not hold.

**Cancel-then-close behavior:** `close_session` always succeeds for a known session id, even with active extraction jobs. It first cancels and removes every extraction job owned by that session (calling `CancellationToken::cancel()` on each), then removes the session itself. There is no "reject close while jobs are active" path.

## Extraction Job Lifecycle And Progress Events

`extract_file` validates the request synchronously (session lookup, then `VolumeSession::stat` to reject directory extraction *before* any job is created — `directory_extraction_unsupported`, not a job that immediately fails), registers the job, and returns `ExtractStartedDto` immediately. The actual copy runs on a spawned `std::thread`, calling `VolumeSession::extract_file` and translating each `cryptovol_app::ProgressEvent` into a GUI-facing event emitted to the frontend via `AppHandle::emit`.

Five events are emitted, defined in `apps/cryptovol-gui/src-tauri/src/events.rs`:

| Event name | When |
| --- | --- |
| `extract://started` | Once, when the copy begins (mirrors `ProgressEvent::Started`) |
| `extract://progress` | After each chunk written to the destination (mirrors `ProgressEvent::Advanced`) |
| `extract://finished` | Once, on success (mirrors `ProgressEvent::Finished`) |
| `extract://cancelled` | Once, if the job's `CancellationToken` was cancelled |
| `extract://failed` | Once, for any other terminal error, carrying the mapped `GuiErrorDto` code/message |

Every payload carries only job/session ids, paths, and byte counts — never passwords, keys, decrypted header bytes, or decrypted file contents. The job is removed from `GuiState`'s registry once it reaches any terminal state. `cancel_extract` cancels and removes the job's `CancellationToken`; cancelling an unknown or already-finished job id returns `job_not_found` rather than panicking. Cancellation relies on `cryptovol-app`'s existing streaming-writer guarantee that a cancelled or failed extraction never leaves a partially written file at the destination (see [streaming-extraction.md](streaming-extraction.md)) — the GUI layer does not duplicate that logic.

## Dialog Permissions

`apps/cryptovol-gui/src-tauri/capabilities/default.json` grants only `core:default` plus `dialog:allow-open` and `dialog:allow-save` (via the official `tauri-plugin-dialog` crate). No shell, broad filesystem, network, clipboard, or updater permissions are enabled. Manual path text entry remains a fully functional fallback in both `OpenVolumePage` and the extraction destination field if a dialog is dismissed or unavailable.

## Security Model

* The password field is a native MUI `TextField` with `type="password"`; the value is held only in local React state and is cleared (`setPassword("")`) immediately after a successful `open_container` call.
* `OpenContainerRequestDto` cannot be accidentally logged: it has no `Debug`/`Clone` derive on the Rust side, and the frontend never `console.log`s raw request objects.
* No GUI code path prints, logs, or displays a password, derived key, decrypted header bytes, or raw decrypted file content — errors and events are always the sanitized `GuiErrorDto`/`extract://*` shapes described above.
* `cryptovol-app`'s own no-terminal-io guard test (`crates/cryptovol-app/tests/no_terminal_io.rs`) additionally forbids `clap` and `tauri` appearing in `cryptovol-app`'s source, keeping the shared core provably framework-neutral.

## Automated Test Coverage

The manual smoke test below found a real bug (see "Bug found and fixed during this pass") that the entire automated suite at the time missed, because those tests never exercised the real chain from React UI through Tauri IPC to `cryptovol-app`. That chain is now covered by two additional durable, version-controlled test layers on top of the existing Rust suite: frontend integration tests (Vitest + React Testing Library, mocking only `shared/api/commands`) and a persisted local Tauri E2E harness (WebdriverIO + `@wdio/tauri-service`) that drives the real built app. See [gui-testing.md](gui-testing.md) for the full test strategy, how to run/debug each layer, and the platform caveats found while building it. This manual smoke test procedure remains useful for ad hoc verification and is not replaced by the automated layers.

## Manual Smoke Test

Steps and observed results as of the final regression pass (macOS, `npm run tauri dev`):

1. Launch the app — Open Volume page renders with Container path / Password / PIM (optional) / KDF fields and an Open button. **Pass.**
2. Open `testdata/static/tcvc-aes-sha512-fat-lfn-unicode.hc` with password `test-password` — navigates to the Volume Browser page, showing `Filesystem: FAT`, `Cipher: AES-XTS`, `KDF/hash: SHA-512`, `PIM: Default`, `Header: Primary`, `Read-only: yes`, and a root listing matching the documented fixture exactly (`System Volume Information`, `Folder With Spaces`, `Emoji Rocket 🚀 Test.txt`, `Please Do Not Open 😅.txt`, `Project Notes Final.txt`, the Sydney Sweeney JPEG, `Unicode Umlaut äöü ÄÖÜ ß.txt`, `$RECYCLE.BIN`). **Pass.**
3. Navigate into `Folder With Spaces` — lists `Rocket Science 🚀 For Beginners.txt` (43 bytes), matching the documented fixture. **Pass.**
4. Selecting a directory — `DirectoryBrowser` shows the info alert "Directory extraction is not supported in this version. Select a file to extract it." **Pass.**
5. Extract `Project Notes Final.txt` — **initially failed**: a real bug was found and fixed during this pass (see below). After the fix, extraction succeeds, the destination file is byte-for-byte identical to `testdata/static/fs-fat-lfn-original/Project Notes Final.txt`, and the UI shows "Extraction finished (49 bytes)." **Pass, after fix.**
6. Wrong password — verified via the automated `open_container_with_wrong_password_returns_auth_failed` test (fast, uses a KDF hint). A manual GUI attempt with KDF left at "Auto" was also started to confirm the same path end-to-end; because a wrong password forces an exhaustive autoprobe across all 5 KDFs (including the slow pure-software Whirlpool/Streebog implementations) at both header candidates, this manual attempt took over 15 minutes of single-core CPU time without yet resolving by the time this pass concluded — a real but expected cost of the "Auto" KDF setting against a non-matching password (worst case, no early exit possible), not a bug. Prefer picking an explicit KDF hint in the GUI when testing wrong-password behavior by hand. **Pass (automated); manual GUI path observed correct-but-very-slow, not run to completion in this pass.**
7. Cancelling an in-flight extraction — covered by `extract_job.rs`'s `run_extraction_job_cancelled_mid_copy_emits_cancelled_not_finished` test. **Pass (automated).**

### Bug found and fixed during this pass

Manual GUI extraction consistently failed with "Selected entry has no known path" for every file in the FAT LFN fixture, despite all relevant automated tests passing. Root cause: `VolumeSession::stat_via_parent_listing` (the FAT-specific `stat` fallback, since `FatFileSystem` has no native stat) in `crates/cryptovol-app/src/session.rs` returned the raw entry from `list_dir` unmodified, without setting `FileEntry.path` — contradicting that field's own documented contract ("`stat` fills it in with the queried path"). This broke the GUI's `select()` → `stat()` → `selectedEntry.path` chain (used to build `ExtractFileRequestDto.source_path`) for every FAT-backed volume, since `list_dir` entries never carry `path`. It was invisible to the existing automated suite because `extract_file_impl`'s own tests call `VolumeSession::stat` with an already-known hardcoded path (never reading `.path` off the result), and `list_dir_stat_fixtures.rs`'s `stat` assertions checked `name`/`size`/`is_dir` but never `path`. Fixed by setting `path: Some(path.to_string())` on the matched entry in `stat_via_parent_listing`, and added a regression assertion to `list_dir_stat_fixtures.rs` asserting `stat_entry.path` is the queried path.

**Correction (2026-07-03): exFAT and NTFS had the same bug.** The original write-up above claimed "exFAT and NTFS were unaffected — their `stat` calls go through each filesystem's own native `.stat()`, which already sets `path` correctly." This was wrong. `ExfatEntry`/`NtfsEntry` (`crates/cryptovol-fs-exfat/src/lib.rs`, `crates/cryptovol-fs-ntfs/src/lib.rs`) never carried a `path` field at all, and `VolumeSession::stat()`'s `ExFat`/`Ntfs` match arms (`session.rs`) converted their native `.stat()` result straight through `FileEntry::from(...)`, which — correctly, for `list_dir`'s sake — hardcodes `path: None`. So `stat()` silently returned no path for exFAT/NTFS regardless of nesting depth, exactly mirroring the FAT bug: browsing/listing worked fine (doesn't need `path`), but selecting a file for extraction always showed "Selected entry has no known path." Found via a real user report opening an NTFS container. Fixed the same way, at the `stat()` call site instead of a shared fallback (mirroring FAT's fix pattern: `FileEntry { path: Some(path.to_string()), ..FileEntry::from((entry, filesystem)) }` for both the `ExFat` and `Ntfs` arms). `list_dir_stat_fixtures.rs`'s `path` assertion already covered all three filesystems via a shared helper — it just wasn't being run for exFAT/NTFS in routine checks (see [gui-testing.md](gui-testing.md)'s note on fixture-gated tests). The GUI E2E suite's exFAT/NTFS specs were also extended with the same select+extract+byte-verify regression FAT already had (`apps/cryptovol-gui/e2e/specs/fixtures.e2e.ts`).

## Beta Visual Audit (2026-07-04)

Manual pass against `npm run tauri dev`, driven via macOS Accessibility (System Events) since the app is a native AppKit window with a WKWebView, not a browser. Used `testdata/static/tcvc-aes-sha512-fat-lfn-unicode.hc` with an explicit `SHA-512` KDF hint throughout (never "Auto" for a wrong-password check — see the note in "Manual Smoke Test" above on why that forces a 15+ minute autoprobe).

States checked:

* **Open Volume page, default window (900x640):** renders correctly; the form is a fixed `maxWidth: 480` `Stack` left-aligned in a much wider window, leaving a large empty right/bottom area. Not a regression (this is how the page has always looked) and not fixed here — flagged only as a cosmetic observation, not something worth restructuring for this milestone.
* **Open Volume page, small window (560x400):** no clipping or overlap; form remains fully usable.
* **Volume Browser, empty selection:** clean; dense table renders long filenames (`Sydney Sweeney at the 2025 Toronto International Film Festival.jpg`), emoji (`Emoji Rocket 🚀 Test.txt`, `Please Do Not Open 😅.txt`), and Unicode (`Unicode Umlaut äöü ÄÖÜ ß.txt`) correctly, ellipsis-truncating only the one column too narrow to fit at 560px. **Pass.**
* **Volume Browser, selected directory (`Folder With Spaces`) and selected file with path (`Project Notes Final.txt`), 900px window:** **found and fixed a real bug.** The status bar's right-hand selection segment (`selected-entry-panel`) was over-truncating every value via CSS ellipsis — even a short size like `49 bytes` rendered as `4...` — despite ample free horizontal space, because it had no `flexShrink: 0` and so competed for width with the left-hand volume-facts segment (which already self-truncates its own long container path). Fixed in `apps/cryptovol-gui/src/widgets/status-bar/ui/StatusBar.tsx` by giving `selected-entry-panel` `flexShrink: 0` (so it always gets its natural width, and the left segment — which already degrades gracefully — yields space instead), and added missing `title` tooltips to the selected-entry name and the directory-extraction-unsupported message for graceful degradation at genuinely narrow widths. Re-verified at both 900px and 560px after the fix: full text renders at 900px, and only the left (least-important) segment truncates at 560px. `npm run typecheck` and `npm test` (18/18) still pass, zero test files changed.
* **Wrong password, explicit KDF hint:** clean single-line dense `Alert` reading "Incorrect password, or unsupported volume parameters." — no password/KDF details leaked, matches security policy. **Pass, no issue found.**
* **Extraction of `Project Notes Final.txt` (49 bytes):** **found a real functional bug, not fixed here (out of scope for a docs/layout audit).** The file *is* written correctly to the destination (verified byte-for-byte on disk), but the `ExtractionPanel` UI stayed stuck at "0 bytes / 49 bytes" with `Cancel` still enabled indefinitely, with no visible effect from clicking `Cancel` either. This is a lost-event race between starting the job and subscribing to its events, not a layout issue — full root-cause analysis, reproduction steps, and suggested fixes are recorded in [known-issues.md](known-issues.md) for a dedicated follow-up.
* **Selected file without a path:** not reproduced live (not reachable through normal navigation); covered instead by `VolumeBrowserPage.test.tsx`'s existing mocked-`stat` test, which asserts `selected-entry-error` renders "Selected entry has no known path; extraction is unavailable."

No other layout regressions (clipping, overlap, unreadable spacing) were found. No redesign was attempted; only the `StatusBar` fix above was applied.

### Error/Empty/Loading State Audit (2026-07-04, continued)

A second pass specifically targeted the states the audit above didn't cover: opening, loading, empty directory, unsupported directory extraction, no selection, extraction failed, and extraction cancelled. Verified via source review and the automated test suites (frontend integration + the two new `DirectoryBrowser` tests added for the fix below) rather than a fresh live `tauri dev` screenshot pass, since the machine was in active concurrent use by the repository owner at the time (see [gui-testing.md](gui-testing.md) caveat 10 for the broader reason this project avoids disturbing a foreground session) — the change involved is small and low-risk enough that unit-level verification is sufficient.

* **Opening:** the Open button's label switches to "Opening…" while `state.status === "opening"` (`OpenVolumePage.tsx`). Clear, no change needed.
* **Loading a directory:** the directory table shows only its header row while `loading` is true and no entries have arrived yet -- no spinner or skeleton. Not fixed here: against this project's local fixtures, a directory listing resolves near-instantly, so a loading indicator would only ever flash briefly; adding one risked more visual noise than value for this milestone. Left as a known, minor gap rather than restructured.
* **Empty directory:** **found and fixed a real gap.** With zero entries, the table previously rendered only its header with no body content at all -- visually indistinguishable from "still loading" or "something went wrong." Fixed in `DirectoryBrowser.tsx` by rendering a single centered `This directory is empty.` row (`data-testid="directory-empty-row"`) spanning all four columns, using the existing MUI `TableRow`/`TableCell` components (no new library). Gated on `!loading` specifically so it cannot flash misleadingly during the brief window between navigating and the new entries arriving. Covered by two new tests in `DirectoryBrowser.test.tsx`: the message renders when `entries` is empty and not loading, and is absent while `loading` is true with no entries yet.
* **Unsupported directory extraction:** already covered by the audit above (`StatusBar`'s directory-selected message) -- no change needed.
* **No selection:** the status bar shows only the volume-facts segment; no `selected-entry-panel` renders at all until something is selected (confirmed by `StatusBar.tsx` and its existing tests). Clear, no change needed.
* **Extraction failed:** `ExtractionPanel` renders a dense `severity="error"` `Alert` (`data-testid="extract-error"`) with the error message. Clear, no change needed.
* **Extraction cancelled:** `ExtractionPanel` renders a dense `severity="warning"` `Alert` reading "Extraction cancelled." Clear, no change needed.

No toast library or new UI framework was introduced; every fix reuses the existing MUI `Table`/`Alert`/`Typography` patterns already used throughout the app.

## Dev Commands

```bash
cd apps/cryptovol-gui
npm install
npm run dev         # Vite dev server only
npm run tauri dev   # full app: Vite + Rust backend + native window
npm run build        # tsc -b && vite build
npm run typecheck    # tsc --noEmit
npm run tauri build  # packaged app (not exercised on every change; see limitations below)

npm test              # frontend integration tests (Vitest)
npm run test:e2e       # persisted local Tauri E2E suite (WebdriverIO) -- see gui-testing.md
```

Rust-side commands run from the repository root as usual: `cargo build -p cryptovol-gui`, `cargo clippy -p cryptovol-gui --all-targets -- -D warnings`, `cargo test -p cryptovol-gui`.

## Known Limitations

* No directory extraction — only single files, matching the CLI's current scope.
* No file preview.
* No recent-files list.
* No packaging or code signing.
* No keyfile support.
* No hidden-volume support.
* Single-session UI: `GuiState`'s locking is coarse-grained (one mutex for all sessions, one for all jobs), which is fine for the one-session-at-a-time MVP but would want per-session locking for a future multi-window/multi-session UI.
* No keyboard navigation in the directory table (arrow keys/Enter/Backspace) — considered and explicitly deferred during the gui-density-redesign feature (2026-07-03).
