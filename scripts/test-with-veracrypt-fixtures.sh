#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
GENERATED_DIR="$ROOT_DIR/testdata/generated"
CONTAINER_PATH="$GENERATED_DIR/tcvc-aes-sha512-basic.hc"
RANDOM_SOURCE="$GENERATED_DIR/random-source.bin"
FIXTURE_PATH_FOR_DOCS="testdata/generated/tcvc-aes-sha512-basic.hc"
ENSURE_VERACRYPT="$ROOT_DIR/scripts/ensure-veracrypt-cli.sh"

create_random_source() {
  dd if=/dev/urandom of="$RANDOM_SOURCE" bs=1024 count=1024 status=none
}

create_container() {
  local veracrypt_bin="$1"
  local filesystem="$2"

  "$veracrypt_bin" \
    --text \
    --non-interactive \
    --create "$CONTAINER_PATH" \
    --volume-type normal \
    --size 20M \
    --encryption AES \
    --hash SHA-512 \
    --filesystem "$filesystem" \
    --password "test-password" \
    --pim 0 \
    --keyfiles "" \
    --random-source "$RANDOM_SOURCE"
}

main() {
  local veracrypt_bin
  veracrypt_bin="$("$ENSURE_VERACRYPT")"

  rm -rf "$GENERATED_DIR"
  mkdir -p "$GENERATED_DIR"
  create_random_source

  create_container "$veracrypt_bin" FAT

  (
    cd "$ROOT_DIR"
    CRYPTOVOL_TEST_CONTAINER="$CONTAINER_PATH" cargo test -- --ignored
  )
}

main "$@"
