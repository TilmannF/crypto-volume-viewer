#!/usr/bin/env bash
# Writes SHA256SUMS.txt for the current version's packaged macOS artifacts.
# Hashes the .dmg and, if present, a zipped .app archive -- never a raw
# .app directory (shasum has no meaningful notion of hashing a directory).
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib/packaging-common.sh
source "$script_dir/lib/packaging-common.sh"

main() {
  local out_dir
  out_dir="$(dist_dir)"

  local dmg_name app_zip_name
  dmg_name="$(cd "$out_dir" && find . -maxdepth 1 -iname '*.dmg' -print -quit 2>/dev/null || true)"
  app_zip_name="$(cd "$out_dir" && find . -maxdepth 1 -iname '*.app.zip' -print -quit 2>/dev/null || true)"

  if [[ -z "$dmg_name" ]]; then
    echo "ERROR: no .dmg found in $out_dir -- run scripts/package-macos-local.sh or scripts/package-macos-release.sh first." >&2
    exit 1
  fi

  local sums_file="$out_dir/SHA256SUMS.txt"
  (
    cd "$out_dir"
    {
      shasum -a 256 "${dmg_name#./}"
      if [[ -n "$app_zip_name" ]]; then
        shasum -a 256 "${app_zip_name#./}"
      fi
    } > "$sums_file"
  )

  echo "==> Wrote $sums_file"
  cat "$sums_file"
}

main "$@"
