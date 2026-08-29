# FAT Long File Names, Unicode, and Metadata

This document describes the Long File Name (LFN) support, UTF-16LE decoding, Unicode normalization
policy, metadata fields, and path lookup behaviour implemented in `cryptovol-fs-fat`.

## FAT LFN Support

### LFN entry format

FAT LFN entries are ordinary 32-byte directory entries with the attribute byte set to `0x0F`
(`READ_ONLY | HIDDEN | SYSTEM | VOLUME_LABEL`). Each LFN entry holds up to 13 UTF-16LE code units
spread across three fields:

| Offset | Length | Content |
|--------|--------|---------|
| 1      | 10     | Name chars 1–5 (UTF-16LE) |
| 14     | 12     | Name chars 6–10 (UTF-16LE) |
| 28     | 4      | Name chars 11–12 (UTF-16LE) |

Byte `[0]` is the sequence number. The highest-sequence entry (furthest from the associated short
entry) carries the `LAST_LONG_ENTRY` flag (`0x40` OR-ed with the sequence number). Byte `[13]` is a
checksum of the associated 8.3 short name. Unused code-unit slots are padded with `0xFFFF`.

### Checksum algorithm

The LFN checksum is computed over the 11-byte 8.3 name (8 name bytes + 3 extension bytes, no dot):

```
acc = 0u8
for each byte in name11:
    acc = acc.rotate_right(1).wrapping_add(byte)
```

Every collected LFN entry must carry the same checksum value. A mismatch causes a fallback to the
8.3 short name.

### Collection and reconstruction

When the parser walks a directory sector:

1. LFN entries (attribute `0x0F`) are accumulated in a pending buffer in encounter order.
2. The `LAST_LONG_ENTRY` flag (`seq & 0x40 != 0`) marks the first entry encountered (highest
   sequence number); the final LFN entry before the short entry has sequence number `1`.
3. When a non-LFN, non-deleted entry is reached, the collected buffer is processed:
   - The 11-byte 8.3 name is extracted from the short entry and its checksum is computed.
   - Every LFN entry's checksum field must match. Any mismatch discards the buffer.
   - Entries are reassembled in correct name order: highest sequence number provides first characters.
   - The concatenated UTF-16LE bytes are decoded via `decode_utf16le_name`.
4. The reconstructed string is stored in `InternalEntry.name`; the short name remains in
   `InternalEntry.short_name`.

### Short-name fallback

Short names are used in place of a long name when:

- No LFN entries precede the short entry (short-name-only files).
- Any LFN checksum mismatches the computed short-name checksum.
- An orphaned or incomplete LFN sequence is encountered (e.g. a new LFN sequence starts before a
  short entry closes the previous one).
- The reconstructed UTF-16LE contains only invalid code units (all replaced by `U+FFFD`).

8.3 short names are always available alongside long names. The short name is formatted as
`BASENAME.EXT` (dot inserted unless the extension field is all spaces).

## UTF-16LE Decoding

LFN names are stored on disk as UTF-16LE code-unit sequences. The decoder (`decode_utf16le_name`):

1. Reads the byte slice in 2-byte steps.
2. Stops at the first `0x0000` code unit (null terminator).
3. Skips `0xFFFF` code units (LFN padding).
4. Passes remaining code units through `char::decode_utf16`. On `Ok(ch)` the character is appended;
   on `Err(_)` the Unicode replacement character `U+FFFD` is appended.
5. Returns the decoded `String` without any Unicode normalization.

### Supported character ranges

| Category | Examples | Notes |
|----------|----------|-------|
| BMP — ASCII | A–Z, 0–9 | Direct code-unit mapping |
| BMP — Latin extended | ä ö ü Ä Ö Ü ß | Single code unit each (precomposed) |
| BMP — combining | `a` + `U+0308 COMBINING DIAERESIS` | Two code units per combined character |
| SMP — surrogate pairs | 🚀 U+1F680, 😅 U+1F605 | High + low surrogate pair |
| Invalid sequences | Unpaired surrogates | Replaced with `U+FFFD`; no panic |

Surrogate pairs are handled transparently by `char::decode_utf16`. The decoder does not impose any
restriction on the Unicode block.

### Invalid UTF-16

If a LFN entry contains an unpaired high surrogate, an unpaired low surrogate, or any other
sequence that `char::decode_utf16` cannot decode, the affected code unit position is replaced with
`U+FFFD REPLACEMENT CHARACTER`. The decoder never panics on malformed input.

## Unicode Normalization Policy

**No normalization is applied.** The on-disk UTF-16LE sequence is decoded and stored exactly as
found. `cryptovol-fs-fat` does not apply NFC, NFD, NFKC, or any other Unicode normalization form.

This has a practical consequence for the included fixture:

> The file `Unicode Umlaut äöü ÄÖÜ ß.txt` is stored on disk in **decomposed form**. Each umlaut
> (e.g. `ä`) is encoded as the base letter `a` (U+0061) followed by `U+0308 COMBINING DIAERESIS`.
> The NFC-precomposed form `ä` (U+00E4) is a different code-unit sequence.

Path lookup uses the exact on-disk byte sequence. Callers must supply the same decomposed form that
is present on disk. Supplying a precomposed NFC string for the umlaut filename will produce
`PathNotFound`.

## Metadata Fields

`DirectoryEntry` exposes the following metadata fields:

| Field | Type | Description |
|-------|------|-------------|
| `name` | `String` | Long name (or 8.3 fallback) |
| `short_name` | `String` | 8.3 short name (e.g. `PROJEC~1.TXT`) |
| `is_dir` | `bool` | True for directory entries |
| `size` | `u32` | File size in bytes (0 for directories) |
| `attributes` | `FatAttributes` | FAT attribute flags |
| `created` | `Option<FatTimestamp>` | Creation timestamp |
| `modified` | `Option<FatTimestamp>` | Last-write timestamp |
| `accessed` | `Option<FatDate>` | Last-accessed date |

### FatAttributes

```rust
pub struct FatAttributes {
    pub read_only: bool,
    pub hidden:    bool,
    pub system:    bool,
    pub directory: bool,
    pub archive:   bool,
}
```

Bit mapping: `read_only` = bit 0, `hidden` = bit 1, `system` = bit 2, `directory` = bit 4,
`archive` = bit 5.

### FatDate, FatTime, FatTimestamp

```rust
pub struct FatDate      { pub year: u16, pub month: u8, pub day: u8 }
pub struct FatTime      { pub hour: u8,  pub minute: u8, pub second: u8 }
pub struct FatTimestamp { pub date: FatDate, pub time: FatTime }
```

FAT stores dates and times in packed 16-bit fields:

- **Date** (bits 15–9 = year offset from 1980; bits 8–5 = month; bits 4–0 = day)
- **Time** (bits 15–11 = hour; bits 10–5 = minute; bits 4–0 = 2-second count, so `second = count * 2`)

A raw value of zero, or a value whose fields are out of range (month 0, day 0, hour > 23,
minute > 59, second > 59 after conversion) returns `None`. Timestamps carry no timezone — they
represent raw FAT local time, whatever the creating system's locale was. The last-write time has
two-second granularity; creation time may have finer granularity via a separate 10-ms field
(not currently parsed). Accessed time stores a date only (no time component).

## Path Lookup Behaviour

`list_dir` and `read_file` resolve path components using `entry_matches`, which checks in order:

1. **Exact long name**: `entry.name == component` — Unicode-exact, case-sensitive for non-ASCII.
2. **Case-folded long name**: `entry.name.to_lowercase() == component.to_lowercase()` — effective
   for ASCII characters in long names.
3. **ASCII case-insensitive 8.3 short name**: `entry.short_name.to_ascii_uppercase() == component.to_ascii_uppercase()`.

The first match wins. `entry_matches` never panics on any Unicode input.

### Practical implications

- Emoji and other non-ASCII characters in paths must match the exact on-disk code-unit sequence.
- Decomposed Unicode paths (e.g. `a` + combining diaeresis) must use the same decomposed form that
  is stored on disk — see the normalization policy above.
- ASCII characters in long names are case-insensitive via the case-folded comparison (step 2).
- Non-ASCII characters in long names are **not** subject to full Windows-compatible Unicode
  casefolding. Only the ASCII-safe `to_lowercase()` / `to_uppercase()` Rust methods are used.

## `ls --long` Format

`cryptovol ls <container> <path> --long` prints one line per entry in the format:

```
{type_char}{attr_chars}  {size:>8}  {date_time}  {name}
```

Where:

- `type_char` is `d` for directories, `-` for files.
- `attr_chars` is a simplified 9-character permission string (`rw-r--r--` for files, `rwxr-xr-x`
  for directories).
- `size` is right-aligned in an 8-character field.
- `date_time` is `YYYY-MM-DD HH:MM` from the modified timestamp, or `----  --:--` when
  `modified` is `None`.
- `name` is `entry.name` (long name preferred; 8.3 fallback).

Example output for a file with a timestamp:

```
-rw-r--r--        49  2026-06-28 14:31  Project Notes Final.txt
```

Example output for a directory without a modified timestamp:

```
drwxr-xr-x         0  ----  --:--  Folder With Spaces
```

## Static LFN Fixture

The committed static fixture exercises LFN and Unicode support end-to-end.

```text
Path:       testdata/static/tcvc-aes-sha512-fat-lfn-unicode.hc
Password:   test-password
Container:  VeraCrypt-compatible, AES-XTS, SHA-512 KDF, default PIM, no keyfiles, no hidden volume
Filesystem: FAT with Long File Name (LFN) entries
```

See [test-containers.md](test-containers.md) for the full file listing and SHA-256 hashes.

To run LFN fixture tests:

```bash
CRYPTOVOL_STATIC_FAT_LFN_FIXTURE=testdata/static/tcvc-aes-sha512-fat-lfn-unicode.hc \
  cargo test --test lfn_fixture -- --ignored
```

Normal `cargo test` skips all `#[ignore]` tests and does not require the fixture.

## Unsupported Features

| Feature | Status |
|---------|--------|
| exFAT | Not supported in this milestone |
| Directory extraction | Not supported; `extract` handles single files only |
| Full Windows Unicode normalization and casefolding | Not implemented |
| FAT write operations | Never supported (read-only design) |
| FAT12 fixture tests | No real-container fixture; code path exists but is not integration-tested |
