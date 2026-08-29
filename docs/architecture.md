# Architecture

`cryptovol` is a read-only CLI for inspecting file-hosted encrypted container data without mounting it.

## Current Crate Boundaries

* `cryptovol-cli`: clap command parsing, password prompting, user-facing output, and exit-code mapping. Delegates container/volume/filesystem logic to `cryptovol-app` for `info`, `test-open`, `ls`, and `extract`. `probe-fs` still calls `cryptovol-tcvc` directly, since its output needs the raw decrypted-data offset/length that `cryptovol-app`'s `VolumeInfo` does not yet expose.
* `cryptovol-gui` (`apps/cryptovol-gui/src-tauri`): Tauri 2 desktop GUI backend. Registers `#[tauri::command]` functions that adapt `cryptovol-app` to DTOs/events for a React + MUI frontend (`apps/cryptovol-gui/src`), and owns GUI-only state (open session and extraction job registries). See [gui-mvp.md](gui-mvp.md).
* `cryptovol-app`: framework-neutral application core shared by the CLI and the GUI. Owns password-free container inspection (`inspect_container`), volume opening (`open_volume`/`VolumeSession`), directory listing/stat, and progress- and cancellation-aware single-file extraction (`VolumeSession::extract_file`), including the canonical streaming destination writer. It never prompts for passwords and never prints to stdout/stderr — see [security.md](security.md).
* `cryptovol-core`: shared read-only block abstractions and common file-backed block reader errors.
* `cryptovol-tcvc`: TC/VC-compatible backend for header-candidate inspection, multi-KDF autoprobing (SHA-512, SHA-256, Whirlpool, BLAKE2s-256, Streebog), custom PIM support, AES-XTS decryption, decrypted data reads, and first-sector filesystem probing.
* `cryptovol-fs-fat`: read-only FAT parser for 8.3 directory listing and single-file reads.
* `cryptovol-fs-exfat`: read-only exFAT parser for boot sector parsing, cluster mapping, directory listing, and single-file extraction.
* `cryptovol-fs-ntfs`: read-only NTFS parser for boot sector parsing, MFT record access, directory listing, metadata, and single-file extraction.

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

`cryptovol-app` is the single boundary between any frontend (CLI or GUI) and the TC/VC + filesystem backends. `cryptovol-gui`'s Tauri command layer depends on `cryptovol-app` the same way `cryptovol-cli` does, without needing to know about `cryptovol-tcvc` or the filesystem crates directly.

## Data Flow

```text
CLI command
  -> cryptovol-app::inspect_container / open_volume
  -> FileBlockReader
  -> TC/VC header candidate inspection
     or password-based open:
       -> for each header candidate (primary, backup):
            for each KDF profile in autoprobe order (or hint):
              -> PBKDF2-HMAC key derivation (PIM controls iteration count)
              -> AES-XTS header decrypt + validation
              -> TcvcMatchedOpenedVolume (matched KDF, PIM, header role)
  -> TcvcDataReader decrypted logical block reader
  -> filesystem probing (exFAT OEM name, then NTFS OEM name, then FAT-like heuristics)
  -> FAT, exFAT, or NTFS filesystem reader
  -> listing or single-file extraction:
       ls:      VolumeSession::list_dir -> CLI prints names/metadata
       extract: VolumeSession::extract_file -> cryptovol-app's StreamingWriter (temp file)
                -> atomic rename to destination path on success
```

`info` stops at file metadata and TC/VC header candidate locations. `test-open` validates the narrow supported TC/VC profile. `probe-fs` opens the volume and reads only the first decrypted sector. `ls` and `extract` open the supported profile and dispatch to the FAT, exFAT, or NTFS reader based on the first-sector probe result. For `extract`, file data is streamed in 256 KiB chunks directly to a temp file; the full decrypted file is never held in RAM. See [streaming-extraction.md](streaming-extraction.md).

The architecture keeps encrypted-volume handling separate from filesystem parsing. Filesystem code reads from a `BlockReader` and does not know how TC/VC decryption works. TC/VC code exposes a decrypted read-only `BlockReader` and does not parse directory entries. `cryptovol-app` keeps this dispatch logic out of any specific frontend.

## Current Boundaries

The implementation does not mount containers, use FUSE, use kernel extensions, expose write APIs, or modify source containers. Decrypted file bytes are written only when the user explicitly runs `extract`.
