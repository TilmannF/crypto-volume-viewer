#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
GENERATED_DIR="$ROOT_DIR/testdata/generated/crypto-matrix"
RANDOM_SOURCE="$ROOT_DIR/testdata/generated/crypto-matrix-random.bin"
ENSURE_VERACRYPT="$ROOT_DIR/scripts/ensure-veracrypt-cli.sh"
CRYPTOVOL="$ROOT_DIR/target/release/cryptovol"

# VeraCrypt --hash argument values for each supported profile
HASH_NAMES=("sha-512" "sha-256" "whirlpool" "blake2s-256" "streebog")

# PIM values to test: 0 = VeraCrypt default, 500 = custom
PIM_VALUES=("0" "500")

passed=0
skipped=0
failed=0
any_failure=0

build_cryptovol() {
  if [[ ! -x "$CRYPTOVOL" ]]; then
    printf '==> Building cryptovol (release)...\n'
    (cd "$ROOT_DIR" && cargo build --release --quiet)
  fi
}

# Run cryptovol test-open using expect to feed the password through a pty.
# This works around rpassword reading from /dev/tty rather than piped stdin.
# Usage: open_with_password <container> <password> [extra-args...]
# Returns: 0 if cryptovol exited 0, non-zero otherwise.
open_with_password() {
  local container="$1"
  local password="$2"
  shift 2

  # Build a space-separated extra-args string safe for use in [list ...] below.
  # Our extra args are always simple tokens like "--pim" and "500".
  local extra=""
  for arg in "$@"; do
    extra="$extra $arg"
  done

  # Env vars carry container path and password into the Tcl script without
  # risking injection through shell metacharacters in the -c "..." string.
  OPEN_CRYPTOVOL="$CRYPTOVOL" \
  OPEN_CONTAINER="$container" \
  OPEN_PASSWORD="$password" \
  expect -c "
    set args [list \$env(OPEN_CRYPTOVOL) test-open \$env(OPEN_CONTAINER)]
    foreach arg [list$extra] { lappend args \$arg }
    spawn {*}\$args
    expect {Password:}
    send \"\$env(OPEN_PASSWORD)\r\"
    expect eof
    lassign [wait] pid spawnid os_error_flag value
    exit \$value
  " >/dev/null 2>&1
}

# Map VeraCrypt --hash name to cryptovol --kdf hint.
# This is used for rejection checks so cryptovol tries exactly one KDF
# instead of autoprobing all five (which takes ~5x longer per rejection).
kdf_hint_for_hash() {
  case "$1" in
    sha-512)     echo "sha512" ;;
    sha-256)     echo "sha256" ;;
    whirlpool)   echo "whirlpool" ;;
    blake2s-256) echo "blake2s" ;;
    streebog)    echo "streebog" ;;
    *)           echo "" ;;
  esac
}

check_profile() {
  local veracrypt_bin="$1"
  local hash="$2"
  local pim="$3"
  local container="$GENERATED_DIR/crypto-matrix-${hash}-pim${pim}.hc"
  local label="${hash}/pim${pim}"
  local kdf_hint
  kdf_hint="$(kdf_hint_for_hash "$hash")"
  local profile_ok=1

  printf '\n--- %s ---\n' "$label"

  # Attempt container creation; skip if VeraCrypt doesn't support this profile.
  if ! "$veracrypt_bin" \
      --text \
      --non-interactive \
      --create "$container" \
      --volume-type normal \
      --size 20M \
      --encryption AES \
      --hash "$hash" \
      --filesystem FAT \
      --password "test-password" \
      --pim "$pim" \
      --keyfiles "" \
      --random-source "$RANDOM_SOURCE" 2>/dev/null; then
    printf 'SKIP: %s — not supported by this VeraCrypt build\n' "$label"
    skipped=$((skipped + 1))
    return
  fi
  printf '  container created: %s\n' "$(basename "$container")"

  if [[ "$pim" == "0" ]]; then
    # Default-PIM container: open without --pim flag
    if open_with_password "$container" "test-password"; then
      printf '  PASS: correct password / default PIM opens\n'
    else
      printf '  FAIL: correct password / default PIM did not open\n'
      profile_ok=0
    fi

    # Wrong variant: --pim 500 must be rejected.
    # Pass --kdf hint so cryptovol tries one KDF only, not all five.
    if open_with_password "$container" "test-password" --kdf "$kdf_hint" --pim 500; then
      printf '  FAIL: --pim 500 accepted on default-PIM container\n'
      profile_ok=0
    else
      printf '  PASS: --pim 500 rejected on default-PIM container\n'
    fi
  else
    # Custom-PIM container: open with --pim <value>
    if open_with_password "$container" "test-password" --pim "$pim"; then
      printf '  PASS: correct password / --pim %s opens\n' "$pim"
    else
      printf '  FAIL: correct password / --pim %s did not open\n' "$pim"
      profile_ok=0
    fi

    # Wrong variant: default PIM (no --pim) must be rejected.
    # Pass --kdf hint so cryptovol tries one KDF only, not all five.
    if open_with_password "$container" "test-password" --kdf "$kdf_hint"; then
      printf '  FAIL: default PIM accepted on pim=%s container\n' "$pim"
      profile_ok=0
    else
      printf '  PASS: default PIM rejected on pim=%s container\n' "$pim"
    fi
  fi

  # Wrong password must always be rejected regardless of PIM.
  # Pass --kdf hint so cryptovol tries one KDF only, not all five.
  if [[ "$pim" == "0" ]]; then
    if open_with_password "$container" "wrong-password" --kdf "$kdf_hint"; then
      printf '  FAIL: wrong password accepted\n'
      profile_ok=0
    else
      printf '  PASS: wrong password rejected\n'
    fi
  else
    if open_with_password "$container" "wrong-password" --kdf "$kdf_hint" --pim "$pim"; then
      printf '  FAIL: wrong password accepted\n'
      profile_ok=0
    else
      printf '  PASS: wrong password rejected\n'
    fi
  fi

  if [[ "$profile_ok" -eq 1 ]]; then
    printf '  OK: %s\n' "$label"
    passed=$((passed + 1))
  else
    printf '  FAILED: %s\n' "$label"
    failed=$((failed + 1))
    any_failure=1
  fi
}

main() {
  local veracrypt_bin
  veracrypt_bin="$("$ENSURE_VERACRYPT")"

  build_cryptovol

  rm -rf "$GENERATED_DIR"
  mkdir -p "$GENERATED_DIR"

  printf '==> Generating random source...\n'
  dd if=/dev/urandom of="$RANDOM_SOURCE" bs=1024 count=1024 status=none

  local hash pim
  for hash in "${HASH_NAMES[@]}"; do
    for pim in "${PIM_VALUES[@]}"; do
      check_profile "$veracrypt_bin" "$hash" "$pim"
    done
  done

  printf '\n=== Crypto Matrix Results ===\n'
  printf '  Passed:  %d\n' "$passed"
  printf '  Skipped: %d\n' "$skipped"
  printf '  Failed:  %d\n' "$failed"

  if [[ "$any_failure" -eq 1 ]]; then
    printf 'RESULT: FAILED\n'
    exit 1
  fi
  printf 'RESULT: PASSED\n'
}

main "$@"
