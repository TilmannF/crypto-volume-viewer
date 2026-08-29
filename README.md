# Crypto Volume Viewer

`cryptovol` is intended to become a cross-platform, read-only command-line tool for inspecting encrypted volume/container formats without mounting them.

This repository is experimental. It currently has a Rust workspace, a read-only file-backed block reader, basic file metadata output, TC/VC-style header candidate inspection for file-hosted containers, a TC/VC header-open path with multi-KDF autoprobing and custom PIM support, FAT directory listing with Long File Name (LFN) and Unicode support, FAT metadata, exFAT directory listing and single-file extraction, and NTFS read-only listing and single-file extraction.

`cryptovol ls` can list directories with long names (including names with spaces, Unicode, and emoji) on FAT, exFAT, and NTFS volumes. `cryptovol ls --long` additionally shows file attributes, size, and modification timestamp. `cryptovol extract` can extract single files using long-name source paths from FAT, exFAT, or NTFS volumes. Supported KDF/hash profiles: SHA-512, SHA-256, Whirlpool, BLAKE2s-256, Streebog. Custom PIM values are supported via `--pim N`. Passwords are always entered interactively; there is no `--password` flag. No broad TrueCrypt or VeraCrypt compatibility is claimed. No mounting, FUSE, or kernel extensions are used. FAT, exFAT, and NTFS support is read-only. Directory extraction is not supported.

Container inspection, volume opening, directory listing, and extraction live in a framework-neutral `cryptovol-app` crate shared by the CLI and a desktop GUI spike; `cryptovol-cli` itself only owns argument parsing, password prompting, and terminal output. See [docs/architecture.md](docs/architecture.md).

A Tauri + React + MUI desktop GUI lives at `apps/cryptovol-gui`, built on the same `cryptovol-app` core and the same read-only, no-mount, no-FUSE security model as the CLI. See [docs/gui-mvp.md](docs/gui-mvp.md).

## Download (macOS)

Signed, notarized macOS builds: **[GitHub Releases](https://github.com/TilmannF/crypto-volume-viewer/releases/latest)**.

Verify the `.dmg` against the attached `SHA256SUMS.txt` (`shasum -a 256 -c SHA256SUMS.txt`). See [RELEASE_NOTES.md](RELEASE_NOTES.md) for what this release contains.

Issues: [github.com/TilmannF/crypto-volume-viewer/issues](https://github.com/TilmannF/crypto-volume-viewer/issues). Privacy: [docs/privacy.md](docs/privacy.md).

## Status

`cryptovol` (public product name: **Crypto Volume Viewer**) is at version `0.1.0`. Initial public macOS release. Feature-limited; not independently audited.

* **Read-only.** The tool never writes to, mounts, or modifies a source encrypted container. See [docs/security.md](docs/security.md).
* **The GUI (`apps/cryptovol-gui`) exists and works, but is early-release quality** — functional and tested, not yet polished as a finished public product. See [docs/gui-mvp.md](docs/gui-mvp.md).
* **Supported containers/filesystems:** TC/VC-compatible file-hosted containers (AES-XTS) over FAT (with long filenames/Unicode), exFAT, and NTFS. See [docs/format-support.md](docs/format-support.md) for the exact support matrix.
* **Explicitly unsupported:** directory extraction, keyfiles, hidden volumes, mounting, FUSE, kernel extensions, and any write support.
* **License: Apache-2.0.** See [LICENSE](LICENSE) and [docs/license-decision.md](docs/license-decision.md).
* **How to run locally:**
  ```bash
  cargo build                     # CLI (cryptovol)
  cd apps/cryptovol-gui && npm run tauri dev   # desktop GUI
  ```
* See [docs/beta-readiness.md](docs/beta-readiness.md) for the full picture: what's ready, what isn't, and known risks.

This project does not claim to be audited, does not claim broad TrueCrypt/VeraCrypt compatibility, and does not claim feature completeness.

## macOS Packaging

Local unsigned build: `./scripts/package-macos-local.sh`. Signed+notarized release build: `./scripts/package-macos-release.sh`. Publish the DMG to GitHub Releases: `./scripts/publish-github-release.sh`. See [docs/packaging-macos.md](docs/packaging-macos.md) and [docs/release-checklist.md](docs/release-checklist.md).

## Current Commands

```bash
cryptovol info <container>
cryptovol test-open <container> [--pim N] [--kdf <name>]
cryptovol probe-fs <container> [--pim N] [--kdf <name>]
cryptovol ls <container> <path> [--pim N] [--kdf <name>]
cryptovol extract <container> <source-path> <destination-path> [--pim N] [--kdf <name>]
```

Examples with custom PIM:

```bash
cryptovol test-open backup.hc --pim 500
cryptovol ls backup.hc / --pim 500
cryptovol extract backup.hc "/Project Notes Final.txt" ./notes.txt --pim 500
```

Current behavior:

* `cryptovol info <container>` opens the supplied file read-only and prints real file metadata, as well as TC/VC-style candidate header locations.
* `cryptovol test-open <container>` prompts securely for a password (without echoing) and verifies whether the `tcvc-aes-sha512-basic` profile header can be successfully opened.
* `cryptovol probe-fs <container>` prompts securely for a password, opens the volume, reads the first decrypted sector, and reports a conservative filesystem candidate (FAT-like, exFAT, or NTFS) using the decrypted block reader. See [tcvc-data-reader.md](docs/tcvc-data-reader.md) for more details.
* `cryptovol ls <container> <path>` prompts for password, detects the filesystem (FAT, exFAT, or NTFS), and lists the directory at the given path. Long names with spaces, Unicode, and emoji are supported. Authentication failure, path-not-found, and not-a-directory cases return clean non-zero exits.
* `cryptovol ls <container> <path> --long` displays a long listing with type character, file size, modification date/time, and name.
* `cryptovol extract <container> <source-path> <destination-path>` extracts a single file from the decrypted FAT, exFAT, or NTFS filesystem to a host path. Accepts long-name source paths (names with spaces, Unicode, emoji). Supports `--overwrite` to replace an existing destination file and `--parents` to create missing parent directories. Refuses to extract directories. Extraction is streamed in 256 KiB chunks — the full decrypted file is never buffered in RAM. File data is written to a temp file in the destination directory and atomically renamed on success; see [docs/streaming-extraction.md](docs/streaming-extraction.md).

Example with a supported filesystem container:

```bash
cryptovol ls backup.hc /
cryptovol ls backup.hc "/" --long
cryptovol extract backup.hc "/Folder With Spaces/report.pdf" ./report.pdf
```

## Intentionally Unsupported

The current implementation does not include keyfiles, hidden volumes, directory extraction, write support, mounting, FUSE, brute force, wordlists, password recovery, raw decrypted block dumping, or any broad compatibility claim. No general VeraCrypt compatibility beyond the tested fixture profiles is claimed.

`cryptovol test-open` does not accept passwords through command-line flags, password files, or test-only environment variables.

Generated encrypted test containers should stay out of Git by default. Use local ignored paths such as `testdata/generated/` or `.examples/`, and do not commit `*.hc` or `*.tc` files unless a future task explicitly documents and approves an intentional fixture.

## Fixture Checks

Run normal checks without VeraCrypt:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features
cargo test
```

Run generated VeraCrypt fixture checks separately:

```bash
./scripts/test-with-veracrypt-fixtures.sh
```

Run the full KDF/PIM crypto-matrix check (requires VeraCrypt CLI):

```bash
./scripts/test-with-tcvc-crypto-matrix.sh
```

Run static full-pipeline fixture tests (requires committed fixture files):

```bash
CRYPTOVOL_STATIC_CRYPTO_MATRIX_DIR=$(pwd)/testdata/static/crypto-matrix \
  cargo test -- --ignored
```

Generated containers remain ignored by Git.

## Development Checks

```bash
cargo fmt --check
cargo clippy --all-targets --all-features
cargo test
```

## GUI Checks

```bash
cd apps/cryptovol-gui
npm install
npm run typecheck
npm run build
npm test              # frontend integration tests (Vitest)
npm run test:e2e       # persisted local Tauri E2E suite (WebdriverIO)
```

See [docs/gui-testing.md](docs/gui-testing.md) for the full GUI test strategy, fixture requirements, and how to debug a failing E2E spec.

## License

`cryptovol` / Crypto Volume Viewer is licensed under the Apache License, Version 2.0. Copyright 2026 Tilmann Felgner. See [LICENSE](LICENSE) for the full text and [docs/license-decision.md](docs/license-decision.md) for how this decision was made.
