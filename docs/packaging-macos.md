# macOS Packaging

## Goal and scope

This document covers the local, reproducible macOS packaging foundation added in the `macos-packaging-foundation` milestone: turning a checkout into a `Crypto Volume Viewer.app` and `.dmg`, optionally Developer-ID-signed and Apple-notarized, plus checksums, release notes, and build metadata.

This is macOS-first and local-only. See "What is not covered" below for what this milestone deliberately does not do.

Two distribution channels exist. **The outside-store DMG path is active** (GitHub Releases). Mac App Store packaging is prepared as an overlay, not used by these scripts.

- **Default (`tauri.conf.json` + `scripts/package-macos-*.sh`):** Developer ID signed, Apple-notarized `.app` / `.dmg`. No App Sandbox, no Mac App Store provisioning profile. Published via GitHub Releases (`scripts/publish-github-release.sh`).
- **Later (`src-tauri/tauri.appstore.conf.json` + `src-tauri/appstore/`):** Mac App Store overlay. Not used by the packaging scripts. Do not merge it into a DMG build — sandbox entitlements would break typed container/extract paths.

- **Product name:** Crypto Volume Viewer
- **CLI binary name:** `cryptovol` (unaffected by GUI packaging)
- **Bundle identifier:** `com.flgnr.cryptovol`

## Artifact layout

Every packaging run collects output under `dist/macos/<version>/` (version read from `apps/cryptovol-gui/src-tauri/tauri.conf.json`, e.g. `dist/macos/0.1.0/`):

```text
dist/macos/<version>/
  Crypto Volume Viewer.app
  Crypto Volume Viewer_<version>_<arch>.dmg
  SHA256SUMS.txt
  build-info.txt
```

`RELEASE_NOTES.md` lives at the repository root (not copied per-version). `dist/` is gitignored — built artifacts are never committed. Public downloads are GitHub Release assets, not git files.

Re-running packaging for the *same* version overwrites that version's own files. It never touches a different version's directory.

## Local unsigned/ad-hoc build

```bash
./scripts/package-macos-local.sh
```

Requires no Apple Developer credentials. Runs `npm run tauri build -- --bundles app,dmg` from `apps/cryptovol-gui` (which also runs the frontend build), copies the resulting `.app`/`.dmg` into `dist/macos/<version>/`, detects and prints whatever signing state is present (typically `unsigned` or `ad-hoc` on a machine with no signing identity), and writes `build-info.txt`. It explicitly does not claim the result is release-ready.

## Signed and notarized release build

```bash
export APPLE_SIGNING_IDENTITY="Developer ID Application: Your Name (TEAMID1234)"
# plus one full notarization credential set -- see below
./scripts/package-macos-release.sh
```

Optional: put the same `export` lines in a gitignored `.env.macos-release` at the repo root (matched by `.env.*`). The release script sources that file if it exists and never prints its contents.

Unlike the local script, this one fails loudly: any missing prerequisite, or any codesign/spctl/stapler verification failure, aborts the script with a non-zero exit code rather than producing an unverified artifact. It also rejects App Store signing identities (`3rd Party Mac Developer Application`, `Apple Distribution`) — those cannot notarize an outside-store DMG.

### Required certificate

You need a **Developer ID Application** certificate (for distribution *outside* the App Store) — not an App Store distribution certificate. Discover installed identities with:

```bash
security find-identity -v -p codesigning
```

Set `APPLE_SIGNING_IDENTITY` to the full identity string shown (e.g. `Developer ID Application: Your Name (TEAMID1234)`).

### Required environment variables

Always required:

- `APPLE_SIGNING_IDENTITY`

Plus **one full set** of notarization credentials:

**App Store Connect API** (recommended):
- `APPLE_API_ISSUER`
- `APPLE_API_KEY`
- `APPLE_API_KEY_PATH`

**Apple ID + app-specific password**:
- `APPLE_ID`
- `APPLE_PASSWORD`
- `APPLE_TEAM_ID`

If `APPLE_SIGNING_IDENTITY` is missing, or neither credential set is complete, `scripts/package-macos-release.sh` exits immediately and names exactly which variable(s) are missing. It never prints the value of any of these variables.

### Verification performed automatically

`scripts/package-macos-release.sh` runs, in order, and aborts on any failure:

```bash
codesign --verify --deep --strict --verbose=2 "Crypto Volume Viewer.app"
codesign -dv --verbose=4 "Crypto Volume Viewer.app"
spctl --assess --type execute --verbose=4 "Crypto Volume Viewer.app"
spctl --assess --type open --context context:primary-signature --verbose=4 "Crypto Volume Viewer.dmg"
xcrun stapler validate "Crypto Volume Viewer.app"
xcrun stapler validate "Crypto Volume Viewer.dmg"
```

The `spctl --assess --type open` check on the `.dmg` is treated as best-effort (a warning, not a hard failure) since its support varies by system; the two `stapler validate` checks are the authoritative, mandatory proof of a stapled notarization ticket.

Tauri notarizes and staples the `.app`, then builds the `.dmg` around that already-notarized app and only Developer-ID-signs the image. `scripts/package-macos-release.sh` therefore submits the `.dmg` to `notarytool` itself and staples the ticket before checksums are generated (stapling changes the `.dmg` bytes).

## Checksums

```bash
./scripts/create-checksums.sh
```

Hashes the `.dmg` (and a zipped `.app`, if one exists) in `dist/macos/<version>/` with `shasum -a 256`, writing `SHA256SUMS.txt` with relative filenames. It never attempts to hash a raw `.app` directory. Both packaging scripts call it internally, but it can also be run standalone against an already-built `dist/macos/<version>/`.

To verify a downloaded `.dmg` against `SHA256SUMS.txt`:

```bash
shasum -a 256 -c SHA256SUMS.txt
```

## GitHub Releases

Public download URL: https://github.com/TilmannF/crypto-volume-viewer/releases

After a successful `./scripts/package-macos-release.sh` (`.dmg` passes `xcrun stapler validate`):

```bash
./scripts/publish-github-release.sh
```

This creates git tag `v<version>` and a GitHub Release attaching the `.dmg` and `SHA256SUMS.txt`, using `RELEASE_NOTES.md` as the release body. It does not build or notarize. CI does not produce this artifact — signing and notarization stay on the local Mac with the Developer ID identity.

## What is not covered by this milestone

- CI (GitHub Actions) that builds or notarizes. Releases are created locally, then uploaded with `scripts/publish-github-release.sh`.
- Auto-update.
- Windows or Linux packaging.
- Mac App Store packaging. Overlay files live under `apps/cryptovol-gui/src-tauri/appstore/` and `tauri.appstore.conf.json`. `scripts/package-macos-release.sh` never applies them. Remaining store work: [packaging-appstore.md](packaging-appstore.md).

## Troubleshooting

**"missing required environment variable: APPLE_SIGNING_IDENTITY" / missing notarization variables**
`scripts/package-macos-release.sh` checked and one or more required variables aren't set. Run `security find-identity -v -p codesigning` to find a Developer ID Application identity, and confirm you have a complete App Store Connect API set or Apple ID set (see above) — a partial set of either is treated as incomplete. An App Store Connect API key is a `.p8` file downloaded once from [Users and Access → Integrations](https://appstoreconnect.apple.com/access/integrations); store the path in `APPLE_API_KEY_PATH` (or in `.env.macos-release`), never in git.

**`codesign --verify` says "does not satisfy its designated Requirement" / `TeamIdentifier=not set`**
The Developer ID leaf or Apple's Developer ID Certification Authority has a custom Keychain trust setting (`TrustAsRoot` / "Always Trust"), often added to work around an expiry warning. That makes `codesign` treat the leaf as a custom root instead of an Apple Developer ID identity. In Keychain Access, open the certificate → Trust → **Use System Defaults**. Then re-run `scripts/package-macos-release.sh`. Do not set Developer ID certificates to Always Trust.

**Notarization rejected**
Check `xcrun notarytool log` (or the equivalent output from `tauri build`) for the rejection reason — common causes are an unsigned nested binary, a missing hardened runtime entitlement, or an expired/revoked certificate. This is outside the scope of this document; consult Apple's notarization documentation.

**`stapler validate` fails on the `.dmg` ("does not have a ticket stapled to it") while the `.app` passes**
Expected from Tauri alone: it staples the `.app` then creates the `.dmg`. Current `scripts/package-macos-release.sh` notarizes and staples the `.dmg` in a second pass. If an older run left an unstapled `.dmg` in `dist/`, re-run the release script (or let it submit that `.dmg`); do not ship a `.dmg` that fails `xcrun stapler validate`.

**Gatekeeper blocks an unsigned/ad-hoc local build from opening**
Expected: `scripts/package-macos-local.sh` does not produce a Developer-ID-signed artifact. Right-click the `.app` and choose Open, or use `spctl --assess` to see the exact rejection reason. Use `scripts/package-macos-release.sh` for a build that macOS will open without a manual override.

**Tauri bundle output path differs from what's documented here**
`scripts/package-macos-local.sh` and `scripts/package-macos-release.sh` do not hardcode a bundle output path — they resolve it dynamically via `cargo metadata`'s `target_directory` field plus Tauri's `release/bundle/{macos,dmg}/` convention. In this repository (a Cargo workspace), that resolves to `<repo>/target/release/bundle/`, not `apps/cryptovol-gui/src-tauri/target/release/bundle/` as a single-crate Tauri project would use. If a future Tauri version changes the `release/bundle/{macos,dmg}/` convention itself, both scripts will fail with a clear "no .app/.dmg bundle found under ..." error naming the path they searched.
