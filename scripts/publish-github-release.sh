#!/usr/bin/env bash
# Publish dist/macos/<version>/ artifacts to a GitHub Release.
# Does not build, sign, or notarize — run scripts/package-macos-release.sh first.
# Never prints credential values.
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib/packaging-common.sh
source "$script_dir/lib/packaging-common.sh"

if ! command -v gh >/dev/null 2>&1; then
  echo "ERROR: gh (GitHub CLI) is required. Install it and run gh auth login." >&2
  exit 1
fi

if ! git -C "$(project_root)" remote get-url origin >/dev/null 2>&1; then
  echo "ERROR: no git remote named origin. Create the GitHub repo first." >&2
  exit 1
fi

version="$(package_version)"
tag="v${version}"
out="$(dist_dir)"

dmg="$(find "$out" -maxdepth 1 -iname '*.dmg' -print -quit 2>/dev/null || true)"
sums="$out/SHA256SUMS.txt"
notes="$(project_root)/RELEASE_NOTES.md"

if [[ -z "$dmg" || ! -f "$dmg" ]]; then
  echo "ERROR: no .dmg in $out — run ./scripts/package-macos-release.sh first." >&2
  exit 1
fi
if [[ ! -f "$sums" ]]; then
  echo "ERROR: missing $sums — run ./scripts/create-checksums.sh after packaging." >&2
  exit 1
fi
if [[ ! -f "$notes" ]]; then
  echo "ERROR: missing $notes" >&2
  exit 1
fi

echo "==> Verifying stapled notarization on DMG (failure aborts publish)..."
xcrun stapler validate "$dmg"

if gh release view "$tag" >/dev/null 2>&1; then
  echo "ERROR: GitHub release $tag already exists." >&2
  exit 1
fi

echo "==> Creating GitHub release $tag"
gh release create "$tag" \
  --title "Crypto Volume Viewer ${version}" \
  --notes-file "$notes" \
  "$dmg" \
  "$sums"

echo
echo "==> Published $tag"
echo "    https://github.com/$(gh repo view --json nameWithOwner -q .nameWithOwner)/releases/tag/${tag}"
