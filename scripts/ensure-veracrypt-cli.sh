#!/usr/bin/env bash
set -euo pipefail

resolve_veracrypt() {
  if command -v veracrypt >/dev/null 2>&1; then
    command -v veracrypt
    return 0
  fi

  if [[ -x "/Applications/VeraCrypt.app/Contents/MacOS/VeraCrypt" ]]; then
    printf '%s\n' "/Applications/VeraCrypt.app/Contents/MacOS/VeraCrypt"
    return 0
  fi

  return 1
}

main() {
  if resolve_veracrypt; then
    return 0
  fi

  if [[ "$(uname -s)" == "Darwin" ]]; then
    if ! command -v brew >/dev/null 2>&1; then
      cat >&2 <<'MSG'
VeraCrypt is not installed and Homebrew was not found.
Install Homebrew or install VeraCrypt manually, then rerun this script.
MSG
      return 1
    fi

    brew install --cask veracrypt
    resolve_veracrypt
    return 0
  fi

  cat >&2 <<'MSG'
VeraCrypt is not installed.
Install the VeraCrypt CLI and ensure `veracrypt` is on PATH, then rerun this script.
MSG
  return 1
}

main "$@"
