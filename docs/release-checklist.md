# Release Checklist

A local checklist for cutting a release candidate. Signing and notarization run on the developer Mac. Publishing the notarized `.dmg` uses GitHub Releases (`scripts/publish-github-release.sh`). There is no CI job that builds or notarizes. See [gui-testing.md](gui-testing.md) for why the automated suites stay forge-agnostic.

Run every command below from the repository root unless noted otherwise.

## 1. Version Bump Checklist

Public versions use `x.y.z` only (currently `0.1.0`) so the same string is a valid `CFBundleShortVersionString` for a later Mac App Store build. `apps/cryptovol-gui/src-tauri/tauri.conf.json` also has `bundle.macOS.bundleVersion` (App Store build number; increment on every store upload, even when `x.y.z` stays the same). When cutting a new release, bump the crate/package version consistently across every one of these locations — none of them is workspace-inherited (`version.workspace = true`), so each needs its own edit:

- `crates/cryptovol-app/Cargo.toml`
- `crates/cryptovol-cli/Cargo.toml`
- `crates/cryptovol-core/Cargo.toml`
- `crates/cryptovol-fs-exfat/Cargo.toml`
- `crates/cryptovol-fs-fat/Cargo.toml`
- `crates/cryptovol-fs-ntfs/Cargo.toml`
- `crates/cryptovol-tcvc/Cargo.toml`
- `apps/cryptovol-gui/src-tauri/Cargo.toml`
- `apps/cryptovol-gui/package.json`
- `apps/cryptovol-gui/src-tauri/tauri.conf.json`

Do not touch third-party dependency version fields (e.g. `tauri = "2"`) — only this project's own crate/package versions. After bumping, run `cargo build --workspace` once to regenerate `Cargo.lock` and confirm it still resolves cleanly.

## 2. Rust Checks

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets
cargo doc --workspace --no-deps
```

All four must pass with no warnings/errors before proceeding.

## 3. GUI Checks

Run from `apps/cryptovol-gui/`:

```bash
npm run typecheck
npm run build
npm test
npm run test:e2e
```

`npm run test:e2e` builds and runs the app with its window hidden by default (safe to run in the background without stealing foreground focus or disturbing concurrent use of the machine) and should complete in roughly the same ~51-53s ballpark whether run headless or headed — see [gui-testing.md](gui-testing.md) caveat 10 for the full mechanism, the confirmed before/after numbers, and why a run that's noticeably slower or fails specifically on the wrong-password spec is worth a re-run before assuming it's unrelated flakiness. Use `npm run test:e2e:headed` instead if you want to watch the suite run (e.g. debugging a flaky visual issue).

## 4. Fixture-Gated Checks

Some Rust tests are `#[ignore]`-gated behind environment variables pointing at the committed static fixtures under `testdata/static/` (see [test-containers.md](test-containers.md) for exact fixture contents/hashes). A routine `cargo test --workspace` reports these as "ignored," not failing — run them explicitly as part of a release candidate, since these are the tests that catch cross-filesystem regressions a mocked test cannot:

```bash
CRYPTOVOL_STATIC_FAT_LFN_FIXTURE=$(pwd)/testdata/static/tcvc-aes-sha512-fat-lfn-unicode.hc \
CRYPTOVOL_STATIC_EXFAT_LFN_FIXTURE=$(pwd)/testdata/static/tcvc-aes-sha512-exfat-lfn-unicode.hc \
CRYPTOVOL_STATIC_NTFS_LFN_FIXTURE=$(pwd)/testdata/static/tcvc-aes-sha512-ntfs-lfn-unicode.hc \
  cargo test --workspace --all-targets -- --ignored
```

These fixtures are committed and always present in this repository, so this step should always be run for a release candidate, not skipped.

## 5. VeraCrypt-Generated Fixture Scripts (Optional)

Only run these if the VeraCrypt CLI is actually available locally (`scripts/ensure-veracrypt-cli.sh` resolves it from `PATH` or the common macOS app bundle path, and can install it via Homebrew on macOS if missing). They are not required for a release candidate — the committed static fixtures already cover the behaviors that matter — but strengthen confidence if VeraCrypt is available:

```bash
./scripts/test-with-veracrypt-fixtures.sh
./scripts/test-with-tcvc-crypto-matrix.sh
```

If VeraCrypt is not available, record these as skipped with that reason rather than blocking the release on them.

## 6. Manual GUI Smoke Test

Using `npm run tauri dev` (or a built binary) and the fixtures documented in [test-containers.md](test-containers.md) (password `test-password`, always with an explicit KDF hint — never "Auto" for a wrong-password check, which forces a 15+ minute autoprobe):

- [ ] Open the app; the Open Volume page renders.
- [ ] Open the FAT fixture (`tcvc-aes-sha512-fat-lfn-unicode.hc`).
- [ ] Wrong-password test with an explicit KDF hint shows a sanitized error.
- [ ] Correct-password open transitions to the Volume Browser.
- [ ] Browse into `Folder With Spaces`.
- [ ] Select a file.
- [ ] Extract the file; verify success (and byte-for-byte correctness against `testdata/static/fs-fat-lfn-original/` if checking by hand).
- [ ] Open the exFAT fixture (`tcvc-aes-sha512-exfat-lfn-unicode.hc`).
- [ ] Open the NTFS fixture (`tcvc-aes-sha512-ntfs-lfn-unicode.hc`).
- [ ] Try the directory-extraction-unsupported flow (select a directory, confirm the status bar message, confirm extraction stays disabled).
- [ ] Test keyboard navigation (arrow keys, Enter, Backspace, Escape) in the directory table.
- [ ] Test the `Browse...` file dialog for the container path.
- [ ] Test the save dialog for the extraction destination.

## 7. Manual CLI Smoke Test

```bash
cryptovol info <container>
cryptovol test-open <container>
cryptovol ls <container> /
cryptovol extract <container> <source-path> <destination-path>
```

- [ ] `info` reports file metadata and header candidates without decrypting.
- [ ] `test-open` succeeds with the correct password and explicit KDF hint.
- [ ] `ls` lists the expected directory contents, including long/Unicode/emoji names.
- [ ] `extract` extracts a known file byte-for-byte.
- [ ] Wrong-password behavior returns a clean non-zero exit with a sanitized message.
- [ ] Unsupported file/directory behavior (e.g. attempting to extract a directory) fails cleanly with the documented exit code.

## 8. Documentation And Compliance Review

- [ ] Re-review [security.md](security.md) for accuracy against the current code (no new overclaims introduced since the last release).
- [ ] Re-check [license-decision.md](license-decision.md) — do not proceed with public distribution while license status is `blocked`.
- [ ] Re-run both dependency license inventories from [dependency-licenses.md](dependency-licenses.md) and review any dependency added since the last release.
- [ ] Re-review [beta-readiness.md](beta-readiness.md)'s "Known Risks" and "What is explicitly not supported" sections for anything that changed.

## 9. macOS Packaging, Signing, Notarization, And Checksums

See [packaging-macos.md](packaging-macos.md) for the full reference (artifact layout, required certificate/env vars, troubleshooting). Summary for a release candidate:

- [ ] Build a local package and sanity-check it:

  ```bash
  ./scripts/package-macos-local.sh
  ```

  Confirms the Tauri build, frontend build, and bundling still succeed end to end, and produces a `.app`/`.dmg` under `dist/macos/<version>/` for local inspection. This does not require Apple Developer credentials and is not release-ready on its own.

- [ ] If Apple Developer credentials are available (`APPLE_SIGNING_IDENTITY` plus a complete App Store Connect API or Apple ID credential set — see `packaging-macos.md`), build the signed/notarized release candidate:

  ```bash
  ./scripts/package-macos-release.sh
  ```

  This single command performs, and aborts the release on failure of any of:
  - [ ] `codesign --verify --deep --strict --verbose=2` on the `.app`
  - [ ] `codesign -dv --verbose=4` on the `.app`
  - [ ] `spctl --assess --type execute --verbose=4` on the `.app`
  - [ ] `spctl --assess --type open --context context:primary-signature --verbose=4` on the `.dmg` (best-effort; a warning here does not block, since `stapler validate` below is authoritative)
  - [ ] `xcrun stapler validate` on both the `.app` and the `.dmg`

  If credentials are not available, run it anyway and confirm it fails fast naming the exact missing environment variable(s) — do not skip this check silently.

- [ ] Generate checksums (also run automatically by `package-macos-release.sh`, but confirm they exist):

  ```bash
  ./scripts/create-checksums.sh
  ```

  Confirm `dist/macos/<version>/SHA256SUMS.txt` lists the `.dmg` with a relative filename.

- [ ] Review `dist/macos/<version>/build-info.txt` for accuracy (version, commit, signing mode — should read `unsigned`/`ad-hoc` for a local build or `signed+notarized` for a release build, never overstated).

- [ ] Re-run the **Manual GUI Smoke Test** checklist from Section 6, but launched from the actual packaged `.app` in `dist/macos/<version>/` (e.g. `open "dist/macos/<version>/Crypto Volume Viewer.app"`), not `npm run tauri dev` — this is the only way to confirm the bundled, (optionally) signed artifact itself opens and works, not just the dev build.

## 10. Release Notes Draft

- [ ] Update [RELEASE_NOTES.md](../RELEASE_NOTES.md) at the repo root: bump the version heading, and review the supported/unsupported/known-limitations lists against what actually changed since the last release candidate (new features, fixes, known issues from [known-issues.md](known-issues.md), and any status-table changes in `beta-readiness.md`).
- [ ] Call out any newly-introduced known limitation explicitly, rather than leaving it to be discovered.

## 11. GitHub Release

- [ ] Confirm `xcrun stapler validate` passes on both the `.app` and the `.dmg` in `dist/macos/<version>/`.
- [ ] Confirm `RELEASE_NOTES.md` matches this version.
- [ ] Publish:

  ```bash
  ./scripts/publish-github-release.sh
  ```

  This creates tag `v<version>` and attaches the `.dmg` plus `SHA256SUMS.txt`.
- [ ] Open the release URL and confirm the DMG downloads.

## 12. Rollback Notes

- [ ] Confirm the previous release's git tag is still reachable. Rolling back a bad GitHub Release means marking it as latest-on-a-previous-tag, not deleting the git history.
- [ ] Do not overwrite `dist/macos/<previous-version>/` when packaging a new version (the scripts already isolate per version).

## On `scripts/check-beta-readiness.sh`

No such script was added. A thin wrapper around the commands above would either (a) just re-run them verbatim, duplicating this document and risking the two silently drifting apart, or (b) grow its own flags/logic to be genuinely useful, which is unwarranted complexity for a project with no CI to integrate it with yet. This checklist is the single source of truth for the local acceptance process; if a wrapper script becomes worth it once CI/hosting is chosen, add it then.
