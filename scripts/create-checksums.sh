#!/usr/bin/env bash
# Writes SHA256SUMS.txt for the current version's packaged macOS artifacts.
# Hashes exactly the single .dmg, the only release artifact; other files in
# dist/ (the .app bundle, build-info.txt) are never listed or published.
# The hashed name must be GitHub-safe (see release_asset_name).
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib/packaging-common.sh
source "$script_dir/lib/packaging-common.sh"

main() {
  local out_dir
  out_dir="$(dist_dir)"

  local dmg
  if ! dmg="$(single_dmg_in "$out_dir")"; then
    echo "       Run scripts/package-macos-local.sh or scripts/package-macos-release.sh first." >&2
    exit 1
  fi

  # The name hashed here is the name published; GitHub would rename an
  # unsafe one on upload and break `shasum -c` for every downloader.
  local dmg_name="${dmg##*/}"
  require_release_safe_name "$dmg_name"

  local sums_file="$out_dir/SHA256SUMS.txt"
  (cd "$out_dir" && shasum -a 256 "$dmg_name" > "$sums_file")

  echo "==> Wrote $sums_file"
  cat "$sums_file"
}

main "$@"
