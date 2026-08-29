# FAT Directory Listing

This document describes the FAT filesystem support implemented in `cryptovol-fs-fat`.

## Supported Scope

* **FAT type detection**: FAT12, FAT16, FAT32 (by cluster count per Microsoft spec).
* **Entry format**: Long File Name (LFN) entries (attribute `0x0F`) are collected and decoded. Volume label entries (attribute `0x08`), deleted entries (`0xE5`), and end-of-directory sentinels (`0x00`) are skipped.
* **Names**: Long names are displayed when valid LFN sequences are present. 8.3 short names are the fallback when LFN is absent, checksum-mismatched, or orphaned.
* **Access mode**: read-only. No writes, no mounting, no FUSE, no kernel extensions.
* **Nesting**: root directory and one or more levels of subdirectory via cluster chain traversal.
* **Path handling**: case-insensitive for ASCII in long names; exact match required for non-ASCII characters. Leading and trailing `/` tolerated. `.` and `..` not supported.

## Long File Name (LFN) Support

`cryptovol ls` displays long filenames when valid LFN directory sequences are present in the FAT
directory. This includes names with spaces, Unicode characters, and emoji.

* Long names are preferred. When a valid LFN sequence with a matching checksum precedes a short
  entry, the long name is shown.
* 8.3 short names are used as fallback when no LFN is present, the LFN checksum mismatches, or the
  LFN sequence is orphaned or incomplete.
* `cryptovol ls` default output shows the best available name (long name preferred).
* `cryptovol ls --long` adds metadata columns: type character, file size, modification date/time,
  and name.
* Path lookup accepts long names including non-ASCII characters. For non-ASCII, the exact on-disk
  Unicode sequence must be supplied (no automatic normalization is applied).

See [fat-lfn-unicode-metadata.md](fat-lfn-unicode-metadata.md) for full LFN entry format details,
UTF-16LE decoding, Unicode normalization policy, and path lookup behaviour.

## Static Fixture

```text
Path:     testdata/static/tcvc-aes-sha512-fat-files.hc
Password: test-password
Profile:  TC/VC-compatible, AES-XTS, SHA-512 KDF, FAT filesystem
```

Expected root directory contents:

```text
-       36  HELLO.TXT
-    89489  SYDNEY.JPG
d        0  DIR
```

Expected `/DIR` contents:

```text
-       17  NESTED.TXT
```

The fixture is committed under `testdata/static/` and excluded from the `*.hc` wildcard via a negation pattern in `.gitignore`.

## Running Fixture Tests

Fixture tests are marked `#[ignore]` and require the environment variable `CRYPTOVOL_STATIC_FAT_FIXTURE` to point to the committed fixture file:

```bash
CRYPTOVOL_STATIC_FAT_FIXTURE=testdata/static/tcvc-aes-sha512-fat-files.hc \
  cargo test -- --ignored
```

Normal `cargo test` runs without the fixture and skips all `#[ignore]` tests.

## Limitations

* `..` and `.` directory traversal are not supported.
* exFAT is not supported.
* FAT12 cluster traversal is not tested with a real fixture.
* Non-ASCII long-name path lookup is case-sensitive (no full Windows Unicode casefolding).
* No write support, mounting, FUSE integration, or kernel extension is used.
* Broad compatibility with arbitrary TrueCrypt or VeraCrypt containers is not claimed; only the narrow `tcvc-aes-sha512-basic` profile with AES-XTS and SHA-512 KDF is supported.

## What `cryptovol` Does Not Do

* Does not mount the volume.
* Does not invoke VeraCrypt, TrueCrypt, or any third-party tool.
* Does not write to the container.
* Does not expose raw decrypted block data.

## File Extraction

File extraction is now available. See [docs/fat-file-extraction.md](fat-file-extraction.md) for the `cryptovol extract` command.
