# `cryptovol-app`: The Application Core

## Purpose

`cryptovol-app` is the framework-neutral application core shared by `cryptovol-cli` and the `cryptovol-gui` (Tauri) desktop frontend. It owns everything between "here is a container file and a password" and "here are directory entries / extracted bytes": password-free container inspection, TC/VC volume opening, filesystem probing and dispatch across FAT/exFAT/NTFS, directory listing and stat, and progress- and cancellation-aware single-file extraction.

It exists so that container/volume/filesystem logic is written and tested exactly once, instead of being duplicated between the CLI and the GUI. See [architecture.md](architecture.md) for how this crate fits into the overall crate layout, and [gui-mvp.md](gui-mvp.md) for how the GUI's Tauri command layer wraps this crate's API.

## Why This Crate Stays Framework-Neutral

`cryptovol-app` does not depend on `clap`, `rpassword`, `tauri`, or any other CLI/GUI framework, even though both a CLI and a GUI now consume it. Its public API is plain Rust types (`Vec<FileEntry>`, `Result<_, AppError>`, a `FnMut(ProgressEvent)` callback) that either a terminal loop or a GUI toolkit's event loop/reactive model can wrap, rather than API shapes tailored to one framework's async/callback conventions. This also keeps the crate's dependency graph small — no GUI toolkit, no async runtime — so it stays fast to build and test, and so a future change of GUI toolkit would only touch `cryptovol-gui`, not this layer.

## Dependency Direction

```text
cryptovol-cli               cryptovol-gui (Tauri commands)
       \                          /
        \                        /
             cryptovol-app
                  -> cryptovol-tcvc
                  -> cryptovol-fs-fat / cryptovol-fs-exfat / cryptovol-fs-ntfs
                       -> cryptovol-core
```

`cryptovol-app` is the only crate a frontend needs to depend on for container/volume/filesystem behavior. It depends on `cryptovol-core`, `cryptovol-tcvc`, and the three filesystem crates, plus `thiserror`, `secrecy`, and `tempfile`. It does not depend on `clap`, `rpassword`, any GUI toolkit, `serde`, or an async runtime — see `crates/cryptovol-app/Cargo.toml` and the guard test below.

## Public API

### `OpenVolumeRequest`

The input to `open_volume`. Carries `container_path: PathBuf`, `password: secrecy::SecretString`, `pim: Option<u32>` (VeraCrypt PIM; `None` uses the default), and `kdf_hint: Option<TcvcKdf>` (`None` autoprobes all supported KDFs). The password is consumed by `open_volume` and is not retained afterward.

### `open_volume(request: OpenVolumeRequest) -> Result<VolumeSession, AppError>`

Opens a TC/VC volume: opens the container file, tries the requested (or autoprobed) KDF/PIM/header-candidate combinations, and returns an owned `VolumeSession` on success. Returns `AppError::AuthFailed` for a wrong password or unsupported profile parameters, `AppError::UnsupportedFormat` for a recognized-but-unsupported profile, or `AppError::Io`/`AppError::ExtractionFailed`-family variants for I/O failures.

### `VolumeSession`

An opened, owned TC/VC volume. Holds the container's `FileBlockReader` and the matched, opened TC/VC volume state; the password used to open it is never retained. Exposes:

* `volume_info() -> VolumeInfo` — safe, non-secret metadata about the opened volume.
* `list_dir(path: &str) -> Result<Vec<FileEntry>, AppError>` — lists a directory's entries, dispatching to the FAT, exFAT, or NTFS backend based on the first-sector probe result.
* `stat(path: &str) -> Result<FileEntry, AppError>` — metadata for a single file or directory. exFAT and NTFS use their native stat; FAT (which has none) falls back to listing the parent directory and matching by name.
* `extract_file(source_path: &str, destination_path: impl AsRef<Path>, options: ExtractOptions, progress: impl FnMut(ProgressEvent)) -> Result<ExtractSummary, AppError>` — extracts a single file to a host destination path, streaming through the canonical `StreamingWriter`, reporting `ProgressEvent`s, and honoring an optional `CancellationToken`.

### `VolumeInfo`

Safe, non-secret metadata returned by `volume_info()`: `container_path`, `container_size_bytes`, `backend` (currently always `"tcvc"`), `cipher` (currently always `"AES-XTS"`), `kdf`, `pim`, `header_role`, `filesystem` (a `FilesystemKind`, or `Unknown` if the decrypted data area could not be probed), `read_only` (always `true` in this MVP), and `decrypted_data_offset`/`decrypted_data_len: Option<u64>` (the decrypted data region's byte offset/length within the container; `Some` for TC/VC volumes today, with `None` reserved for a future backend that might not expose this concept). Only the numeric offset/length are exposed — never decrypted header bytes, keys, or decrypted content. `cryptovol-cli`'s `probe-fs` command still calls `cryptovol-tcvc` directly rather than `VolumeInfo` for historical reasons (see [architecture.md](architecture.md)); it could be migrated onto `VolumeInfo` now that these fields exist, but that migration is out of scope for this milestone.

### `FileEntry`

A unified, filesystem-agnostic directory/file entry, used by both `list_dir` and `stat`: `name`, `path` (`Some` only when returned by `stat`), `is_dir`, `size: u64`, `attributes: FileAttributes`, `created`/`modified`/`accessed: Option<AppTimestamp>`, and `filesystem: FilesystemKind`. It is built by converting each backend's own entry type (`DirectoryEntry`, `ExfatEntry`, `NtfsEntry`) into this shared shape, so callers never need to match on which filesystem produced an entry.

### `ExtractOptions`

Destination and cancellation policy for `extract_file`: `overwrite: bool`, `parents: bool` (create missing destination parent directories), and `cancellation_token: Option<CancellationToken>`.

### `ProgressEvent`

Reported to the caller's progress callback during `extract_file`: `Started { source_path, destination_path, total_bytes }` (once, with the known total size when available), `Advanced { bytes_written, total_bytes }` (one or more times as chunks are written to the destination), and `Finished { bytes_written }` (once, on success). Progress is based on bytes written to the destination, not bytes read from the source, and events never carry decrypted file content — only paths and byte counts.

### `CancellationToken`

A cheap, `Clone`-able, thread-safe handle (`CancellationToken::new()`, `.cancel()`, `.is_cancelled()`) a caller can hold onto and cancel from another thread or callback. Passed into `ExtractOptions::cancellation_token`; if cancelled before or during `extract_file`'s copy, the call returns `Err(AppError::Cancelled)`, the partial temp file is dropped without ever being renamed into place, and a pre-existing destination is left untouched.

### `AppError`

The single error type returned by every `cryptovol-app` operation. Variants: `Io`, `AuthFailed`, `UnsupportedFormat(String)`, `FilesystemNotRecognized`, `PathNotFound(String)`, `DirectoryExtractionUnsupported(String)`, `UnsupportedFeature(String)`, `ExtractionFailed(String)`, `Cancelled`, `InvalidInput(String)`. It has `From` conversions from `cryptovol_tcvc::TcvcOpenError`, `cryptovol_fs_fat::FatError`, `cryptovol_fs_exfat::ExfatError`, `cryptovol_fs_ntfs::NtfsError`, and `std::io::Error`, so call sites can use `?` without hand-mapping every backend error. Its `Display`/`Debug` output never includes passwords, derived key material, or decrypted file contents.

## How `cryptovol-cli` Uses It Today

`cryptovol-cli` owns argument parsing (`clap`), password prompting (`rpassword`, wrapped into a `secrecy::SecretString` immediately), user-facing `println!`/`eprintln!` rendering, and exit-code mapping. For `info`, `test-open`, `ls`, and `extract`, it calls `cryptovol_app::inspect_container`/`open_volume`/`VolumeSession` methods and renders the returned data or maps `AppError` variants to the documented exit codes; see `crates/cryptovol-cli/src/commands.rs`. `extract` passes a no-op progress closure (`|_event| {}`) and `cancellation_token: None`, since the CLI does not yet expose a `--progress` flag or a way to cancel a running extraction — see [streaming-extraction.md](streaming-extraction.md).

`probe-fs` is the one exception: it still calls `cryptovol-tcvc` directly, because its output needs the decrypted-data offset/length that `VolumeInfo` does not expose (see above).

## How `cryptovol-gui` Uses It

`apps/cryptovol-gui/src-tauri` depends on `cryptovol-app` the same way `cryptovol-cli` does, and nothing lower — see [gui-mvp.md](gui-mvp.md) for the full command/DTO/event model. In summary:

* The password arrives via `OpenContainerRequestDto` (a `serde::Deserialize`-only DTO, never `Debug`/`Clone`) and is wrapped into a `secrecy::SecretString` before calling `open_volume`, matching the CLI's pattern.
* `open_volume` and `extract_file` are called synchronously from a `#[tauri::command]` handler (the former on the command-invocation thread; the latter's actual copy runs on a spawned `std::thread` so the command returns immediately), and the returned `VolumeSession` is held in `GuiState`'s session registry, keyed by an opaque UUID `SessionId`.
* `list_dir`/`stat` results are mapped to `FileEntryDto` and rendered by the React `DirectoryBrowser` widget.
* `extract_file`'s progress closure translates each `ProgressEvent` into a typed `extract://*` Tauri event forwarded to the frontend; a `CancellationToken` is stored in `GuiState`'s job registry so a `cancel_extract` command can cancel the in-flight copy from another thread.
* `AppError` is mapped to a stable-coded `GuiErrorDto` (`code` + short message) via `From<AppError> for GuiErrorDto` in `apps/cryptovol-gui/src-tauri/src/error.rs`, rather than relying on `AppError`'s `Display` text as final UI copy.

## Non-Goals

`cryptovol-app` deliberately does not, and — enforced by `crates/cryptovol-app/tests/no_terminal_io.rs` — must never:

* Prompt for a password itself (no `rpassword` dependency, no stdin reads).
* Print to stdout/stderr (no `println!`/`eprintln!`/`print!`/`eprint!`).
* Render any terminal or GUI widgets, or depend on a CLI/GUI framework (no `clap`, no `tauri`) — the guard test's forbidden-substring list covers both, since this crate is the shared core for both the CLI and the GUI.
* Depend on an async runtime (no `tokio`/`async-std`; `open_volume`/`extract_file` are synchronous, blocking calls).
* Mount volumes, use FUSE, or use kernel extensions.
* Write to, or otherwise modify, the source encrypted container. The only write path is the destination file `extract_file` is explicitly asked to create.
