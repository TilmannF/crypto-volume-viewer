# Known Issues

Confirmed product bugs that are documented for a future fix rather than patched
immediately, because fixing them was out of scope for the task during which
they were found. Each entry has enough context (repro, root cause, file/line
references) for someone with no prior context to pick up and fix it.

## GUI: extraction UI can get stuck at "0 bytes / N bytes" forever

- **Status:** fixed, 2026-07-05. See "Resolution" below.
- **Found:** 2026-07-04, during the GUI visual audit (`T-002` of
  `.work/beta-readiness-hardening/`, see the "Beta Visual Audit (2026-07-04)"
  section in [gui-mvp.md](gui-mvp.md)). Not introduced by that task or any
  other task in that milestone -- pre-existing.
- **Severity:** high for perceived reliability (the single most common GUI
  action -- extract a small file -- can look permanently broken), but the
  underlying copy itself is correct: **no data is lost or corrupted.**

### Symptom

After clicking **Extract** for a small file, `ExtractionPanel` shows the
progress bar stuck at `0 bytes / N bytes` forever. The **Extract** button
stays disabled and **Cancel** stays enabled, but clicking **Cancel** has no
visible effect either -- the panel never reaches the finished, failed, or
cancelled state. There is no error shown to the user.

Despite the frozen UI, **the file is actually written correctly**: verified
by extracting `/Project Notes Final.txt` (49 bytes) from
`testdata/static/tcvc-aes-sha512-fat-lfn-unicode.hc` to a scratch path and
diffing it byte-for-byte against
`testdata/static/fs-fat-lfn-original/Project Notes Final.txt` -- identical,
while the UI was still showing `0 bytes / 49 bytes`.

### Reproduction

1. `cd apps/cryptovol-gui && npm run tauri dev`.
2. Open `testdata/static/tcvc-aes-sha512-fat-lfn-unicode.hc` with password
   `test-password` and KDF hint `SHA-512` (see
   [test-containers.md](test-containers.md)).
3. Select `Project Notes Final.txt` (49 bytes) in the root listing.
4. Type any writable destination path (e.g. a path under `/tmp`) into
   **Destination path**.
5. Click **Extract**.
6. Observe: the progress strip appears showing `0 bytes / 49 bytes` and never
   updates, even after several seconds. The destination file exists on disk
   and is correct. Clicking **Cancel** does not change the displayed state.

Reproduced manually via real GUI interaction (`npm run tauri dev`, driven
through macOS Accessibility/System Events), not through the automated test
suites.

**Open question for whoever picks this up:** the existing Tauri E2E spec
`fatFixtureSelectionExtractionRegression` in
`apps/cryptovol-gui/e2e/specs/fixtures.e2e.ts` extracts a similarly small FAT
fixture file and asserts success, and it passed in the same session this bug
was found in (see `T-001`'s re-verification in
`.work/gui-density-redesign/tasks.yaml`). Before assuming a fix works, figure
out *why* the E2E path apparently avoids or tolerates this race (different
IPC timing under `tauri-plugin-wdio`? a larger effective delay between
`invoke` and `listen` under WebDriver? something else?) -- otherwise a "fix"
that only passes the existing E2E suite may not actually close the real race.

### Root cause (hypothesis, well-supported by the code but not yet proven with a
regression test)

This is a lost-event race between starting the extraction job and
subscribing to its events, on both sides of the Tauri IPC boundary:

- **Frontend:** `apps/cryptovol-gui/src/features/extract-file/model/useExtractFile.ts`,
  function `start` (lines 45-88). It does, in order:
  1. `await extractFile({...})` (line 62) -- an `invoke("extract_file", ...)`
     round-trip (see `apps/cryptovol-gui/src/shared/api/commands.ts` line 83).
  2. `setState(startingExtractionState(started))` (line 69).
  3. `await subscribeToExtractionEvents(started.jobId, {...})` (line 71) --
     which itself does five separate `listen(...)` calls in parallel
     (`commands.ts` lines 119-139), each its own async IPC round-trip to
     register a webview-side event listener.

  Nothing observes or buffers events emitted *before* step 3's listeners are
  actually registered on the webview side.

- **Backend:** `apps/cryptovol-gui/src-tauri/src/commands/extraction.rs`,
  the `#[tauri::command] extract_file` wrapper (lines 162-190). It validates
  the request and registers the job synchronously
  (`extract_file_impl`, returns immediately), then calls
  `std::thread::spawn(...)` (line 175) to run `run_extraction_job` --
  **a real OS thread that starts running concurrently, immediately**,
  copying the file and calling `emit_extraction_event` (line 147) for each
  lifecycle event (`extract://started`, `extract://progress`,
  `extract://finished`) -- and only *then* does the command handler return
  `Ok(started)` (line 189) to the frontend's `invoke`.

  `app.emit(...)` (`emit_extraction_event`, line 150-154) is fire-and-forget:
  if no webview-side listener is registered yet for that event name when
  `emit` is called, the event is simply not delivered -- there is no
  buffering or replay for late subscribers.

For a large file, step 2's background thread takes long enough (real disk/
decrypt I/O across many chunks) that the frontend easily wins the race and
has its listeners registered well before `finished` fires. For a very small
file (a handful of bytes, one chunk), the backend thread can plausibly
finish end-to-end (open, decrypt, write, close, emit all three events) faster
than the frontend's own `invoke` Promise resolves and its five `listen` calls
complete their own round trips -- so `started`, `progress`, and `finished`
can all be emitted and dropped before anyone is listening, leaving
`useExtractFile`'s state stuck at whatever `startingExtractionState(started)`
set it to (a "running" state at 0 bytes), forever.

`cancel_extract` (same file, lines 203-206) doesn't help either: it cancels
the *backend* job, but if the job already finished (as here) there's nothing
left to cancel, and even a genuine cancellation would emit `extract://cancelled`
through the same fire-and-forget `emit`, which the frontend may equally have
missed subscribing to in time.

### Suggested fix directions (not evaluated in depth -- pick one and add a
regression test before shipping)

1. **Register listeners before starting the job.** Split the frontend flow:
   call `subscribeToExtractionEvents` for a job id *before* telling the
   backend to actually start copying, so no event can be emitted before a
   listener exists. This likely requires splitting `extract_file` into two
   backend commands (e.g. "register job" returning a job id synchronously,
   then a separate "start copy" command), so the frontend can subscribe
   between the two.
2. **Buffer/replay on the backend.** Have `GuiState`/the job registry retain
   the last-known status (and a short backlog of not-yet-acknowledged
   events) per job id, and expose a way for the frontend to fetch current
   status directly (e.g. a `get_extraction_status(job_id)` command) as a
   fallback/reconciliation path independent of the event stream -- useful
   regardless of the race, since Tauri events are inherently best-effort.
3. **Delay the spawn.** Have the command handler wait for some signal that
   the frontend is ready before spawning the copy thread -- more fragile and
   not recommended over (1) or (2).

Whichever direction is chosen, add a regression test that exercises a
same-tick-fast completion deterministically (e.g. a zero-byte or few-byte
file, or a test seam that lets the copy run synchronously before any
`listen` call could possibly have registered) so this can't silently
regress again. Today's Rust and TypeScript test suites do not cover the
IPC event race at all -- `run_extraction_job`'s tests call it directly with
a plain callback (no real Tauri event emission or webview listener timing
involved), and `useExtractFile`'s behavior isn't covered by any frontend
integration test.

### Resolution (2026-07-05)

Fixed entirely on the frontend -- **no backend/Rust change was needed**.
Direction 1 above ("register listeners before starting the job") turned out
to be applicable without splitting `extract_file` into two commands: the
five `listen(...)` calls in `subscribeToExtractionEvents`
(`apps/cryptovol-gui/src/shared/api/commands.ts`) listen on the global
`extract://*` event names, not a per-job channel -- job filtering already
happened client-side by comparing `event.payload.jobId`. That means the
frontend can register all five listeners *before* it knows the job id, then
bind the id once `extract_file`'s response arrives.

`subscribeToExtractionEvents` no longer takes a `jobId` up front; it returns
`{ bindJobId, unsubscribe }`. Any event received before `bindJobId` is called
is buffered (not dropped) and replayed, in arrival order, filtered to the
bound id, the instant `bindJobId` runs.
`apps/cryptovol-gui/src/features/extract-file/model/useExtractFile.ts`'s
`start()` was reordered to `await subscribeToExtractionEvents(...)` first
and `await extractFile(...)` second, calling `subscription.bindJobId(started.jobId)`
right after `extractFile` resolves. Because listener registration now always
completes before `extract_file` is even invoked, no event can be emitted
before a listener exists, regardless of file size or IPC speed -- with no
artificial delay added anywhere. This also means the original "open
question" above (why the existing Tauri E2E spec didn't hit the race) is
moot: the fix makes the outcome deterministic rather than IPC-speed-dependent
either way.

Regression coverage:

- `apps/cryptovol-gui/src/shared/api/commands.test.ts` (new) -- exercises
  `subscribeToExtractionEvents`'s real implementation (mocking only
  `@tauri-apps/api/event`'s `listen`) to reproduce the exact race
  deterministically: an event fired before `bindJobId` is proven buffered,
  then replayed once bound.
- `apps/cryptovol-gui/src/widgets/extraction-panel/ui/ExtractionPanel.integration.test.tsx`
  gained an ordering assertion (`subscribeToExtractionEvents` called before
  `extractFile`) that fails against the pre-fix call order.

See `docs/gui-testing.md` for why `commands.test.ts` is a deliberate, narrow
exception to the frontend/Rust two-layer test split described there.
