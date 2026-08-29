# FAT File Extraction

This document describes single-file extraction support in `cryptovol-fs-fat` and the `cryptovol extract` CLI command.

## Supported Scope

* **Container format**: TC/VC-compatible file-hosted container, AES-XTS encryption, SHA-512 KDF.
* **FAT variant**: FAT16 (confirmed for the static fixture; FAT32 supported by the same code path; FAT12 not exercised with a real fixture).
* **Entry format**: Long File Name (LFN) entries are decoded. Long-name source paths (including names with spaces, Unicode characters, and emoji) are accepted.
* **Access mode**: read-only. No writes, no mounting, no FUSE, no kernel extensions.
* **Extraction granularity**: single file only. Directory extraction is not implemented.

## Long-Name Source Paths

`cryptovol extract` now accepts long-name source paths. Files with names containing spaces,
accented characters, or emoji can be extracted using the long name visible in `cryptovol ls`:

```bash
cryptovol extract backup.hc "/Emoji Rocket 🚀 Test.txt" ./rocket.txt
cryptovol extract backup.hc "/Project Notes Final.txt" ./notes.txt
```

Path lookup checks the long name first, then the 8.3 short name as fallback. For non-ASCII
characters the path must match the exact on-disk Unicode sequence — no automatic Unicode
normalization is applied. See [fat-lfn-unicode-metadata.md](fat-lfn-unicode-metadata.md) for
details on path lookup and the Unicode normalization policy.

8.3 short names are still accepted for backwards compatibility:

```bash
cryptovol extract backup.hc /PROJEC~1.TXT ./notes.txt
```

## No Directory Extraction

`cryptovol extract` targets a single file path. Passing a path that resolves to a directory returns an error:

```
error: /DIR is a directory; directory extraction is not supported
```

Use `cryptovol ls` to list directory contents and extract files individually.

## `--overwrite` Behavior

By default, `cryptovol extract` refuses to write to an existing destination file:

```bash
cryptovol extract backup.hc /HELLO.TXT ./hello.txt
# error if ./hello.txt already exists
```

Pass `--overwrite` to allow replacement:

```bash
cryptovol extract backup.hc /HELLO.TXT ./hello.txt --overwrite
```

## `--parents` Behavior

By default, the parent directory of the destination path must already exist. If it does not, `cryptovol extract` returns an error.

Pass `--parents` to create missing parent directories automatically:

```bash
cryptovol extract backup.hc /DIR/NESTED.TXT ./out/dir/nested.txt --parents
```

## Streaming Extraction

Cluster data is streamed cluster-by-cluster using a fixed 256 KiB read buffer. The full file is never buffered in RAM. A temp file is created in the destination directory, data is streamed into it, and on success it is atomically renamed to the final path. On failure the temp file is deleted automatically.

See [streaming-extraction.md](streaming-extraction.md) for the complete memory model and design rationale.

## Destination Safety

* Destination is a host filesystem path only. The encrypted container is never modified.
* Extraction refuses to overwrite by default; use `--overwrite` explicitly.
* If the destination path exists and is a directory, extraction fails even with `--overwrite`.
* Parent directories are not created unless `--parents` is passed.
* A temp file is written in the destination directory during extraction; it is atomically renamed on success and removed on failure. No partial decrypted content appears at the final path.

## Static Fixture

```text
Path:         testdata/static/tcvc-aes-sha512-fat-files.hc
Password:     test-password
Profile:      TC/VC-compatible, AES-XTS, SHA-512 KDF, FAT16 filesystem
Ground truth: testdata/static/fs-fat-original/
```

Ground-truth originals used for byte-for-byte verification:

| Container path  | Original file                              | Size      |
|-----------------|--------------------------------------------|-----------|
| `/HELLO.TXT`    | `testdata/static/fs-fat-original/HELLO.TXT`    | 36 bytes  |
| `/SYDNEY.JPG`   | `testdata/static/fs-fat-original/SYDNEY.JPG`   | 89,489 bytes |
| `/DIR/NESTED.TXT` | `testdata/static/fs-fat-original/DIR/NESTED.TXT` | 17 bytes  |

## Running Fixture Tests

Extraction fixture tests are marked `#[ignore]` in `crates/cryptovol-fs-fat/tests/fixture.rs`. Run them with:

```bash
CRYPTOVOL_STATIC_FAT_FIXTURE=testdata/static/tcvc-aes-sha512-fat-files.hc \
  cargo test -- --ignored
```

Covered scenarios:

* `fixture_extract_hello_txt` — extracts `/HELLO.TXT` and compares bytes to ground truth
* `fixture_extract_nested_txt` — extracts `/DIR/NESTED.TXT` and compares bytes to ground truth
* `fixture_extract_sydney_jpg` — extracts `/SYDNEY.JPG` and compares bytes to ground truth
* `fixture_extract_dir_rejected` — confirms `/DIR` returns `IsADirectory` error
* `fixture_extract_unknown_path` — confirms `/DOESNOTEXIST.TXT` returns `PathNotFound` error

Normal `cargo test` runs without the environment variable and skips all `#[ignore]` tests.

## What `cryptovol extract` Does Not Do

* Does not mount the container.
* Does not invoke VeraCrypt, TrueCrypt, or any third-party tool.
* Does not write to the encrypted container.
* Does not extract directories (single-file extraction only).
* Does not preserve metadata (timestamps, permissions) in this release.
* Does not apply Unicode normalization to source paths; callers must supply the exact on-disk form.
* Does not support keyfiles, hidden volumes, non-default PIM, or encryption algorithms other than AES-XTS with SHA-512 KDF.
* Does not support exFAT.
