# Security

`cryptovol` is for legitimate access to encrypted containers where the user already has the password. It is not a cracking, bypass, or recovery tool.

## Read-Only Design

The source container is opened through read-only file access. The current architecture exposes decrypted contents through read-only block and filesystem readers. There is no container write path, no in-place modification, and no filesystem mutation support.

## No Mounting

The tool does not mount volumes, install kernel extensions, or depend on FUSE. All supported operations run in userspace and return command output or explicitly extracted files.

## Passwords And Secrets

Commands that open a volume prompt for a password without echoing it. Passwords are not accepted through command-line flags. Derived keys and decrypted headers are not printed. Secret buffers in the TC/VC path are zeroized where the current implementation owns them; this includes header key buffers derived during each KDF autoprobe candidate, which are zeroized after each trial regardless of success or failure.

Password prompting itself lives entirely in `cryptovol-cli` and, for the desktop GUI, in `apps/cryptovol-gui`'s password `TextField`. `cryptovol-app`, the shared application core, never prompts for input and never prints to stdout/stderr — it only accepts an already-collected `secrecy::SecretString` through `OpenVolumeRequest`. That password is consumed by `open_volume` to derive the TC/VC key material and is not stored on the returned `VolumeSession`; operations that need a decrypted reader construct one from the session's owned state rather than retaining the original password. In the GUI, the password never derives a session/job id, is cleared from local UI state immediately after a successful open, and the DTO carrying it (`OpenContainerRequestDto`) has no `Debug`/`Clone` derive so it cannot be accidentally logged; see [gui-mvp.md](gui-mvp.md) for the full GUI security model.

PIM values are not logged, not cached, and not shown in error messages. The `--kdf` hint is a performance option and is not a secret.

Error messages intentionally avoid revealing exact cryptographic trial details. Authentication failure and unsupported parameters may share user-facing text.

## No Telemetry, Analytics, Or Network Activity

`cryptovol` (CLI and GUI) does not include telemetry, usage analytics, crash reporting, or auto-update checks, and makes no network calls of its own. All operations are local: the tool reads the container file and, for `extract`, writes to the destination path the user names — nothing else leaves the machine. There is no background process and no update-check mechanism.

## Temporary Files

The tool does not automatically write decrypted previews. `extract` writes only when the user explicitly supplies a destination path. During extraction, a temporary file is created in the same directory as the destination via the `tempfile` crate, using the canonical `StreamingWriter` owned by `cryptovol-app` (see [streaming-extraction.md](streaming-extraction.md) for the full temp-file-then-atomic-rename mechanics). Decrypted bytes are never fully held in RAM — they stream through a bounded buffer directly to the temp file. On success the temp file is atomically renamed to the final path, so partial decrypted data never appears at the destination. On failure — including cancellation, see below — the temp file is deleted automatically. Generated test containers belong under ignored local paths unless explicitly documented as committed fixtures.

`cryptovol-app` reports extraction progress through `ProgressEvent::{Started, Advanced, Finished}`, keyed on bytes written to the destination. These events carry only byte counts and paths — never decrypted file content — so a caller (the CLI or the GUI) can render a progress bar without exposing plaintext data through its own logging or event handling. Extraction accepts an optional `CancellationToken`; if cancelled before or during a copy, the operation returns `AppError::Cancelled`, the partial temp file is dropped without ever being renamed into place, and any pre-existing destination file is left untouched. The GUI forwards these as typed `extract://*` Tauri events with the same no-secrets guarantee — see [gui-mvp.md](gui-mvp.md).

`cryptovol-app` never modifies the source encrypted container. All operations (`inspect_container`, `open_volume`, `list_dir`, `stat`, `extract_file`) are read-only with respect to the container; the only write path is the destination file the caller explicitly names for `extract_file`.

## Extraction Risks

Extracted files are no longer protected by the source container encryption. Users are responsible for destination location, permissions, backup behavior, and later deletion. Extraction refuses to overwrite normal files unless `--overwrite` is supplied, can create missing parents only with `--parents`, and rejects symlink destinations.

## Filesystem Parsers

The currently supported read-only filesystem parsers are FAT, exFAT, and NTFS. All filesystem
structures are treated as untrusted binary input. Parser failures must return structured errors, not
panic, and unsupported features must fail closed rather than silently guessing.

## Hidden Volumes

Hidden volumes are not supported. The tool does not attempt hidden-volume detection heuristics and must not violate plausible deniability assumptions.

## Unsupported Workflows

The project does not include brute force, wordlists, password recovery, credential harvesting, authentication bypass, exploit logic, write support, mounting, or system-volume decryption.

## Known Unsupported TC/VC Features

The following TC/VC-compatible features are not implemented and must not be assumed to work: keyfiles (additional key material beyond the password), hidden volumes (see above), Argon2id as a KDF (a different memory/time-cost KDF, allowed by policy for a possible future milestone but not implemented today — see [tcvc-kdf-pim-compatibility.md](tcvc-kdf-pim-compatibility.md)), and partition-hosted or system volumes (only file-hosted normal containers are supported — see [format-support.md](format-support.md)).

## Scope Of Testing And Guarantees

No independent security audit has been performed on this project. The guarantees described in this document reflect the current design and what is covered by this project's own test suite (see [gui-testing.md](gui-testing.md)), not an external review. No cryptographic compatibility is claimed beyond the specific cipher/KDF/filesystem combinations documented in [format-support.md](format-support.md) — this project does not claim broad TrueCrypt or VeraCrypt compatibility. Filesystem and container parsers treat all input as untrusted and are designed to fail closed (structured errors, no panics — see "Filesystem Parsers" above), but this is a design goal backed by the committed test fixtures, not an exhaustively fuzz-tested guarantee against every possible malformed input.

## FAT, exFAT, and NTFS Filenames and Unicode

FAT LFN, exFAT, and NTFS directory entries are treated as untrusted binary input throughout the
parsing pipeline.

* **Malformed LFN sequences are discarded.** When an LFN checksum mismatches the computed 8.3
  short-name checksum, or when LFN entries are orphaned or incomplete, the parser falls back to the
  8.3 short name. No panic occurs.
* **Invalid UTF-16 is replaced, never panics.** If a LFN entry contains an unpaired surrogate or
  other invalid UTF-16 sequence, the affected position is substituted with `U+FFFD REPLACEMENT
  CHARACTER`. The decoder never panics on malformed input.
* **Decoded Unicode names are for display and path matching only.** The decoded long name is used
  for listing output and for resolving paths supplied by the user. It is not passed directly to any
  host filesystem operation. If a decoded name is used in a host path (e.g. as a destination
  filename), it must be separately sanitized by the caller.
* **No Unicode normalization is applied.** On-disk UTF-16LE sequences are decoded verbatim for
  FAT LFN, exFAT, and NTFS names. The FAT policy is documented in
  [fat-lfn-unicode-metadata.md](fat-lfn-unicode-metadata.md), and the NTFS policy is documented in
  [ntfs-readonly.md](ntfs-readonly.md). Callers should not assume NFC equivalence when matching
  names across different sources (e.g. terminal input vs. on-disk form).

## Responsible Disclosure

No public responsible-disclosure contact is configured yet. Until one exists, security-sensitive findings should be handled privately with the repository owner.
