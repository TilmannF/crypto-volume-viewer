# 40 GUI Tauri Policy

This policy applies to the Tauri desktop GUI layer for `cryptovol`.

It is optimized for AI agents implementing a Tauri + React + TypeScript GUI over the framework-neutral `cryptovol-app` crate.

This policy is subordinate to:

* `00-engineering-policy.md`
* `10-rust-project-structure-policy.md`
* `20-rust-code-policy.md`
* `30-frontend-policy.md`

When policies conflict, the more specific policy wins for its scope.

## 1. Scope

This policy applies to:

```text id="muu2t4"
apps/cryptovol-gui/src-tauri/
apps/cryptovol-gui/src-tauri/src/
apps/cryptovol-gui/src-tauri/capabilities/
apps/cryptovol-gui/src-tauri/tauri.conf.json
```

and any Rust code whose purpose is to expose `cryptovol-app` functionality to the GUI.

This policy does not apply to:

```text id="929sx2"
crates/cryptovol-app/
crates/cryptovol-core/
crates/cryptovol-tcvc/
crates/cryptovol-fs-fat/
crates/cryptovol-fs-exfat/
crates/cryptovol-fs-ntfs/
apps/cryptovol-gui/src/
```

Frontend TypeScript/React code is governed by `30-frontend-policy.md`.

Core Rust library code is governed by Rust policies.

## 2. Purpose

The Tauri layer MUST be a thin desktop bridge between the frontend and `cryptovol-app`.

The Tauri layer MAY handle:

* Tauri command registration
* DTO serialization/deserialization
* session registry
* extraction job registry
* event emission
* cancellation routing
* file dialog integration
* minimal GUI-specific error mapping
* minimal desktop app bootstrapping

The Tauri layer MUST NOT implement:

* TC/VC crypto logic
* KDF/PIM logic
* filesystem parsing
* path resolution inside encrypted volumes
* extraction algorithms
* overwrite/tempfile/symlink policy
* progress calculation beyond forwarding app-core events
* business rules that belong in `cryptovol-app`

## 3. Dependency Direction

Dependency direction MUST remain:

```text id="ernw1p"
React/TypeScript frontend
  -> Tauri commands/events
      -> cryptovol-app
          -> cryptovol-tcvc
          -> cryptovol-fs-fat / cryptovol-fs-exfat / cryptovol-fs-ntfs
          -> cryptovol-core
```

The Tauri layer MAY depend on:

```text id="2zj48v"
cryptovol-app
secrecy
serde
serde_json
tauri
uuid or equivalent small ID generator, if needed
thiserror, if needed locally
```

The Tauri layer MUST NOT make `cryptovol-app` depend on Tauri.

The Tauri layer MUST NOT depend on CLI internals.

The Tauri layer MUST NOT call the `cryptovol` CLI binary.

The Tauri layer MUST NOT shell out to system commands.

## 4. Project Layout

Use this structure unless there is a strong reason not to:

```text id="vgf4xe"
apps/cryptovol-gui/src-tauri/
  Cargo.toml
  tauri.conf.json
  capabilities/
  src/
    main.rs
    commands/
    dto/
    state.rs
    error.rs
    events.rs
```

Suggested meaning:

```text id="8su0iz"
main.rs        Tauri app bootstrap and command registration
commands/      #[tauri::command] functions
dto/           serde-friendly command request/response structs
state.rs       managed GUI state: sessions and extraction jobs
error.rs       GUI error mapping
events.rs      event payloads and event-name constants
```

Do not put all command, state, DTO, and event logic into one large `main.rs`.

A file SHOULD stay below 300 lines.

A file MUST NOT exceed 500 lines without explicit justification.

## 5. Tauri Command Rules

All Tauri commands MUST be thin adapters.

Allowed command responsibilities:

```text id="i6j5l8"
deserialize DTO
validate shallow GUI input
look up session/job state
call cryptovol-app
map result to DTO
emit events
return sanitized error
```

Forbidden command responsibilities:

```text id="y4d1fm"
parse filesystem structures
decrypt data
derive keys
inspect raw decrypted headers
walk FAT/exFAT/NTFS structures directly
stream file data manually instead of using cryptovol-app
implement overwrite/tempfile policy
perform shell execution
log secrets
```

Commands MUST return `Result<T, GuiErrorDto>` or an equivalent typed error model.

Commands MUST NOT return raw backend errors directly.

Commands MUST NOT expose internal binary data.

## 6. Command Surface

The GUI command surface SHOULD remain small.

Expected commands for the MVP:

```rust id="n9tz71"
inspect_container
open_container
list_dir
stat
extract_file
cancel_extract
close_session
```

Optional commands:

```rust id="t51oem"
select_container_file
select_extract_destination
```

if native file dialogs are implemented through Tauri APIs.

Do not add broad generic commands such as:

```text id="j5380s"
read_any_file
write_any_file
run_command
open_shell
debug_dump_volume
debug_dump_header
debug_dump_keys
```

## 7. DTO Rules

Tauri command DTOs MUST be serde-friendly.

DTOs MUST be separate from app-core types unless the app-core type is intentionally serde-compatible and safe to expose.

Use explicit DTOs for frontend/backend boundaries.

Example:

```rust id="pfcpci"
#[derive(serde::Deserialize)]
pub struct OpenContainerRequestDto {
    pub container_path: String,
    pub password: String,
    pub pim: Option<u32>,
    pub kdf_hint: Option<String>,
}
```

Structs containing passwords MUST NOT derive:

```rust id="o61z75"
Debug
Clone
Copy
```

Password DTOs SHOULD derive only what is strictly necessary, typically:

```rust id="0j58py"
Deserialize
```

Response DTOs MUST NOT contain:

* passwords
* derived keys
* header keys
* master keys
* decrypted header bytes
* decrypted file contents
* raw binary dumps

## 8. Password and Secret Handling

Password handling MUST follow these rules:

1. The frontend sends the password only to `open_container`.
2. The Tauri command receives the password.
3. The Tauri command converts it promptly into the secret type expected by `cryptovol-app`.
4. The Tauri command passes it to `cryptovol-app::open_volume`.
5. The password MUST NOT be stored in Tauri state.
6. The password MUST NOT be returned to the frontend.
7. The password MUST NOT be logged.
8. The password MUST NOT appear in errors.
9. The password MUST NOT appear in progress events.

The Tauri state MUST store:

```text id="5875mx"
VolumeSession
safe session metadata
CancellationToken
job metadata
```

The Tauri state MUST NOT store:

```text id="yxts3y"
passwords
derived keys outside cryptovol-app/tcvc internals
raw decrypted headers
raw decrypted file data
```

Do not add “remember password” features.

Do not use OS keychain APIs in the MVP.

Do not persist secrets to disk.

## 9. Session Registry

The Tauri layer SHOULD manage opened volumes through an opaque session registry.

Suggested shape:

```rust id="xmqc3l"
struct GuiState {
    sessions: Mutex<HashMap<SessionId, VolumeSession>>,
    extraction_jobs: Mutex<HashMap<JobId, CancellationToken>>,
}
```

Exact types may differ.

Session IDs MUST be opaque.

Session IDs MUST NOT be derived from:

* file path
* password
* KDF
* PIM
* filesystem path
* timestamp alone

Use a random or UUID-like identifier.

The registry MUST support closing sessions.

Closing a session SHOULD remove its `VolumeSession`.

If an extraction job is active for a session, closing behavior MUST be explicit:

* reject close while jobs are active, or
* cancel active jobs, then close

Document the chosen behavior.

## 10. Threading and Jobs

Long-running extraction MUST NOT block the UI.

Extraction SHOULD run in a blocking worker or Tauri-supported background task.

Rules:

* create one `CancellationToken` per extraction job
* store the token in the job registry
* call `VolumeSession::extract_file`
* forward progress events to the frontend
* remove job state after completion, failure, or cancellation
* avoid unbounded thread creation
* avoid global mutable state outside Tauri-managed state

Do not add an async runtime unless Tauri already requires one and the choice is documented.

Do not make `cryptovol-app` spawn GUI-specific threads.

## 11. Progress Events

Progress events MUST be emitted through a small, typed event model.

Use event-name constants.

Example event names:

```text id="9345e1"
extract://started
extract://progress
extract://finished
extract://cancelled
extract://failed
```

or equivalent.

Progress payloads MAY contain:

```text id="i63k5r"
job_id
session_id
source_path
destination_path
bytes_written
total_bytes
error code
safe error message
```

Progress payloads MUST NOT contain:

* passwords
* keys
* decrypted file contents
* decrypted header bytes
* raw binary dumps

Event emission MUST tolerate frontend listeners being absent.

Event failures MUST NOT corrupt extraction state.

## 12. Cancellation

Cancellation MUST be real backend cancellation.

The `cancel_extract` command MUST route to the stored `CancellationToken`.

The frontend MUST NOT merely hide progress UI and pretend cancellation happened.

Cancellation behavior MUST preserve `cryptovol-app` guarantees:

* return cancelled error/result
* cleanup partial output where practical
* do not leave a final renamed destination for cancelled extraction unless documented and unavoidable

Cancellation command behavior:

```text id="s1etdh"
known job id      -> cancel token, return success
unknown job id    -> return typed not-found or already-finished error
already finished  -> return success or typed already-finished error
```

Choose one behavior and document it.

## 13. File Dialogs and Filesystem Access

Native file dialogs MAY be used.

Allowed dialogs:

```text id="dc54k1"
select encrypted container file
select extraction destination file
select extraction destination directory, only if later directory extraction exists
```

Do not request broad filesystem permissions beyond what the MVP needs.

Do not add unrestricted read/write APIs exposed to the frontend.

Do not implement a generic frontend file browser for the host filesystem.

Do not auto-open extracted files.

Do not reveal arbitrary host files to the frontend.

All host filesystem writes MUST go through `cryptovol-app` extraction policy.

## 14. Tauri Permissions and Capabilities

Tauri permissions MUST be minimal.

Capabilities MUST allow only what the GUI uses.

Do not enable shell permissions.

Do not enable broad filesystem permissions.

Do not enable network permissions unless explicitly required and reviewed.

Do not enable auto-updater permissions in the MVP.

Do not enable clipboard permissions unless used and justified.

If a permission is added, the final report MUST explain why.

The default security posture is deny-by-default.

## 15. Webview and Frontend Boundary

The webview MUST NOT receive backend internals.

The webview MAY receive:

* session id
* safe volume info
* file entries
* progress events
* sanitized errors

The webview MUST NOT receive:

* passwords after command submission
* decrypted file bytes
* cryptographic keys
* raw header bytes
* raw MFT/FAT/exFAT structures
* backend debug dumps

Do not expose `window.__CRYPTOVOL_DEBUG__` or similar debug globals.

Do not put secrets in query parameters, fragment identifiers, document title, or DOM attributes.

## 16. Error Mapping

Create a GUI error DTO.

Example:

```rust id="77pvf9"
#[derive(Debug, Clone, serde::Serialize)]
pub struct GuiErrorDto {
    pub code: String,
    pub message: String,
}
```

Error codes SHOULD be stable strings.

Suggested codes:

```text id="4h02wx"
auth_failed
unsupported_format
filesystem_not_recognized
path_not_found
directory_extraction_unsupported
unsupported_feature
cancelled
io_error
invalid_input
session_not_found
job_not_found
internal_error
```

Error messages MUST be user-facing and sanitized.

Do not expose:

* raw `Debug` output from backend errors
* stack traces
* secrets
* raw binary data

Backend errors SHOULD map through `AppError`.

Tauri commands SHOULD not need to know individual FAT/exFAT/NTFS error details.

## 17. Logging

Logging in the Tauri layer MUST be minimal.

Do not log:

* passwords
* PIM combined with password context
* derived keys
* decrypted headers
* decrypted file contents
* raw binary buffers
* full backend debug dumps

Allowed logs:

```text id="gwh4kw"
app startup
command name without payload
safe error code
safe high-level state transition
```

Avoid logging full file paths unless needed for debugging and approved.

Do not add telemetry.

Do not add analytics.

Do not add crash reporting.

## 18. Configuration

Do not add persistent settings in the MVP unless explicitly required.

Do not persist:

* passwords
* last opened container path
* extraction destinations
* session history
* recent files

Recent files may be a future feature, but not in the first GUI spike.

Do not add auto-update configuration in the MVP.

## 19. Build and Packaging

The GUI MVP MUST build in development mode.

Packaging/signing is not required in this milestone.

Do not add installer/signing/notarization complexity unless explicitly requested.

Document:

```text id="v0w4yz"
npm install
npm run build
npm run tauri dev
npm run tauri build
```

or the actual commands used by the project.

If `tauri build` fails because local platform packaging prerequisites are missing, report that honestly and ensure the frontend build and Rust checks pass.

## 20. Testing

Add tests where practical.

Rust-side tests SHOULD cover:

* DTO parsing/mapping
* KDF hint parsing
* PIM validation
* error mapping
* session registry insert/get/remove
* session id uniqueness
* job registry insert/cancel/remove
* progress DTO construction
* secret DTOs do not derive `Debug`
* command helpers do not store passwords

Avoid brittle UI automation in the first GUI milestone.

Do not require a real GUI window for normal workspace tests.

Normal Rust tests MUST remain runnable with:

```bash id="6kmap2"
cargo test --workspace --all-targets
```

Frontend tests and build checks are governed by `30-frontend-policy.md`.

## 21. Interaction With `cryptovol-app`

The Tauri layer MUST prefer app-core APIs.

Allowed:

```rust id="c6a28k"
cryptovol_app::inspect_container
cryptovol_app::open_volume
VolumeSession::volume_info
VolumeSession::list_dir
VolumeSession::stat
VolumeSession::extract_file
CancellationToken
```

Discouraged:

```rust id="rn3nzz"
direct cryptovol-tcvc calls
direct cryptovol-fs-fat calls
direct cryptovol-fs-exfat calls
direct cryptovol-fs-ntfs calls
manual FileBlockReader creation in Tauri commands
manual filesystem probing in Tauri commands
```

Direct lower-layer calls require explicit justification.

## 22. No Business Logic in Tauri Commands

Tauri commands MUST NOT become the new application core.

If a command needs substantial logic, move that logic into `cryptovol-app`.

Heuristic:

```text id="t6ib2n"
If command code exceeds ~80 lines, look for extraction into app-core or helper module.
If command needs filesystem-specific branching, it probably belongs in cryptovol-app.
If command needs crypto-specific branching, it probably belongs in cryptovol-app or cryptovol-tcvc.
```

The command layer should remain boring.

## 23. Security Review Checklist

Before final report, verify:

* no `unsafe`
* no shell plugin / shell command execution
* no password persistence
* no password logging
* no raw backend debug dumps in errors
* no broad filesystem permissions
* no network permissions
* no telemetry
* no updater
* no GUI dependency in `cryptovol-app`
* no CLI dependency in Tauri code
* no frontend direct access to backend internals
* extraction still goes through `cryptovol-app`
* cancellation is real
* progress events contain no secrets

## 24. Documentation

When adding or changing Tauri code, update documentation.

Relevant docs:

```text id="h3slw7"
docs/gui-mvp.md
docs/app-core.md
docs/architecture.md
docs/security.md
README.md
```

Document:

* Tauri command API
* DTO model
* session registry
* extraction job registry
* progress events
* cancellation behavior
* permissions/capabilities used
* development commands
* known limitations

## 25. Final Report Requirements

When a task changes Tauri code, the final report MUST include:

* Tauri files changed
* commands added/changed
* DTOs added/changed
* session registry behavior
* extraction job behavior
* progress event names
* cancellation behavior
* permissions/capabilities added
* how passwords are handled
* confirmation that passwords are not stored
* confirmation that `cryptovol-app` remains Tauri-free
* Rust checks run
* GUI checks run
* manual smoke tests run
* intentionally deferred GUI/Tauri work
