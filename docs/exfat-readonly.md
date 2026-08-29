# exFAT Read-Only Support

`cryptovol` supports read-only directory listing and single-file extraction from exFAT-formatted volumes that are hosted inside a TC/VC-compatible encrypted container.

## Scope

| Capability | Status |
|---|---|
| Boot sector parsing | Supported |
| Cluster mapping (FAT-chained and contiguous) | Supported |
| Directory listing (`ls`, `ls --long`) | Supported |
| Single-file extraction (`extract`) | Supported |
| Directory extraction | Not supported |
| Write operations | Not supported |
| Deleted file recovery | Not supported |
| TexFAT transactional recovery | Not supported |
| Symlink / reparse point semantics | Not supported |
| Full Windows upcase-table casefolding | Not supported |

## Boot Sector Fields Parsed

All fields are read from the first 512 bytes at logical offset 0 of the decrypted volume.

| Offset | Type | Field | Validation |
|--------|------|-------|-----------|
| 3–10 | 8 bytes ASCII | OEM name | Must be `EXFAT   ` (exactly, with trailing spaces) |
| 40 | u64 LE | `VolumeLength` (sectors) | Must be > 0 |
| 48 | u32 LE | `FatOffset` (sectors) | FAT region must not exceed VolumeLength |
| 52 | u32 LE | `FatLength` (sectors) | FAT region end = FatOffset + FatLength; must not overflow or exceed VolumeLength |
| 56 | u32 LE | `ClusterHeapOffset` (sectors) | Cluster heap must not exceed VolumeLength |
| 60 | u32 LE | `ClusterCount` | Must be > 0 |
| 64 | u32 LE | `FirstClusterOfRootDirectory` | Must be >= 2 |
| 108 | u8 | `BytesPerSectorShift` | Must be 9–12 (512–4096 bytes per sector) |
| 109 | u8 | `SectorsPerClusterShift` | Must satisfy BytesPerSectorShift + SectorsPerClusterShift <= 25 |
| 110 | u8 | `NumberOfFats` | Parsed; currently only the first FAT is used |

Derived fields computed at parse time:

- `bytes_per_sector` = 1 << `BytesPerSectorShift`
- `cluster_size` = `bytes_per_sector` << `SectorsPerClusterShift`
- `cluster_heap_byte_offset` = `ClusterHeapOffset` × `bytes_per_sector`
- `fat_byte_offset` = `FatOffset` × `bytes_per_sector`

All arithmetic is checked; out-of-range values return `ExfatError::InvalidBootSector`.

## Cluster Mapping

Cluster numbers start at 2. Cluster N maps to the following byte offset in the volume:

```
byte_offset(N) = cluster_heap_byte_offset + (N - 2) × cluster_size
```

Cluster numbers < 2 and clusters beyond the volume bounds return `ExfatError::InvalidClusterNumber` or `ExfatError::OutOfBoundsRead`.

## Allocation Modes

exFAT supports two cluster allocation modes per file or directory:

### FAT-Chained (NoFatChain = 0)

The FAT table at `fat_byte_offset` contains one u32 LE entry per cluster (including the first two reserved entries). Each entry for a data cluster either:

- Points to the next cluster in the chain (value 2 to ClusterCount+1)
- Is the end-of-chain marker `0xFFFFFF8`–`0xFFFFFFFF`

Reading stops on the first end-of-chain entry. Cycles are detected with a cap of 256 clusters per chain; exceeding this returns `ExfatError::FatChainCycle`.

### Contiguous (NoFatChain = 1)

Clusters are allocated sequentially starting from `FirstCluster`. No FAT lookup is performed. The number of bytes to read comes from the `DataLength` field of the Stream Extension entry.

Directories created by common exFAT formatters use this mode. Contiguous directories may have `ValidDataLength = 0` even when `DataLength` is non-zero; the implementation uses `max(ValidDataLength, DataLength)` for directories to avoid reading zero bytes.

## Directory Entry Set Parsing

Each file or directory is represented by a contiguous sequence of 32-byte directory entries:

1. **File Directory Entry** (type `0x85`): attributes (`u16 LE` at offset 4), secondary count, timestamps.
2. **Stream Extension** (type `0xC0`): `GeneralSecondaryFlags` (offset 1), `ValidDataLength` (`u64 LE`, offset 8), `FirstCluster` (`u32 LE`, offset 20), `DataLength` (`u64 LE`, offset 24).
3. **File Name Entry** (type `0xC1`): 15 UTF-16LE code units per entry; chained until all characters are consumed.

Entries with the "in use" bit cleared (bit 6 of the type byte) are skipped. End-of-directory is signaled by a type byte of 0x00.

The `NoFatChain` flag is bit 1 of `GeneralSecondaryFlags` in the Stream Extension.

## Unicode Filename Handling

Filenames are stored as UTF-16LE across one or more File Name entries (15 code units each). The implementation:

- Concatenates all File Name entry code unit sequences.
- Decodes the resulting UTF-16LE sequence using Rust's standard surrogate-pair handling.
- Preserves the exact on-disk code-unit sequence; no NFC/NFD normalization is applied.
- Surrogate pairs (e.g. emoji such as 🚀 U+1F680) are decoded correctly.
- Decomposed combining characters (e.g. `a` + combining diaeresis for `ä`) are preserved as stored; the precomposed NFC form (U+00E4) is a different code-unit sequence and will not match.

Invalid UTF-16 sequences return `ExfatError::InvalidUtf16`.

## Metadata Fields

Each `ExfatEntry` exposes:

| Field | Source |
|---|---|
| `name` | File Name entries, UTF-16LE decoded |
| `is_dir` | Bit 4 of `FileAttributes` in File Directory Entry |
| `size` | `ValidDataLength` from Stream Extension (files); `max(ValidDataLength, DataLength)` (directories) |
| `attributes` | `FileAttributes` bits: `read_only`, `hidden`, `system`, `directory`, `archive` |
| `created` | `CreateTimestamp` at offset 8 of File Directory Entry |
| `modified` | `LastModifiedTimestamp` at offset 12 |
| `accessed` | `LastAccessedTimestamp` at offset 16 |

## Timestamp Limitations

exFAT uses DOS-style packed timestamps (year/month/day/hour/minute/double-second in a `u32`). Each field also has an optional `UtcOffset` byte. The current implementation:

- Decodes the packed timestamp into year, month, day, hour, minute, second fields.
- Does **not** apply the UTC offset byte.
- Does **not** assume UTC or local timezone.
- Timestamps are reported as-is from the on-disk fields.

Do not rely on timestamps for timezone-aware comparisons.

## Path Lookup

`list_dir` and `read_file` accept `/`-separated paths. Lookup rules:

1. Leading `/` is stripped; an empty path after stripping returns the root entry.
2. Each segment is compared to directory entry names using `eq_ignore_ascii_case` for ASCII-only names.
3. Case-preserving exact match takes precedence before the ASCII case-fold fallback.
4. Full Windows upcase-table casefolding (required by the exFAT specification for non-ASCII names) is **not** implemented. Non-ASCII names must be provided with the exact on-disk casing.
5. Unicode normalization-based lookup is **not** implemented; decomposed and precomposed forms of the same character are treated as distinct paths.

## Static exFAT Fixture

A committed fixture exercises the full open → probe → list → extract pipeline:

```text
Path:       testdata/static/tcvc-aes-sha512-exfat-lfn-unicode.hc
Password:   test-password
PIM:        default (0)
Cipher:     AES-XTS
KDF/hash:   SHA-512
Filesystem: exFAT
Contents:   files with long names, spaces, emoji, combining characters, JPEG binary, nested directory
```

Ground truth originals for extraction verification are under `testdata/static/fs-fat-lfn-original/`.

Env var to activate fixture tests:

```text
CRYPTOVOL_STATIC_EXFAT_LFN_FIXTURE
```

Must be set to an absolute path to the `.hc` file.

### Running fixture tests

```bash
CRYPTOVOL_STATIC_EXFAT_LFN_FIXTURE=$(pwd)/testdata/static/tcvc-aes-sha512-exfat-lfn-unicode.hc \
  cargo test -p cryptovol-fs-exfat -- --ignored
```

Normal `cargo test` skips all `#[ignore]` tests and does not require the fixture.

## Streaming Extraction

Both contiguous and FAT-chained files are extracted via streaming — the full file is never buffered in RAM. A fixed 256 KiB read buffer is used for cluster reads. A temp file is created in the destination directory, data is written into it, and on success it is atomically renamed to the final path.

For FAT-chained files, the exFAT FAT table is traversed one entry at a time per cluster (on-demand metadata); it is not preloaded in full. On very large volumes the FAT table can be proportional to total capacity; a future optimization could cache a sliding FAT-entry window. For contiguous files no FAT lookup is needed — sequential byte offsets are computed directly.

See [streaming-extraction.md](streaming-extraction.md) for the full memory model.

## Known Limitations

- Entry-set checksum (`SetChecksum` at bytes 2–3 of the File Directory Entry) is not validated. The field is currently ignored. Implement validation using the algorithm in the exFAT specification before production use.
- Only the first FAT table is used. Volumes with `NumberOfFats = 2` (TexFAT) are read using the primary FAT; no transactional recovery is attempted.
- Only 512-byte sectors are tested. The implementation supports the sector-shift range 9–12 but has only been exercised with 512-byte sectors.
- Directory extraction is not supported. `read_file` returns `ExfatError::AttemptedDirectoryExtraction` on directory paths.
