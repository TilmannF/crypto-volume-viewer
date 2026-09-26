#!/usr/bin/env bash
# Downloads a published GitHub Release into an empty temporary directory and
# checks it the way a user would: `shasum -a 256 -c SHA256SUMS.txt` passes,
# every asset is listed in SHA256SUMS.txt, and every .dmg has a stapled
# notarization ticket. Read-only: never changes the release.
# Usage: scripts/verify-github-release.sh [tag]   (default: v<current version>)
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib/packaging-common.sh
source "$script_dir/lib/packaging-common.sh"

if ! command -v gh >/dev/null 2>&1; then
  echo "ERROR: gh (GitHub CLI) is required. Install it and run gh auth login." >&2
  exit 1
fi

tag="${1:-v$(package_version)}"

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

echo "==> Downloading release $tag into an empty directory..."
(cd "$(project_root)" && gh release download "$tag" --dir "$work")

echo "==> Checking SHA256SUMS.txt against the downloaded assets..."
verify_release_dir "$work"

found_dmg=0
for dmg in "$work"/*.dmg; do
  [[ -e "$dmg" ]] || continue
  found_dmg=1
  echo "==> Verifying stapled notarization on ${dmg##*/}..."
  xcrun stapler validate "$dmg"
done
if [[ $found_dmg -eq 0 ]]; then
  echo "ERROR: release $tag has no .dmg asset" >&2
  exit 1
fi

echo
echo "==> Release $tag verified: checksums match and every DMG is stapled."
