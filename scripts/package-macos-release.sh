#!/usr/bin/env bash
# Build a Developer-ID-signed and notarized macOS release candidate.
# Requires APPLE_SIGNING_IDENTITY plus either the full App Store Connect
# API credential set or the full Apple ID credential set in the
# environment. Never prints credential values. Unlike
# scripts/package-macos-local.sh, this script must fail loudly on any
# signing/notarization/verification problem rather than silently degrade.
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib/packaging-common.sh
source "$script_dir/lib/packaging-common.sh"

# Optional local credential file (gitignored via .env.*). Never printed.
load_optional_env_file() {
  local env_file
  env_file="$(project_root)/.env.macos-release"
  if [[ -f "$env_file" ]]; then
    set -a
    # shellcheck disable=SC1090
    source "$env_file"
    set +a
  fi
}

check_prerequisites() {
  if [[ "$(uname -s)" != "Darwin" ]]; then
    echo "ERROR: scripts/package-macos-release.sh only supports macOS (found $(uname -s))." >&2
    exit 1
  fi

  if [[ -z "${APPLE_SIGNING_IDENTITY:-}" ]]; then
    echo "ERROR: missing required environment variable: APPLE_SIGNING_IDENTITY" >&2
    echo "       Find installed identities with: security find-identity -v -p codesigning" >&2
    echo "       Or put it in gitignored .env.macos-release at the repo root." >&2
    exit 1
  fi

  case "${APPLE_SIGNING_IDENTITY}" in
    "Developer ID Application:"*) ;;
    *)
      echo "ERROR: APPLE_SIGNING_IDENTITY must be a Developer ID Application identity." >&2
      echo "       App Store identities cannot sign a notarized outside-store DMG." >&2
      echo "       Discover identities with: security find-identity -v -p codesigning" >&2
      exit 1
      ;;
  esac

  check_keychain_trust_settings

  local api_missing=()
  [[ -z "${APPLE_API_ISSUER:-}" ]] && api_missing+=("APPLE_API_ISSUER")
  [[ -z "${APPLE_API_KEY:-}" ]] && api_missing+=("APPLE_API_KEY")
  [[ -z "${APPLE_API_KEY_PATH:-}" ]] && api_missing+=("APPLE_API_KEY_PATH")

  local appleid_missing=()
  [[ -z "${APPLE_ID:-}" ]] && appleid_missing+=("APPLE_ID")
  [[ -z "${APPLE_PASSWORD:-}" ]] && appleid_missing+=("APPLE_PASSWORD")
  [[ -z "${APPLE_TEAM_ID:-}" ]] && appleid_missing+=("APPLE_TEAM_ID")

  if [[ ${#api_missing[@]} -eq 0 || ${#appleid_missing[@]} -eq 0 ]]; then
    # At least one full notarization credential set is present.
    return 0
  fi

  echo "ERROR: no complete notarization credential set found in the environment." >&2
  echo "       Provide EITHER the full App Store Connect API set, missing: ${api_missing[*]}" >&2
  echo "       OR the full Apple ID set, missing: ${appleid_missing[*]}" >&2
  exit 1
}

# Keychain "Always Trust" / TrustAsRoot on the Developer ID leaf or Apple's
# Developer ID CA makes codesign treat the leaf as a custom root: TeamIdentifier
# is omitted and the designated requirement fails `codesign --verify --strict`.
# Fix: Keychain Access → certificate → Trust → Use System Defaults.
check_keychain_trust_settings() {
  local trust_dump
  trust_dump="$(security dump-trust-settings 2>/dev/null || true)"
  if [[ -z "$trust_dump" ]]; then
    return 0
  fi

  local flagged
  flagged="$(echo "$trust_dump" | awk '
    /^Cert [0-9]+:/ { cert=$0; keep=0 }
    /Developer ID Application:/ || /Developer ID Certification Authority/ { keep=1 }
    /TrustAsRoot/ && keep { print cert; keep=0 }
  ')"
  if [[ -n "$flagged" ]]; then
    echo "ERROR: Keychain trust settings mark a Developer ID certificate as TrustAsRoot." >&2
    echo "       That produces a broken designated requirement and empty TeamIdentifier." >&2
    echo "       In Keychain Access, open the cert → Trust → Use System Defaults." >&2
    echo "       Offending entries:" >&2
    echo "$flagged" | sed 's/^/         /' >&2
    exit 1
  fi
}

# Submit a Developer-ID-signed .dmg to notarytool and staple the ticket.
# Never prints credential values. Uses the App Store Connect API set when
# complete, otherwise the Apple ID set (check_prerequisites already required
# one of the two).
notarize_and_staple_dmg() {
  local dmg="$1"

  if xcrun stapler validate "$dmg" >/dev/null 2>&1; then
    echo "    DMG already has a stapled ticket."
    return 0
  fi

  echo "    Submitting DMG to Apple notary service (this can take several minutes)..."
  if [[ -n "${APPLE_API_KEY_PATH:-}" && -n "${APPLE_API_KEY:-}" && -n "${APPLE_API_ISSUER:-}" ]]; then
    xcrun notarytool submit "$dmg" --wait \
      --key "$APPLE_API_KEY_PATH" \
      --key-id "$APPLE_API_KEY" \
      --issuer "$APPLE_API_ISSUER"
  else
    xcrun notarytool submit "$dmg" --wait \
      --apple-id "$APPLE_ID" \
      --password "$APPLE_PASSWORD" \
      --team-id "$APPLE_TEAM_ID"
  fi

  echo "    Stapling notarization ticket onto DMG..."
  xcrun stapler staple "$dmg"
}

resolve_bundle_dir() {
  local root="$1"
  local target_dir
  target_dir="$(cd "$root" && cargo metadata --no-deps --format-version 1 2>/dev/null | node -e '
    let data = "";
    process.stdin.on("data", d => data += d);
    process.stdin.on("end", () => {
      process.stdout.write(JSON.parse(data).target_directory);
    });
  ')"
  printf '%s\n' "$target_dir/release/bundle"
}

run_release_build() {
  local root
  root="$(project_root)"

  echo "==> Building signed Crypto Volume Viewer (app, dmg bundles)..."
  (cd "$root/apps/cryptovol-gui" && APPLE_SIGNING_IDENTITY="${APPLE_SIGNING_IDENTITY}" npm run tauri build -- --bundles app,dmg)

  local bundle_dir
  bundle_dir="$(resolve_bundle_dir "$root")"

  local app_src dmg_src
  app_src="$(find "$bundle_dir/macos" -maxdepth 1 -iname '*.app' -print -quit 2>/dev/null || true)"
  dmg_src="$(find "$bundle_dir/dmg" -maxdepth 1 -iname '*.dmg' -print -quit 2>/dev/null || true)"

  if [[ -z "$app_src" ]]; then
    echo "ERROR: no .app bundle found under $bundle_dir/macos -- Tauri build layout may have changed." >&2
    exit 1
  fi
  if [[ -z "$dmg_src" ]]; then
    echo "ERROR: no .dmg bundle found under $bundle_dir/dmg -- Tauri build layout may have changed." >&2
    exit 1
  fi

  local out_dir app_name dmg_name
  out_dir="$(dist_dir)"
  app_name="$(basename "$app_src")"
  dmg_name="$(basename "$dmg_src")"

  rm -rf "${out_dir:?}/${app_name}"
  cp -R "$app_src" "$out_dir/"
  cp "$dmg_src" "$out_dir/"

  local app_dst="$out_dir/$app_name"
  local dmg_dst="$out_dir/$dmg_name"

  echo "==> Verifying code signature (failure aborts release packaging)..."
  codesign --verify --deep --strict --verbose=2 "$app_dst"
  local codesign_info
  codesign_info="$(codesign -dv --verbose=4 "$app_dst" 2>&1)"
  echo "$codesign_info"
  if ! echo "$codesign_info" | grep -q 'TeamIdentifier=V7PH82SSQV'; then
    echo "ERROR: signed app is missing TeamIdentifier=V7PH82SSQV (see Keychain TrustAsRoot note in docs/packaging-macos.md)." >&2
    exit 1
  fi
  if ! echo "$codesign_info" | grep -q 'Authority=Developer ID Application:'; then
    echo "ERROR: signed app is not using a Developer ID Application authority." >&2
    exit 1
  fi

  echo "==> Verifying Gatekeeper acceptance of the app (failure aborts release packaging)..."
  spctl --assess --type execute --verbose=4 "$app_dst"

  echo "==> Verifying notarization ticket is stapled on the app (failure aborts release packaging)..."
  xcrun stapler validate "$app_dst"

  # Tauri notarizes and staples the .app, then builds the .dmg around it and
  # only Developer-ID-signs the image. The image itself has no ticket until
  # we submit it separately. Checksums are generated after this step because
  # stapling modifies the .dmg bytes.
  echo "==> Notarizing and stapling the DMG..."
  notarize_and_staple_dmg "$dmg_dst"

  echo "==> Verifying notarization ticket is stapled on the DMG (failure aborts release packaging)..."
  xcrun stapler validate "$dmg_dst"

  if ! spctl --assess --type open --context context:primary-signature --verbose=4 "$dmg_dst"; then
    echo "WARNING: spctl --assess --type open on the .dmg did not succeed on this system; treating the xcrun stapler validate check above as authoritative for notarization." >&2
  fi

  local signing_mode="signed+notarized"

  echo "==> Generating checksums..."
  "$script_dir/create-checksums.sh"

  write_build_info "$signing_mode" "$app_name" "$dmg_name"

  echo
  echo "==> Release packaging complete."
  echo "    App:  $app_dst"
  echo "    DMG:  $dmg_dst"
  echo "    Signing mode: $signing_mode"
}

main() {
  load_optional_env_file
  check_prerequisites
  run_release_build
}

main "$@"
