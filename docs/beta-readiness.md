# Beta Readiness

This document is the single place a technical user (or a future contributor) can check what `cryptovol` / Crypto Volume Viewer actually supports today, what it does not, what risks remain, and what would have to happen before any public beta distribution. It reflects the state of the `hardening/beta-readiness` branch and is updated as that state changes.

## What is beta-ready today

- Opening TC/VC-compatible file-hosted containers by password, with multi-KDF autoprobe (SHA-512, SHA-256, Whirlpool, BLAKE2s-256, Streebog) and custom PIM support, over both the CLI and the GUI (shared `cryptovol-app` core — see [architecture.md](architecture.md)).
- Read-only directory listing and metadata for FAT (with long filenames/Unicode), exFAT, and NTFS.
- Single-file extraction, streamed in bounded chunks, with progress and cancellation, using temp-file-then-atomic-rename so a destination is never left partially written (see [streaming-extraction.md](streaming-extraction.md)).
- A dense desktop GUI (Tauri 2 + React + MUI) covering the same open/browse/extract flow, audited visually (see [gui-mvp.md](gui-mvp.md)'s "Beta Visual Audit" section) with a documented security model (see [security.md](security.md)).
- A three-layer, version-controlled test suite (Rust, frontend integration, Tauri E2E — see [gui-testing.md](gui-testing.md)) confirmed passing end to end as of this milestone's final regression pass (2026-07-05): `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, and `cargo doc --workspace --no-deps` all clean; `cargo test --workspace --all-targets` (337 passed, 88 ignored/fixture-gated); the fixture-gated FAT/exFAT/NTFS LFN tests in `crates/cryptovol-app/tests/` (7/7 passed, run explicitly with the static fixture env vars — see [test-containers.md](test-containers.md)); both `scripts/test-with-veracrypt-fixtures.sh` (generates and verifies a real FAT VeraCrypt container end to end; 0 failures across the full ignored-test run it triggers) and `scripts/test-with-tcvc-crypto-matrix.sh` (RESULT: PASSED — 8 passed, 2 skipped since this VeraCrypt build does not support BLAKE2s-256, a known limitation, 0 failed) against a real local VeraCrypt CLI install; GUI `npm run typecheck`, `npm run build`, and `npm test` (30/30); and GUI `npm run test:e2e` (8/8, headless by default — see `gui-testing.md` caveat 10 for the mechanism, and the occasional intermittent flake risk under real system load still being tracked there).
- Re-confirmed via `./scripts/check-local-release-candidate.sh` at the end of the macOS packaging foundation milestone (2026-07-06, on this macOS/arm64 machine): `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, and `cargo doc --workspace --no-deps` all clean; `cargo test --workspace --all-targets` (337 passed, 0 failed, 88 ignored/fixture-gated — same counts as the prior pass, no regression); GUI `npm run typecheck`, `npm run build`, `npm test` (35 passed, up from 30 as frontend tests were added since); and `npm run test:e2e` (8/8 across 2 spec files, ~1m18s). VeraCrypt-fixture scripts were not re-run for this milestone (not required — see "Local checks that define a beta candidate" below).

## What is explicitly not supported

Per [format-support.md](format-support.md) and `AGENTS.md`'s non-goals: keyfiles, hidden volumes, partition/system volumes, non-AES-XTS ciphers, Argon2id (allowed by policy for a future milestone, not implemented — see `docs/tcvc-kdf-pim-compatibility.md`), directory extraction, write support, mounting, FUSE, kernel extensions, password recovery/cracking/brute-force, telemetry, analytics, crash reporting, and auto-update.

## Known risks

- **The GUI extraction-UI event race is fixed** (2026-07-05) — see [known-issues.md](known-issues.md). The underlying copy was always correct.
- **No broad TrueCrypt/VeraCrypt compatibility is claimed.** Support is intentionally narrow (see [format-support.md](format-support.md)) and validated only against this project's own committed/generated test fixtures, not real-world third-party containers.
- **No security audit has been performed.** The security model in [security.md](security.md) describes the design and what has been tested, not an independent review.

## Public distribution

macOS GitHub Releases is the public download channel (see [packaging-macos.md](packaging-macos.md)). Mac App Store submission is a separate later channel (sandbox, numeric `CFBundleShortVersionString` already `0.1.0`, privacy/support URLs on GitHub).

Before each GitHub Release: `./scripts/package-macos-release.sh` must pass `codesign` / `spctl` / `stapler` on both `.app` and `.dmg`, then `./scripts/publish-github-release.sh`. Re-run [release-checklist.md](release-checklist.md) immediately before the cut. No broad TrueCrypt/VeraCrypt compatibility is claimed.

## Status table

| Area | Status |
|---|---|
| Core read-only open/list/extract | beta-ready |
| CLI | beta-ready |
| GUI | beta-ready |
| FAT/exFAT/NTFS | beta-ready |
| Streaming extraction | beta-ready |
| Progress/cancellation | beta-ready |
| Test suite | beta-ready |
| License | beta-ready |
| Packaging | ready |
| Signing/notarization | ready |
| GitHub Releases | ready |
| Mac App Store | deferred |
| CI | deferred |
| Documentation | beta-ready |
| Security/privacy statement | beta-ready |

Status values used: `ready`, `beta-ready`, `prepared`, `needs decision`, `blocked`, `deferred`, `not supported`. `prepared` means implemented and working locally, but not yet verified in the one condition that matters most (here: signing/notarization verified end-to-end with real Apple Developer credentials). CI remains intentionally deferred to a later milestone rather than ruled out; this project describes itself as experimental throughout.

"Beta-ready" here means: functional, tested, and documented, but not audited, not packaged/signed, and not proven against real-world third-party containers beyond this project's own fixtures.

## License, packaging, and CI status

- **License:** Apache-2.0, resolved — see [license-decision.md](license-decision.md).
- **Packaging/signing:** Developer-ID-signed and notarized macOS `.dmg` via `scripts/package-macos-release.sh` (app and DMG both stapled). See [packaging-macos.md](packaging-macos.md).
- **Public download:** GitHub Releases — https://github.com/TilmannF/crypto-volume-viewer/releases — via `scripts/publish-github-release.sh`.
- **CI:** no GitHub Actions build/notarize workflow. Signing stays on the local Mac. The test suites remain runnable without a forge.

## Local checks that define a beta candidate

See [release-checklist.md](release-checklist.md) for the full local acceptance process (exact Rust/GUI commands, fixture-gated tests, manual smoke tests). At a minimum: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo test --workspace --all-targets`, and from `apps/cryptovol-gui`: `npm run typecheck`, `npm run build`, `npm test`, `npm run test:e2e` must all pass.
