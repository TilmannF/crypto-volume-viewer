# Test Containers

Generated fixtures prove password-based header opening, decrypted data block reading, and first-sector filesystem probing via `probe-fs`. The committed static fixtures additionally prove FAT, exFAT, and NTFS directory listing, metadata, single-file extraction, LFN/Unicode paths, and multiple KDF/PIM combinations. Neither fixture category proves mounting, broad TrueCrypt/VeraCrypt compatibility, keyfiles, hidden volumes, or directory extraction. See [tcvc-data-reader.md](tcvc-data-reader.md) for logical offset mapping and decryption details. See [tcvc-kdf-pim-compatibility.md](tcvc-kdf-pim-compatibility.md) for KDF and PIM documentation. The desktop GUI's manual smoke test (see [gui-mvp.md](gui-mvp.md)) and its automated Tauri E2E suite (`apps/cryptovol-gui/e2e/specs/fixtures.e2e.ts` -- see [gui-testing.md](gui-testing.md)) both reuse these same static fixtures under `testdata/static/` rather than introducing GUI-only test containers.

Generated encrypted containers stay out of Git by default. Static committed fixtures are excluded from the wildcard rules via negation patterns. Fixture extraction tests (marked `#[ignore]`) exercise both `read_file` and `read_file_to_writer` to verify that streaming output matches the buffered path byte-for-byte. The repository ignores:

```gitignore
testdata/generated/
.examples/
*.hc
!testdata/static/*.hc
*.tc
!testdata/static/*.tc
```

Only commit an encrypted fixture if a future task explicitly approves it, documents why it is safe, and confirms it contains no private data.

## Static Fixture: tcvc-aes-sha512-fat-files

Purpose: FAT directory listing and fixture workflow integration tests. This fixture is committed at `testdata/static/tcvc-aes-sha512-fat-files.hc` and is safe to commit because it contains only public test data with a documented test password.

```text
Container type: file-hosted TC/VC-compatible normal container
Path: testdata/static/tcvc-aes-sha512-fat-files.hc
Size: 20 MiB
Password: test-password
PIM: default / 0
Keyfiles: none
Hidden volume: none
Encryption: AES
Mode: XTS
Hash/KDF: SHA-512
Filesystem: FAT
```

Expected root directory contents:

```text
HELLO.TXT   (file, size=36)
SYDNEY.JPG  (file, size=89489)
DIR         (directory)
```

Expected `/DIR` contents:

```text
NESTED.TXT  (file, size=17)
```

To run fixture tests:

```bash
CRYPTOVOL_STATIC_FAT_FIXTURE=testdata/static/tcvc-aes-sha512-fat-files.hc \
  cargo test -- --ignored
```

`testdata/static/fs-fat-original/` contains the ground-truth originals used to verify extraction byte-for-byte: `HELLO.TXT` (36 bytes), `SYDNEY.JPG` (89,489 bytes), `DIR/NESTED.TXT` (17 bytes). Fixture tests in `crates/cryptovol-fs-fat/tests/fixture.rs` compare extracted bytes to these originals.

Size approval: this 20 MiB fixture is intentionally committed for now because it is the smallest currently available static encrypted container that exercises the full supported path: TC/VC AES-XTS SHA-512 opening, decrypted FAT short-name directory listing, and byte-for-byte extraction against public originals. Password `test-password` and all contained files are public test data. The fixture was generated for this repository from public synthetic contents, so there is no copied third-party source or private data. Replace it with a smaller generated or committed fixture once the project has a documented process that can produce an equivalent smaller TC/VC-compatible FAT container reproducibly.

Commit policy: this fixture is intentionally committed under `testdata/static/`. It contains no private data. The `.gitignore` negation pattern `!testdata/static/*.hc` ensures it is not suppressed by the wildcard rule.

## Static Fixture: tcvc-aes-sha512-fat-lfn-unicode

Purpose: FAT Long File Name (LFN) and Unicode support integration tests. This fixture is committed
at `testdata/static/tcvc-aes-sha512-fat-lfn-unicode.hc` and contains only public synthetic test
data with a documented test password.

```text
Container type: file-hosted TC/VC-compatible normal container
Path:           testdata/static/tcvc-aes-sha512-fat-lfn-unicode.hc
Password:       test-password
PIM:            default / 0
Keyfiles:       none
Hidden volume:  none
Encryption:     AES
Mode:           XTS
Hash/KDF:       SHA-512
Filesystem:     FAT with Long File Name (LFN) entries
```

Expected root directory contents:

| File | Size (bytes) | SHA-256 |
|------|-------------|---------|
| `Emoji Rocket 🚀 Test.txt` | 22 | `99d1c67af30cc495fe28836483c8fb86a41bea4a8510d9a39be4f3ae4075bd0e` |
| `Please Do Not Open 😅.txt` | 23 | `ede269a0be0c6d600305a67e44b1fab1fa2c6741ab0f35856d4200afc486a9bc` |
| `Project Notes Final.txt` | 49 | `3497b7fb6e5d93370e0212b439bb4516cbf7b03180ae33ac06401d6ba063463d` |
| `Sydney Sweeney at the 2025 Toronto International Film Festival.jpg` | 89489 | `e2ee40fdb8cb5dcff4a2870f7d773cb1979054bd55ee00245ac151251edd48d3` |
| `Unicode Umlaut äöü ÄÖÜ ß.txt` | 39 | `6cb589b1df71ebdb4ae405c1d33e806e2e23e2bc78e15c4a33835a12b905c87a` |
| `Folder With Spaces/Rocket Science 🚀 For Beginners.txt` | 43 | `7a05f383d6f29a0456153fd0a4f6dbcba2ce48081d7aa4a8d410c2a644a79034` |

**Unicode normalization note:** The `Unicode Umlaut` filename is stored in **decomposed form** on
disk. Each umlaut (e.g. `ä`) is encoded as the base letter `a` (U+0061) followed by
`U+0308 COMBINING DIAERESIS`. The NFC-precomposed form `ä` (U+00E4) is a different code-unit
sequence. Path lookup must use the exact on-disk decomposed form. The display may look identical
depending on the terminal, but the byte sequences differ.

To run LFN fixture tests:

```bash
CRYPTOVOL_STATIC_FAT_LFN_FIXTURE=testdata/static/tcvc-aes-sha512-fat-lfn-unicode.hc \
  cargo test --test lfn_fixture -- --ignored
```

Normal `cargo test` skips all `#[ignore]` tests and does not require the fixture.

Commit policy: static fixture files in `testdata/static/` are intentionally tracked in Git. The
`.gitignore` negation pattern `!testdata/static/*.hc` ensures they are not suppressed by the
wildcard rule. Generated containers in `testdata/generated/` are not committed.

## Static Fixtures: Crypto-Matrix (SHA-256 and PIM-500)

Two additional static fixtures cover different KDF/PIM combinations. Both contain the same file tree as `testdata/static/fs-fat-lfn-original/` and use the same `test-password`.

```text
.gitignore negation: !testdata/static/crypto-matrix/*.hc
```

### `testdata/static/crypto-matrix/tcvc-aes-sha256-pim-default-fat-lfn-unicode.hc`

```text
Container type: file-hosted TC/VC-compatible normal container
Path:           testdata/static/crypto-matrix/tcvc-aes-sha256-pim-default-fat-lfn-unicode.hc
Password:       test-password
PIM:            default / 0
Keyfiles:       none
Hidden volume:  none
Encryption:     AES
Mode:           XTS
Hash/KDF:       SHA-256
Filesystem:     FAT with Long File Name (LFN) entries
Contents:       same file tree as testdata/static/fs-fat-lfn-original/
```

### `testdata/static/crypto-matrix/tcvc-aes-sha512-pim-500-fat-lfn-unicode.hc`

```text
Container type: file-hosted TC/VC-compatible normal container
Path:           testdata/static/crypto-matrix/tcvc-aes-sha512-pim-500-fat-lfn-unicode.hc
Password:       test-password
PIM:            500  (515,000 PBKDF2 iterations: 15,000 + 500×1,000)
Keyfiles:       none
Hidden volume:  none
Encryption:     AES
Mode:           XTS
Hash/KDF:       SHA-512
Filesystem:     FAT with Long File Name (LFN) entries
Contents:       same file tree as testdata/static/fs-fat-lfn-original/
```

### Running crypto-matrix static fixture tests

```bash
CRYPTOVOL_STATIC_CRYPTO_MATRIX_DIR=$(pwd)/testdata/static/crypto-matrix \
  cargo test -- --ignored
```

The path must be absolute (or resolved with `$(pwd)`). Tests use `testdata/static/fs-fat-lfn-original/` as ground truth for file content verification.

## Static Fixture: tcvc-aes-sha512-exfat-lfn-unicode

Purpose: exFAT directory listing and single-file extraction integration tests. This fixture is committed at `testdata/static/tcvc-aes-sha512-exfat-lfn-unicode.hc` and contains only public synthetic test data with a documented test password.

```text
Container type: file-hosted TC/VC-compatible normal container
Path:           testdata/static/tcvc-aes-sha512-exfat-lfn-unicode.hc
Password:       test-password
PIM:            default / 0
Keyfiles:       none
Hidden volume:  none
Encryption:     AES
Mode:           XTS
Hash/KDF:       SHA-512
Filesystem:     exFAT
```

Expected root directory contents:

| File | Size (bytes) | SHA-256 |
|------|-------------|---------|
| `Emoji Rocket 🚀 Test.txt` | 22 | `99d1c67af30cc495fe28836483c8fb86a41bea4a8510d9a39be4f3ae4075bd0e` |
| `Please Do Not Open 😅.txt` | 23 | `ede269a0be0c6d600305a67e44b1fab1fa2c6741ab0f35856d4200afc486a9bc` |
| `Project Notes Final.txt` | 49 | `3497b7fb6e5d93370e0212b439bb4516cbf7b03180ae33ac06401d6ba063463d` |
| `Sydney Sweeney at the 2025 Toronto International Film Festival.jpg` | 89489 | `e2ee40fdb8cb5dcff4a2870f7d773cb1979054bd55ee00245ac151251edd48d3` |
| `Unicode Umlaut äöü ÄÖÜ ß.txt` | 39 | `6cb589b1df71ebdb4ae405c1d33e806e2e23e2bc78e15c4a33835a12b905c87a` |
| `Folder With Spaces/` | (directory) | — |

Expected `Folder With Spaces/` contents:

| File | Size (bytes) | SHA-256 |
|------|-------------|---------|
| `Rocket Science 🚀 For Beginners.txt` | 43 | `7a05f383d6f29a0456153fd0a4f6dbcba2ce48081d7aa4a8d410c2a644a79034` |

Ground truth originals for extraction verification: `testdata/static/fs-fat-lfn-original/`.

**Unicode normalization note:** The `Unicode Umlaut` filename is stored in **decomposed form** on disk (same as the FAT LFN fixture). Each umlaut is encoded as the base letter followed by `U+0308 COMBINING DIAERESIS`. Path lookup must use the exact on-disk decomposed form; the NFC-precomposed form (e.g. U+00E4 for `ä`) is a different byte sequence.

To run exFAT fixture tests:

```bash
CRYPTOVOL_STATIC_EXFAT_LFN_FIXTURE=$(pwd)/testdata/static/tcvc-aes-sha512-exfat-lfn-unicode.hc \
  cargo test -p cryptovol-fs-exfat -- --ignored
```

The path must be absolute (or resolved with `$(pwd)`). Normal `cargo test` skips all `#[ignore]` tests and does not require the fixture.

Commit policy: static fixture files in `testdata/static/` are intentionally tracked in Git. The `.gitignore` negation pattern `!testdata/static/*.hc` ensures they are not suppressed by the wildcard rule. Generated containers in `testdata/generated/` are not committed.

## Static Fixture: tcvc-aes-sha512-ntfs-lfn-unicode

Purpose: NTFS directory listing and single-file extraction integration tests. This fixture is
committed at `testdata/static/tcvc-aes-sha512-ntfs-lfn-unicode.hc` and contains only public
synthetic test data with a documented test password.

```text
Container type: file-hosted TC/VC-compatible normal container
Path:           testdata/static/tcvc-aes-sha512-ntfs-lfn-unicode.hc
Size:           20 MiB
Password:       test-password
PIM:            default / 0
Keyfiles:       none
Hidden volume:  none
Encryption:     AES
Mode:           XTS
Hash/KDF:       SHA-512
Filesystem:     NTFS
```

Expected root directory contents:

| File | Size (bytes) | SHA-256 |
|------|-------------|---------|
| `Emoji Rocket 🚀 Test.txt` | 22 | `99d1c67af30cc495fe28836483c8fb86a41bea4a8510d9a39be4f3ae4075bd0e` |
| `Please Do Not Open 😅.txt` | 23 | `ede269a0be0c6d600305a67e44b1fab1fa2c6741ab0f35856d4200afc486a9bc` |
| `Project Notes Final.txt` | 49 | `3497b7fb6e5d93370e0212b439bb4516cbf7b03180ae33ac06401d6ba063463d` |
| `Sydney Sweeney at the 2025 Toronto International Film Festival.jpg` | 89489 | `e2ee40fdb8cb5dcff4a2870f7d773cb1979054bd55ee00245ac151251edd48d3` |
| `Unicode Umlaut äöü ÄÖÜ ß.txt` | 39 | `6cb589b1df71ebdb4ae405c1d33e806e2e23e2bc78e15c4a33835a12b905c87a` |
| `Folder With Spaces/` | (directory) | — |

Expected `Folder With Spaces/` contents:

| File | Size (bytes) | SHA-256 |
|------|-------------|---------|
| `Rocket Science 🚀 For Beginners.txt` | 43 | `7a05f383d6f29a0456153fd0a4f6dbcba2ce48081d7aa4a8d410c2a644a79034` |

Ground truth originals for extraction verification: `testdata/static/fs-fat-lfn-original/`.

**Unicode normalization note:** The `Unicode Umlaut` filename is stored in decomposed form on disk
(same as the FAT and exFAT LFN fixtures). Path lookup must use the exact on-disk decomposed form;
the NFC-precomposed form is a different byte sequence.

To run NTFS fixture tests:

```bash
CRYPTOVOL_STATIC_NTFS_LFN_FIXTURE=$(pwd)/testdata/static/tcvc-aes-sha512-ntfs-lfn-unicode.hc \
  cargo test -p cryptovol-fs-ntfs ntfs_fixture -- --ignored
```

The path must be absolute (or resolved with `$(pwd)`). Normal `cargo test` skips all `#[ignore]`
tests and does not require the fixture.

Commit policy: static fixture files in `testdata/static/` are intentionally tracked in Git. The
`.gitignore` negation pattern `!testdata/static/*.hc` ensures they are not suppressed by the
wildcard rule. Generated containers in `testdata/generated/` are not committed.

## cryptovol-app Fixture Tests

`crates/cryptovol-app/tests/` has its own env-gated fixture tests (also `#[ignore]`d by default) that exercise `VolumeSession` end to end against the same static LFN fixtures documented above. No new fixture files or environment variables are introduced; both files reuse `CRYPTOVOL_STATIC_FAT_LFN_FIXTURE`, `CRYPTOVOL_STATIC_EXFAT_LFN_FIXTURE`, and `CRYPTOVOL_STATIC_NTFS_LFN_FIXTURE`.

* `list_dir_stat_fixtures.rs` — verifies `VolumeSession::list_dir` and `VolumeSession::stat` return the documented root directory contents and metadata across FAT, exFAT, and NTFS.
* `extract_file_fixtures.rs` — verifies `VolumeSession::extract_file` extracts a known file byte-for-byte with a leading `Started`/trailing `Finished` `ProgressEvent` sequence across FAT, exFAT, and NTFS, and that a mid-extraction `CancellationToken::cancel()` stops the copy and leaves no destination file behind.

```bash
CRYPTOVOL_STATIC_FAT_LFN_FIXTURE=$(pwd)/testdata/static/tcvc-aes-sha512-fat-lfn-unicode.hc \
CRYPTOVOL_STATIC_EXFAT_LFN_FIXTURE=$(pwd)/testdata/static/tcvc-aes-sha512-exfat-lfn-unicode.hc \
CRYPTOVOL_STATIC_NTFS_LFN_FIXTURE=$(pwd)/testdata/static/tcvc-aes-sha512-ntfs-lfn-unicode.hc \
  cargo test -p cryptovol-app -- --ignored
```

Normal `cargo test` skips all `#[ignore]` tests and does not require the fixtures.

## Fixture Script

Use the fixture script when real generated-container behavior must be proven:

```bash
./scripts/test-with-veracrypt-fixtures.sh
```

The script:

* resolves `veracrypt` from `PATH` or the common macOS app bundle path
* installs VeraCrypt on macOS with `brew install --cask veracrypt` when missing and Homebrew is available
* removes and recreates only `testdata/generated/`
* generates `testdata/generated/tcvc-aes-sha512-basic.hc` with a FAT filesystem
* runs ignored tests with `CRYPTOVOL_TEST_CONTAINER` set

Normal `cargo test` must not require VeraCrypt or generated containers.

## Profile: tcvc-aes-sha512-basic

Purpose: current generated happy-path fixture for opening one simple file-hosted TC/VC-compatible volume and probing the first decrypted sector. Generated fixtures are not committed and do not currently serve as the source of truth for listing or extraction tests; the committed static fixture covers that behavior.

```text
Container type: file-hosted TC/VC-compatible normal container
Size: 20 MiB
Password: test-password
Wrong test password: wrong-test-password
PIM: default / 0
Keyfiles: none
Hidden volume: none
Encryption: AES
Mode: XTS
Hash/KDF: SHA-512
Filesystem: FAT
```

Expected behavior:

```text
open with test-password => succeeds and returns only safe non-secret metadata
open with wrong-test-password => fails with authentication failed or unsupported parameters
first decrypted sector => readable and conservatively detected as FAT-like
directory listing and extraction => covered by the committed static fixture, not by this generated fixture profile
```

Commit policy: generated containers for this profile stay in `testdata/generated/` and are not committed.

## Future Profile: tcvc-aes-fat32-basic

Purpose: future happy-path fixture for opening and listing a simple file-hosted TC/VC-compatible container.

```text
Container type: file-hosted TC/VC-compatible container
Size: 10-20 MiB
Password: test-password
PIM: default
Keyfiles: none
Encryption: AES
Hash/KDF: VeraCrypt default or SHA-512
Filesystem: FAT32 or exFAT
```

Expected contents:

```text
/hello.txt       => "Hello from cryptovol test container.\n"
/dir/nested.txt  => "Nested test file.\n"
/binary.bin      => deterministic binary bytes
```

Commit policy: generated containers for this profile stay in `testdata/generated/` or `.examples/` and are not committed by default.

## Future Profile: tcvc-aes-fat32-wrong-password

Purpose: future authentication-failure fixture once password-based opening exists.

```text
Container type: file-hosted TC/VC-compatible container
Size: 10-20 MiB
Correct password: test-password
Wrong test password: wrong-test-password
PIM: default
Keyfiles: none
Encryption: AES
Hash/KDF: same as tcvc-aes-fat32-basic
Filesystem: FAT32 or exFAT
Expected behavior: opening with the wrong password fails without revealing secrets or exact internal trial details
```

Commit policy: generated containers for this profile stay out of Git by default.

## Future Profile: tcvc-unsupported-parameters

Purpose: future unsupported-parameter handling fixture once decryption and parameter detection exist.

```text
Container type: file-hosted TC/VC-compatible container
Size: 10-20 MiB
Password: test-password
PIM: default unless the unsupported parameter requires otherwise
Keyfiles: none unless keyfile handling is explicitly being tested later
Encryption: a TC/VC-compatible option outside the supported MVP set
Hash/KDF: a TC/VC-compatible option outside the supported MVP set, if practical
Filesystem: FAT32 or exFAT
Expected behavior: the CLI reports unsupported parameters clearly without claiming the password is wrong
```

Commit policy: generated containers for this profile stay out of Git by default.

The committed fixtures under `testdata/static/` are large (on the order of **120 MB**). A full git clone downloads them. That is intentional so listing/extraction tests are reproducible without generating containers. Generated extra containers still belong in `testdata/generated/` and stay gitignored.

## Test Data Attribution

The static committed fixtures under `testdata/static/` include one public-domain / openly licensed photographic image used solely as realistic binary payload for exercising FAT LFN/Unicode paths, metadata, and single-file extraction:

- `Sydney Sweeney at the 2025 Toronto International Film Festival.jpg` (89,489 bytes, appears in the LFN Unicode FAT fixture and as `SYDNEY.JPG` in the basic FAT files fixture)
- Source: https://commons.wikimedia.org/wiki/File:SydneySweeney-TIFF2025-01-Cropped.png (cropped TIFF 2025 version; converted/embedded as JPEG inside the synthetic test containers)

This asset is used **only** as test data inside the project's own committed `.hc` fixture files and the corresponding ground-truth originals in `testdata/static/fs-fat-original/`. It is never shipped inside the Crypto Volume Viewer application binary or presented to end users as application content.

Full credit and license details are on the Wikimedia Commons page linked above. The image is included here under the terms stated on that page for the purpose of reproducible open-source testing of an encrypted-volume inspection tool. No other third-party assets are embedded in the committed fixtures.
