# NTFS Read-Only Support

## MVP Scope

NTFS support is read-only and intentionally narrow. The MVP supports filesystem detection with
`probe-fs`, directory listing with `ls` and `ls --long`, and single-file extraction with `extract`
after a supported TC/VC container is opened.

The NTFS reader never writes to the source container, never mounts the volume, and does not expose
directory extraction or filesystem repair behavior.

## Boot Sector Fields

The NTFS boot sector parser validates and reads these fields:

* OEM ID: must be `NTFS    `.
* `BytesPerSector`.
* `SectorsPerCluster`.
* 64-bit `TotalSectors`.
* `MftLcn`.
* `MftMirrorLcn`.
* `ClustersPerFileRecordSegment`, using NTFS signed size encoding.
* `ClustersPerIndexBuffer`, using the same signed size encoding.
* `VolumeSerialNumber`.

The parser rejects malformed geometry, out-of-range MFT locations, and arithmetic overflow with
structured NTFS errors.

## Cluster and MFT Mapping

NTFS logical cluster numbers are mapped with checked arithmetic:

```text
cluster_size = BytesPerSector x SectorsPerCluster
MFT_start = MftLcn x cluster_size
MFT_record_N = MFT_start + N x file_record_size
```

`file_record_size` is decoded from `ClustersPerFileRecordSegment`:

```text
positive value -> value x cluster_size
negative value -> 2^abs(value)
```

Index buffer size uses the same signed NTFS encoding.

## FILE Record Fixup Validation

NTFS FILE records protect sector tails with an Update Sequence Array (USA). The last two bytes of
each sector in the record are replaced on disk with an Update Sequence Number (USN), while the
original two-byte sector tails are stored in the USA.

`cryptovol` validates that every protected sector tail matches the USN before restoring the original
bytes. A mismatch returns `FixupValidationFailed` instead of attempting to parse a corrupted record.

## Attribute Parsing

The attribute walker starts at the record's first attribute offset, advances by each bounded
attribute length, and stops at the `0xFFFFFFFF` end-of-list marker.

Supported attribute types for the NTFS MVP are:

* `$STANDARD_INFORMATION` (`0x10`).
* `$FILE_NAME` (`0x30`).
* Unnamed `$DATA` (`0x80`).
* `$INDEX_ROOT` (`0x90`).
* `$INDEX_ALLOCATION` (`0xA0`) for larger directory indexes when present.
* `$BITMAP` (`0xB0`) for filtering allocated index buffers when needed.

Resident attributes store their value inline in the record. Non-resident attributes store their
value in clusters described by the runlist in the attribute header.

## Runlist Decoding

Each non-resident run is encoded as:

```text
header byte:
  high nibble = LCN delta byte count
  low nibble = length byte count
length bytes:
  little-endian unsigned cluster count
LCN delta bytes:
  little-endian signed delta from previous absolute LCN
```

The decoder accumulates signed relative LCN deltas into absolute LCNs. `0x00` terminates the
runlist. Truncated runs, negative absolute LCNs, and checked-arithmetic failures return structured
errors.

## Resident vs Non-Resident $DATA

Resident unnamed `$DATA` is read from the attribute body and bounded by the resident value length. Because MFT records are typically 1–4 KiB, this is a bounded in-memory copy.

Non-resident unnamed `$DATA` is streamed run-by-run: the runlist is decoded into (LCN, cluster-count) pairs, and each run is read in 256 KiB chunks (`EXTRACTION_CHUNK_SIZE`) without allocating a Vec proportional to file size. A temp file is created in the destination directory, streamed data is written into it, and on success the temp file is atomically renamed to the final path. Compressed, sparse, encrypted, and named `$DATA` streams are unsupported in this milestone and fail cleanly.

See [streaming-extraction.md](streaming-extraction.md) for the full memory model.

## Directory Index

Directory listing reads `$INDEX_ROOT` (`0x90`) entries. Each index entry embeds a `$FILE_NAME` value
with parent reference, timestamps, attributes, allocation size, real file size, filename namespace,
and UTF-16LE filename.

The last-entry flag terminates resident index walking. For larger directories, `$INDEX_ALLOCATION`
buffers are read when present, INDX fixups are validated, and `$BITMAP` is consulted to avoid unused
index buffers.

## Unicode Policy

NTFS filenames are stored as UTF-16LE. `cryptovol` decodes them through Rust UTF-16 decoding, so BMP
characters, surrogate pairs, and emoji are handled. Invalid UTF-16 returns `InvalidUtf16`.

No Unicode normalization is applied. Decomposed combining-character sequences are preserved exactly
as they appear on disk; the reader does not silently convert names to NFC or NFD.

## Filename Namespace Preference

NTFS can store filenames in these namespaces:

* POSIX (`0`).
* Win32 (`1`).
* DOS (`2`).
* Win32AndDos (`3`).

When multiple names are available, `cryptovol` prefers Win32, POSIX, or Win32AndDos names over
DOS-only 8.3 names. DOS names remain a fallback only when no better name exists.

## Metadata Fields

`NtfsEntry` exposes:

* `name: String`.
* `is_dir: bool`.
* `size: u64`.
* `attributes`: read-only, hidden, system, directory, and archive flags.
* `created`, `modified`, and `accessed`: optional NTFS timestamps.

## Timestamp Conversion

NTFS timestamps are 100-nanosecond intervals since `1601-01-01 00:00:00 UTC`.
`cryptovol` converts nonzero ticks to Unix epoch seconds (`i64`) by subtracting the Windows-to-Unix
epoch offset and dividing by 10,000,000. Zero NTFS ticks produce `None`.

CLI long listing displays timestamps as `YYYY-MM-DD HH:MM` in UTC. No local timezone conversion is
applied.

## Path Lookup Limitations

Container paths use `/` separators. Exact case-preserving path lookup is the primary behavior.
ASCII case-insensitive matching is available as a convenience.

Full Windows-compatible Unicode casefolding is not implemented. Unicode normalization based lookup
is not implemented, so NFC and decomposed NFD-like forms are distinct unless they are exactly the
same on disk.

## Static NTFS Fixture

The static NTFS fixture is:

```text
Path: testdata/static/tcvc-aes-sha512-ntfs-lfn-unicode.hc
Password: test-password
Cipher: AES-XTS
Hash/KDF: SHA-512
PIM: default / 0
Size: 20 MiB
Filesystem: NTFS
Ground truth: testdata/static/fs-fat-lfn-original/
Env var: CRYPTOVOL_STATIC_NTFS_LFN_FIXTURE
```

The fixture contains long filenames with spaces, emoji, and decomposed combining characters. File
contents match the existing FAT/exFAT Unicode fixture ground truth.

## How to Run Fixture Tests

Run the ignored NTFS fixture tests with:

```bash
CRYPTOVOL_STATIC_NTFS_LFN_FIXTURE=$(pwd)/testdata/static/tcvc-aes-sha512-ntfs-lfn-unicode.hc \
  cargo test -p cryptovol-fs-ntfs ntfs_fixture -- --ignored
```

Normal `cargo test` skips the ignored fixture tests and does not require the fixture path.

## Unsupported Features

The NTFS MVP does not support write operations, directory extraction, Alternate Data Streams
(named `$DATA`), compressed files, sparse files, Encrypted File System (EFS), reparse points,
symlinks, junctions, ACL or security descriptor parsing, object IDs, quotas, USN journal,
transaction logs, deleted-file recovery, full repair or recovery, `$ATTRIBUTE_LIST` spanning
additional records, full Windows Unicode casefolding, hardlink semantics beyond filename
preference, or junction/symlink target interpretation.
