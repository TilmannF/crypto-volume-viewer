# Crypto Volume Viewer 0.1.0

## Status

First public macOS release. Signed and notarized Developer ID build, distributed via GitHub Releases. The GUI is functional and tested, not independently audited. This is an initial feature-limited release, not a claim of finished TrueCrypt/VeraCrypt compatibility.

Downloads: https://github.com/TilmannF/crypto-volume-viewer/releases/latest

## Supported features

- Opening TrueCrypt/VeraCrypt-compatible file-hosted containers by password, with multi-KDF autoprobe (SHA-512, SHA-256, Whirlpool, BLAKE2s-256, Streebog) and custom PIM support.
- AES-XTS volume decryption.
- Read-only directory listing and metadata for FAT (including long filenames/Unicode), exFAT, and NTFS.
- Single-file extraction, streamed in bounded chunks, with progress and cancellation.
- Both a scriptable CLI (`cryptovol`) and a desktop GUI (Crypto Volume Viewer), sharing the same underlying `cryptovol-app` core.

## Known limitations

- File-hosted TrueCrypt/VeraCrypt-compatible containers only — no partition-hosted or system volumes.
- AES-XTS only — no other cipher/mode combinations.
- No keyfiles.
- No hidden volumes.
- No system/partition volumes.
- No directory extraction — single-file extraction only.
- No write support of any kind to the encrypted container.
- No mounting, no FUSE, no kernel extensions.
- macOS-first: this release is a notarized macOS GUI/CLI. Windows/Linux packaging is not included.
- The GUI is early-release quality: functional and tested, but not independently audited.
- No broad TrueCrypt/VeraCrypt compatibility is claimed; support is validated only against this project's own test fixtures, not arbitrary real-world containers.

## Security and privacy

- Read-only by design: the tool never writes to the encrypted container.
- No telemetry, no analytics, no crash reporting.
- No auto-update mechanism.
- No password logging, persistence, or return to the GUI frontend after a volume is opened.
- See [docs/security.md](docs/security.md) and [docs/privacy.md](docs/privacy.md).

Extracted files are no longer protected by the container encryption.

## macOS installation notes

1. Download `Crypto Volume Viewer_0.1.0_aarch64.dmg` from the GitHub Release.
2. Verify the checksum (below).
3. Open the `.dmg` and drag **Crypto Volume Viewer** into **Applications**.
4. First launch should be accepted by Gatekeeper (Developer ID, notarized). If macOS still blocks it, the download was altered or the checksum was skipped.

## Verifying your download

Every release attaches `SHA256SUMS.txt`. From the directory containing both files:

```bash
shasum -a 256 -c SHA256SUMS.txt
```

A line reading `<filename>: OK` confirms the file matches the published checksum.

## License

Crypto Volume Viewer is licensed under the Apache License, Version 2.0. See [LICENSE](LICENSE) and [docs/license-decision.md](docs/license-decision.md).

## Reporting issues

https://github.com/TilmannF/crypto-volume-viewer/issues
