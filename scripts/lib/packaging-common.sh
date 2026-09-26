#!/usr/bin/env bash
# Shared helpers for the macOS packaging and release scripts
# (package-macos-local.sh, package-macos-release.sh, create-checksums.sh,
# publish-github-release.sh, verify-github-release.sh,
# check-local-release-candidate.sh) and their tests
# (test-packaging-common.sh). Meant to be sourced, not executed directly.
set -euo pipefail

# Prints the repository root's absolute path.
project_root() {
  git rev-parse --show-toplevel
}

# Prints the product version as recorded in tauri.conf.json (the single
# source of truth this milestone keeps in sync across Cargo/package.json).
package_version() {
  node -e '
    const fs = require("fs");
    const path = require("path");
    const root = process.argv[1];
    const conf = JSON.parse(
      fs.readFileSync(path.join(root, "apps/cryptovol-gui/src-tauri/tauri.conf.json"), "utf8")
    );
    process.stdout.write(conf.version);
  ' "$(project_root)"
}

# Prints the per-version artifact directory path, e.g.
# <repo>/dist/macos/0.1.0. Only ever creates/touches the directory
# for the *current* version -- never deletes or overwrites another
# version's directory. Re-running packaging for the same version does
# overwrite that version's own files, which is expected and acceptable.
dist_dir() {
  local dir
  dir="$(project_root)/dist/macos/$(package_version)"
  mkdir -p "$dir"
  printf '%s\n' "$dir"
}

# Prints the file name to use for a release artifact. GitHub rewrites release
# asset names outside [A-Za-z0-9._-] on upload (a space becomes "."), and
# SHA256SUMS.txt must list the name users actually download. Tauri names the
# DMG after the product ("Crypto Volume Viewer_<version>_<arch>.dmg"), so
# spaces are replaced with "." here, matching what GitHub would publish.
release_asset_name() {
  printf '%s\n' "${1// /.}"
}

# Fails unless $1 is a name GitHub keeps unchanged as a release asset name:
# only [A-Za-z0-9._-], not starting or ending with ".".
require_release_safe_name() {
  local name="$1"
  if [[ ! "$name" =~ ^[A-Za-z0-9_-]([A-Za-z0-9._-]*[A-Za-z0-9_-])?$ ]]; then
    echo "ERROR: '$name' is not a GitHub-safe release asset name (allowed: A-Z a-z 0-9 . _ -, no leading/trailing '.')." >&2
    echo "       GitHub would rename it on upload, and SHA256SUMS.txt would no longer match the published file." >&2
    return 1
  fi
}

# Prints the path of the single .dmg in directory $1. Fails if there is none
# or more than one, so checksums and uploads never pick an arbitrary DMG.
single_dmg_in() {
  local dir="$1"
  local dmgs=()
  local f
  for f in "$dir"/*.dmg; do
    [[ -e "$f" ]] && dmgs+=("$f")
  done
  if [[ ${#dmgs[@]} -eq 0 ]]; then
    echo "ERROR: no .dmg in $dir" >&2
    return 1
  fi
  if [[ ${#dmgs[@]} -gt 1 ]]; then
    echo "ERROR: more than one .dmg in $dir; remove the stale one(s):" >&2
    printf '         %s\n' "${dmgs[@]##*/}" >&2
    return 1
  fi
  printf '%s\n' "${dmgs[0]}"
}

# Prints the file names listed in $1/SHA256SUMS.txt, one per line, after
# checking that every line is "<sha256>  <name>", every name is GitHub-safe,
# and every listed file exists in $1. Then runs `shasum -a 256 -c` there.
check_sums_file() {
  local dir="$1"
  local sums="$dir/SHA256SUMS.txt"
  if [[ ! -f "$sums" ]]; then
    echo "ERROR: no SHA256SUMS.txt in $dir" >&2
    return 1
  fi

  local line name count=0
  local pattern='^[0-9a-f]{64} [ *](.+)$'
  while IFS= read -r line || [[ -n "$line" ]]; do
    [[ -z "$line" ]] && continue
    if [[ ! "$line" =~ $pattern ]]; then
      echo "ERROR: malformed line in $sums: $line" >&2
      return 1
    fi
    name="${BASH_REMATCH[1]}"
    require_release_safe_name "$name" || return 1
    if [[ ! -f "$dir/$name" ]]; then
      echo "ERROR: SHA256SUMS.txt lists '$name', which is not in $dir" >&2
      return 1
    fi
    printf '%s\n' "$name"
    count=$((count + 1))
  done < "$sums"
  if [[ $count -eq 0 ]]; then
    echo "ERROR: SHA256SUMS.txt in $dir lists no files" >&2
    return 1
  fi

  (cd "$dir" && shasum -a 256 -c SHA256SUMS.txt >&2)
}

# Checks a directory of downloaded release assets exactly as users receive
# them: check_sums_file passes and every downloaded file other than
# SHA256SUMS.txt is listed in it.
verify_release_dir() {
  local dir="$1"
  local listed
  listed="$(check_sums_file "$dir")" || return 1

  local f base
  for f in "$dir"/*; do
    base="${f##*/}"
    [[ "$base" == "SHA256SUMS.txt" ]] && continue
    if ! printf '%s\n' "$listed" | grep -Fxq -- "$base"; then
      echo "ERROR: release asset '$base' is not listed in SHA256SUMS.txt" >&2
      return 1
    fi
  done
}

# Writes "$(dist_dir)/build-info.txt". Usage:
#   write_build_info <signing-mode: unsigned|signed|signed+notarized> <artifact-filename>...
# Contains no credential values and no local OS username -- only tool
# versions, git/build metadata, and the caller-supplied signing mode and
# artifact filenames.
write_build_info() {
  local signing_mode="$1"
  shift
  local artifacts=("$@")

  local tauri_cli_version
  tauri_cli_version="$(cd "$(project_root)/apps/cryptovol-gui" && npm run --silent tauri -- --version 2>/dev/null)" || tauri_cli_version="unknown"

  local commit
  commit="$(git -C "$(project_root)" rev-parse --short HEAD 2>/dev/null)" || commit="unknown"

  {
    echo "Product name: Crypto Volume Viewer"
    echo "Version: $(package_version)"
    echo "Bundle identifier: com.flgnr.cryptovol"
    echo "Git commit: ${commit}"
    echo "Build timestamp (UTC): $(date -u +%Y-%m-%dT%H:%M:%SZ)"
    echo "Rust version: $(rustc --version)"
    echo "Node version: $(node --version)"
    echo "npm version: $(npm --version)"
    echo "Tauri CLI version: ${tauri_cli_version}"
    echo "Target platform: $(uname -sm)"
    echo "Signing mode: ${signing_mode}"
    echo "Artifacts:"
    for artifact in "${artifacts[@]}"; do
      echo "  - ${artifact}"
    done
  } > "$(dist_dir)/build-info.txt"
}
