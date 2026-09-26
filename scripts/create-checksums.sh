#!/usr/bin/env bash
# Writes SHA256SUMS.txt for the current version's packaged macOS artifacts.
# Hashes the single .dmg and, if present, zipped .app archives -- never a raw
# .app directory (shasum has no meaningful notion of hashing a directory).
# Every hashed name must be GitHub-safe (see release_asset_name).
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

  local names=("${dmg##*/}")
  local app_zip
  for app_zip in "$out_dir"/*.app.zip; do
    [[ -e "$app_zip" ]] && names+=("${app_zip##*/}")
  done

  # The names hashed here are the names published; GitHub would rename an
  # unsafe one on upload and break `shasum -c` for every downloader.
  local name
  for name in "${names[@]}"; do
    require_release_safe_name "$name"
  done

  local sums_file="$out_dir/SHA256SUMS.txt"
  (cd "$out_dir" && shasum -a 256 "${names[@]}" > "$sums_file")

  echo "==> Wrote $sums_file"
  cat "$sums_file"
}

main "$@"
