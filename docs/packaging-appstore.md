# Mac App Store packaging (not yet shipping)

Outside-store DMG is live: https://github.com/TilmannF/crypto-volume-viewer/releases

This file is the App Store channel. Do not merge these settings into a GitHub DMG build. Sandbox entitlements break typed container/extract paths used by the current GUI.

## Already in place

- Bundle ID `com.flgnr.cryptovol`, team `V7PH82SSQV`
- Version `0.1.0` (`CFBundleShortVersionString`); `bundle.macOS.bundleVersion` is `1` (increment on every store upload)
- Overlay: `apps/cryptovol-gui/src-tauri/tauri.appstore.conf.json` plus `apps/cryptovol-gui/src-tauri/appstore/` (sandbox entitlements, MAS provision profile `cryptovol-mas`, `ITSAppUsesNonExemptEncryption=true`)
- Category `public.app-category.utilities`
- Privacy policy: https://github.com/TilmannF/crypto-volume-viewer/blob/main/docs/privacy.md
- Support URL: https://github.com/TilmannF/crypto-volume-viewer/issues
- App Store Connect API key exists (same key used for notarization)

## Remaining before first upload

1. **Mac Installer Distribution certificate.** `productbuild` needs `3rd Party Mac Developer Installer: Tilmann Felgner (V7PH82SSQV)`. The `.cer` is in Downloads but there is no matching private-key identity in the keychain. Create a new CSR in Keychain Access, request a Mac Installer Distribution cert, import it, confirm with `security find-identity -v`.
2. **App Sandbox GUI work.** MAS requires sandbox. Today the Open Volume path and extract destination are typed `TextField`s; extraction writes a temp file in the destination parent directory. Under sandbox those fail. Change the GUI to:
   - Open the container only via the native Open panel (`dialog:allow-open` already exists).
   - Extract only via the native Save panel (`dialog:allow-save` already exists).
   - Write the extract temp file in a sandbox-writable place (app container) or directly into the security-scoped destination file — not a sibling tempfile next to a path the panel did not grant.
   - Prove this with a sandboxed, MAS-signed `.app` (not `tauri dev`).
3. **Do not run MAS-signed code as a daily driver.** Use an Apple Development identity for local sandbox tests; App Store distribution-signed apps are not meant to be launched locally.
4. **App Store Connect listing.** Create the Mac app record with bundle ID `com.flgnr.cryptovol`. Fill screenshots, description, age rating, privacy nutrition labels (no tracking / no data collected). Review notes must include the test container + password `test-password` and KDF hint `SHA-512`.
5. **Encryption export questionnaire.** `ITSAppUsesNonExemptEncryption` is `true` in the overlay (AES-XTS, PBKDF2, and related KDFs are not HTTPS-exempt). Complete App Store Connect encryption questions; if Apple issues a compliance code, put it in the overlay Info.plist as `ITSEncryptionExportComplianceCode`. This is legal/export, not a packaging checkbox.
6. **Build and upload.** Sign the `.app` with `3rd Party Mac Developer Application`, embed `cryptovolmas.provisionprofile`, `productbuild` with the Installer identity, upload with `xcrun altool --upload-app --type macos` using the API key. Then TestFlight, then Review.

Keep GitHub DMG builds on the default `tauri.conf.json` (no sandbox). Apply the overlay only with `--config src-tauri/tauri.appstore.conf.json`.
