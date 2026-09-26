#!/usr/bin/env bash
# Tests the release-asset helpers in scripts/lib/packaging-common.sh:
# GitHub-safe asset names, single-DMG selection, and SHA256SUMS.txt checks,
# including the v0.1.0 case (SHA256SUMS.txt listed a name with spaces that
# GitHub had renamed on upload). Needs only bash and shasum; no network.
set -uo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib/packaging-common.sh
source "$script_dir/lib/packaging-common.sh"
set +e  # the helpers are expected to fail in the negative cases below

T="$(mktemp -d)"
trap 'rm -rf "$T"' EXIT
pass=0
fail=0
ok()  { if "$@" >/dev/null 2>&1; then pass=$((pass+1)); else fail=$((fail+1)); echo "FAIL (expected ok): $*"; fi; }
bad() { if "$@" >/dev/null 2>&1; then fail=$((fail+1)); echo "FAIL (expected error): $*"; else pass=$((pass+1)); fi; }
eq()  { if [[ "$1" == "$2" ]]; then pass=$((pass+1)); else fail=$((fail+1)); echo "FAIL: got '$1' want '$2'"; fi; }

eq "$(release_asset_name 'Crypto Volume Viewer_0.1.0_aarch64.dmg')" "Crypto.Volume.Viewer_0.1.0_aarch64.dmg"
eq "$(release_asset_name 'already.safe-name_1.dmg')" "already.safe-name_1.dmg"
ok  require_release_safe_name "Crypto.Volume.Viewer_0.1.0_aarch64.dmg"
ok  require_release_safe_name "SHA256SUMS.txt"
bad require_release_safe_name "Crypto Volume Viewer_0.1.0_aarch64.dmg"
bad require_release_safe_name ".hidden.dmg"
bad require_release_safe_name "trailing."
bad require_release_safe_name "umlaut-ä.dmg"
bad require_release_safe_name "a+b.dmg"
bad require_release_safe_name ""

# single_dmg_in
mkdir -p "$T/none" "$T/one" "$T/two"
bad single_dmg_in "$T/none"
touch "$T/one/A.dmg"; eq "$(single_dmg_in "$T/one")" "$T/one/A.dmg"
touch "$T/two/A.dmg" "$T/two/B.dmg"; bad single_dmg_in "$T/two"

mk() { # dir, file-name-on-disk, name-in-sums
  mkdir -p "$1"; printf 'dmg-bytes' > "$1/$2"
  (cd "$1" && printf '%s  %s\n' "$(shasum -a 256 "$2" | cut -d' ' -f1)" "$3" > SHA256SUMS.txt)
}
# v0.1.0 as published: sums list the spaced name, download has the dotted name
mk "$T/v010" "Crypto.Volume.Viewer_0.1.0_aarch64.dmg" "Crypto Volume Viewer_0.1.0_aarch64.dmg"
bad verify_release_dir "$T/v010"
# fixed layout
mk "$T/fixed" "Crypto.Volume.Viewer_0.1.0_aarch64.dmg" "Crypto.Volume.Viewer_0.1.0_aarch64.dmg"
ok  verify_release_dir "$T/fixed"
eq "$(check_sums_file "$T/fixed" 2>/dev/null)" "Crypto.Volume.Viewer_0.1.0_aarch64.dmg"
# hash mismatch
mk "$T/tampered" "X.dmg" "X.dmg"; printf 'other' > "$T/tampered/X.dmg"
bad verify_release_dir "$T/tampered"
# unlisted extra asset in a download dir
mk "$T/extra" "X.dmg" "X.dmg"; touch "$T/extra/unlisted.zip"
bad verify_release_dir "$T/extra"
# dist dir: extra non-release files are fine for check_sums_file
mk "$T/dist" "X.dmg" "X.dmg"; touch "$T/dist/build-info.txt"; mkdir "$T/dist/X.app"
ok  check_sums_file "$T/dist"
# missing/empty/malformed sums
mkdir -p "$T/nosums"; bad check_sums_file "$T/nosums"
mkdir -p "$T/empty"; : > "$T/empty/SHA256SUMS.txt"; bad check_sums_file "$T/empty"
mkdir -p "$T/malformed"; echo "not a hash line" > "$T/malformed/SHA256SUMS.txt"; bad check_sums_file "$T/malformed"
# binary-mode line ("<hash> *name") is accepted
mkdir -p "$T/binmode"; printf 'x' > "$T/binmode/X.dmg"; (cd "$T/binmode" && shasum -a 256 -b X.dmg > SHA256SUMS.txt); ok check_sums_file "$T/binmode"

echo "passed=$pass failed=$fail"
[[ $fail -eq 0 ]]
