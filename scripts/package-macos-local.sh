#!/usr/bin/env bash
# Build a local unsigned/ad-hoc macOS package for testing. Does not require
# Apple Developer credentials and does not claim the result is
# public-release-ready -- see scripts/package-macos-release.sh for that.
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib/packaging-common.sh
source "$script_dir/lib/packaging-common.sh"

require_tool() {
  local tool="$1"
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "ERROR: required tool not found on PATH: $tool" >&2
    exit 1
  fi
}

main() {
  if [[ "$(uname -s)" != "Darwin" ]]; then
    echo "ERROR: scripts/package-macos-local.sh only supports macOS (found $(uname -s))." >&2
    exit 1
  fi

  require_tool node
  require_tool npm
  require_tool cargo

  local root
  root="$(project_root)"

  if ! (cd "$root/apps/cryptovol-gui" && npm run --silent tauri -- --version >/dev/null 2>&1); then
    echo "ERROR: Tauri CLI is not resolvable via 'npm run tauri -- --version' in apps/cryptovol-gui. Run 'npm install' there first." >&2
    exit 1
  fi

  echo "==> Building Crypto Volume Viewer (app, dmg bundles)..."
  (cd "$root/apps/cryptovol-gui" && npm run tauri build -- --bundles app,dmg)

  local target_dir
  target_dir="$(cd "$root" && cargo metadata --no-deps --format-version 1 2>/dev/null | node -e '
    let data = "";
    process.stdin.on("data", d => data += d);
    process.stdin.on("end", () => {
      process.stdout.write(JSON.parse(data).target_directory);
    });
  ')"
  local bundle_dir="$target_dir/release/bundle"

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

  local out_dir
  out_dir="$(dist_dir)"
  local app_name dmg_name
  app_name="$(basename "$app_src")"
  dmg_name="$(basename "$dmg_src")"

  rm -rf "${out_dir:?}/${app_name}"
  cp -R "$app_src" "$out_dir/"
  cp "$dmg_src" "$out_dir/"

  local signing_state
  local codesign_output
  codesign_output="$(codesign -dv --verbose=2 "$out_dir/$app_name" 2>&1)" || true
  if echo "$codesign_output" | grep -q "code object is not signed"; then
    signing_state="unsigned"
  elif echo "$codesign_output" | grep -q 'Authority=.*Developer ID Application'; then
    signing_state="signed (unverified locally, see package-macos-release.sh for full verification)"
  elif echo "$codesign_output" | grep -qE '^Signature=adhoc'; then
    signing_state="ad-hoc"
  else
    signing_state="unknown (see codesign -dv --verbose=2 output)"
  fi

  write_build_info "$signing_state" "$app_name" "$dmg_name"

  echo
  echo "==> Local packaging complete."
  echo "    App:        $out_dir/$app_name"
  echo "    DMG:        $out_dir/$dmg_name"
  echo "    Build info: $out_dir/build-info.txt"
  echo "    Signing state detected: $signing_state"
  echo
  echo "NOTE: this is a local/dev artifact only. It is not signed, notarized,"
  echo "      or proven release-ready. Use scripts/package-macos-release.sh"
  echo "      for a Developer-ID-signed and notarized release candidate."
}

main "$@"
