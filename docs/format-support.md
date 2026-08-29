# Format Support

Support is intentionally narrow and experimental. No broad TrueCrypt or VeraCrypt compatibility is claimed.

## Current Support Matrix

| Area | Status |
| --- | --- |
| Container type | File-hosted TC/VC-compatible normal containers only |
| Encryption | AES-XTS only |
| KDF/hash | SHA-512, SHA-256, Whirlpool, BLAKE2s-256, Streebog; autoprobed in that order unless `--kdf` hint supplied |
| PIM | Default (500,000 iterations) and custom (`--pim N` → 15,000 + N×1,000 iterations) |
| Keyfiles | Not supported |
| Hidden volumes | Not supported |
| Partition/system volumes | Not supported |
| Sector size | 512-byte sectors only |
| Filesystems | FAT16/FAT32-like layouts; exFAT; NTFS |
| FAT names | Long names (LFN) with spaces, Unicode, and emoji; 8.3 short names as fallback |
| LFN | Supported (long names, UTF-16LE, surrogate pairs, combining characters) |
| FAT metadata | Supported (attributes, creation/modification/accessed timestamps) |
| exFAT | Supported, read-only; listing and single-file extraction |
| exFAT names | Long names, UTF-16LE, surrogate pairs, combining characters (no NFC normalization) |
| exFAT metadata | Supported (attributes, timestamps; UTC offset not applied) |
| TC/VC + NTFS | Supported, read-only; listing and single-file extraction |
| NTFS names | Unicode filenames, emoji, surrogate pairs, and decomposed combining characters supported (no NFC normalization) |
| NTFS metadata | Supported (attributes, size, created/modified/accessed timestamps) |
| Directory listing | Supported for FAT, exFAT, and NTFS; `--long` shows metadata |
| Single-file extraction | Supported for FAT, exFAT, and NTFS; long-name paths accepted; streamed in 256 KiB chunks (see [streaming-extraction.md](streaming-extraction.md)) |
| Directory extraction | Not supported |
| Other volume formats | Not supported |

## Commands

`info` reports file metadata and TC/VC header candidate locations without decrypting. `test-open` attempts all supported KDF profiles by autoprobe and reports the matched KDF/hash and PIM state on success. `probe-fs` opens the volume and reports a conservative first-sector filesystem candidate. `ls` and `ls --long` require the supported TC/VC profile and list files with long names and optional metadata. `extract` accepts long-name source paths. All commands that open a volume accept `--pim N` and `--kdf <name>` options.

`info`, `test-open`, `ls`, and `extract` are implemented on top of the shared `cryptovol-app` crate (see [architecture.md](architecture.md)); the format support described in this document is unchanged by that internal refactor. The desktop GUI (`apps/cryptovol-gui`, see [gui-mvp.md](gui-mvp.md)) is built on the same `cryptovol-app` crate and therefore supports exactly the same containers/filesystems described in this document — no separate format support surface.

## Test Fixtures

Generated containers under `testdata/generated/` remain ignored. The committed static fixtures under `testdata/static/` are public test data and are documented in [test-containers.md](test-containers.md).
