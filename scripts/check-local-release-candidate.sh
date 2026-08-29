#!/usr/bin/env bash
# Runs the local Rust + GUI acceptance checks for a release candidate.
# Does not require Apple Developer credentials and does not package by
# default -- see scripts/package-macos-local.sh / package-macos-release.sh
# for that. Runs every check even if an earlier one fails, then prints a
# PASS/FAIL summary and exits non-zero if anything failed.
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib/packaging-common.sh
source "$script_dir/lib/packaging-common.sh"

names=()
results=()

run_check() {
  local name="$1"
  shift
  echo "==> ${name}"
  if "$@"; then
    names+=("$name")
    results+=("PASS")
  else
    names+=("$name")
    results+=("FAIL")
  fi
}

main() {
  local root
  root="$(project_root)"

  run_check "cargo fmt --all --check" bash -c "cd '$root' && cargo fmt --all --check"
  run_check "cargo clippy --workspace --all-targets --all-features -- -D warnings" \
    bash -c "cd '$root' && cargo clippy --workspace --all-targets --all-features -- -D warnings"
  run_check "cargo test --workspace --all-targets" bash -c "cd '$root' && cargo test --workspace --all-targets"
  run_check "cargo doc --workspace --no-deps" bash -c "cd '$root' && cargo doc --workspace --no-deps"

  run_check "npm run typecheck" bash -c "cd '$root/apps/cryptovol-gui' && npm run typecheck"
  run_check "npm run build" bash -c "cd '$root/apps/cryptovol-gui' && npm run build"
  run_check "npm test" bash -c "cd '$root/apps/cryptovol-gui' && npm test"
  run_check "npm run test:e2e" bash -c "cd '$root/apps/cryptovol-gui' && npm run test:e2e"

  echo
  echo "==> Summary"
  local overall=0
  for i in "${!names[@]}"; do
    printf '  %-70s %s\n' "${names[$i]}" "${results[$i]}"
    if [[ "${results[$i]}" == "FAIL" ]]; then
      overall=1
    fi
  done

  exit "$overall"
}

main "$@"
