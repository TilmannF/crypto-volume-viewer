#!/usr/bin/env bash
# Publish dist/macos/<version>/ artifacts to a GitHub Release, then download
# the release and verify it with scripts/verify-github-release.sh.
# Does not build, sign, or notarize — run scripts/package-macos-release.sh first.
# --dry-run runs every pre-publish check and stops before creating the release.
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

dry_run=0
case "${1:-}" in
  "") ;;
  --dry-run) dry_run=1 ;;
  *)
    echo "Usage: $0 [--dry-run]" >&2
    exit 2
    ;;
esac

version="$(package_version)"
tag="v${version}"
out="$(dist_dir)"
notes="$(project_root)/RELEASE_NOTES.md"

if ! dmg="$(single_dmg_in "$out")"; then
  echo "       Run ./scripts/package-macos-release.sh first." >&2
  exit 1
fi
if [[ ! -f "$notes" ]]; then
  echo "ERROR: missing $notes" >&2
  exit 1
fi

# The release is exactly the DMG plus SHA256SUMS.txt, and SHA256SUMS.txt
# lists exactly that DMG under its published name, so
# `shasum -a 256 -c SHA256SUMS.txt` works for every downloader and no other
# file in dist/ (for example one left over from an older build) is published.
echo "==> Checking SHA256SUMS.txt against $out (failure aborts publish)..."
if ! listed="$(check_sums_file "$out")"; then
  echo "       Re-run ./scripts/package-macos-release.sh; it writes the DMG under a GitHub-safe" >&2
  echo "       name and regenerates SHA256SUMS.txt." >&2
  exit 1
fi
if [[ "$listed" != "${dmg##*/}" ]]; then
  echo "ERROR: SHA256SUMS.txt must list exactly ${dmg##*/}; it lists:" >&2
  while IFS= read -r name; do
    printf '         %s\n' "$name" >&2
  done <<< "$listed"
  echo "       Re-run ./scripts/create-checksums.sh." >&2
  exit 1
fi
assets=("$dmg" "$out/SHA256SUMS.txt")

echo "==> Verifying stapled notarization on DMG (failure aborts publish)..."
xcrun stapler validate "$dmg"

echo "==> Checking that GitHub release $tag does not exist yet..."
if ! state="$(github_release_state "$tag")"; then
  exit 1
fi
if [[ "$state" == "exists" ]]; then
  echo "ERROR: GitHub release $tag already exists." >&2
  exit 1
fi

if [[ $dry_run -eq 1 ]]; then
  echo
  echo "==> Dry run: all checks passed. Would create release $tag with:"
  printf '    %s\n' "${assets[@]##*/}"
  exit 0
fi

echo "==> Creating GitHub release $tag"
gh release create "$tag" \
  --title "Crypto Volume Viewer ${version}" \
  --notes-file "$notes" \
  "${assets[@]}"

echo
echo "==> Published $tag"
echo "    https://github.com/$(gh repo view --json nameWithOwner -q .nameWithOwner)/releases/tag/${tag}"

echo
echo "==> Downloading $tag and verifying it as a user would..."
if ! "$script_dir/verify-github-release.sh" "$tag"; then
  echo "ERROR: the published release $tag failed verification. It is live: fix its assets" >&2
  echo "       (gh release upload $tag <file> --clobber) or delete it before announcing it." >&2
  exit 1
fi
