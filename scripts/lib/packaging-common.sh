#!/usr/bin/env bash
# Shared helpers for the macOS packaging scripts (package-macos-local.sh,
# package-macos-release.sh, create-checksums.sh). Meant to be sourced, not
# executed directly.
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
