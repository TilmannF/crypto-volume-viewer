# TC/VC Decrypted Data Reader

This document describes the design, offset mapping, cryptographic implementation, and filesystem probing behavior for the TrueCrypt/VeraCrypt-compatible (`tcvc`) decrypted data reader.

## Purpose

The `TcvcDataReader` implements the `BlockReader` trait to expose a read-only decrypted view of a volume's logical data area. This layer abstracts away the AES-XTS block decryption, enabling higher-level filesystem parsers to treat the decrypted area as a standard flat random-access block device.

## Preferred Open API

Use `open_with_options` to open a volume. It returns a `TcvcMatchedOpenedVolume` which carries both the opened volume data and the matched profile metadata (KDF, PIM state, and which header candidate matched).

```rust
let matched = open_with_options(&reader, &opts)?;
let profile = matched.matched_profile();  // TcvcMatchedProfile
let data_reader = matched.data_reader(&reader)?;  // TcvcDataReader
```

`TcvcMatchedProfile` exposes:

* `kdf: TcvcKdf` — which KDF/hash profile matched (e.g. `TcvcKdf::Sha256`)
* `pim: PimState` — `Default` (500,000 iterations) or `Custom(n)`
* `header_role: HeaderCandidateRole` — `Primary` or `Backup`

`open_aes_sha512_header` and `open_aes_sha512_volume` are retained for legacy tests and the `tcvc-aes-sha512-basic` fixture path, but `open_with_options` is the preferred API for all new code.

## Offset Mapping: Logical vs. Physical

Physical container files contain header structures, salt, and metadata at the beginning of the file. The logical data reader maps virtual offsets to the decrypted data area:

* **Physical Offset**: The actual byte position in the encrypted container on disk.
* **Logical Offset**: The byte position relative to the start of the decrypted data area.

$$\text{Physical Offset} = \text{Logical Offset} + \text{Data Offset}$$

For the supported `tcvc-aes-sha512-basic` fixture profile, the `Data Offset` is physically located at byte `131,072` (128 KiB). Therefore:
* Logical offset `0` maps to physical container offset `131,072`.
* XTS data unit (tweak) index `0` corresponds to logical sector `0` (bytes 0–511).

## Cryptographic Pipeline & Sector-Aligned Decryption

Volume data is encrypted using **AES-256-XTS** with 512-byte sectors:

1. **Whole-Sector Decryption**: AES-XTS requires decrypting complete 512-byte blocks. XTS tweak calculations use the logical sector index.
2. **Buffer Management**: The data reader handles arbitrary, unaligned read requests by:
   * Determining the sector range containing the requested logical offsets.
   * Decrypting the complete enclosing sectors into a temporary buffer.
   * Slicing the buffer to return only the requested range.
3. **Secret Zeroization**: Temporary buffers, decrypted sector states, and KDF keys are zeroized immediately after use where practical using the `zeroize` crate.

## Partial and Edge-Case Read Behavior

The `TcvcDataReader` enforces strict boundaries to protect memory safety and guarantee robust API behavior:

* **Aligned Reads**: Reads aligned to 512-byte boundaries are decrypted directly.
* **Unaligned & Sector-Crossing Reads**: Any request crossing sector boundaries or starting at an unaligned offset automatically decrypts the enclosing sectors.
* **Zero-Length Reads**: Reading zero bytes is permitted at any valid logical offset up to the length of the data area.
* **Near-EOF Reads**: Reading a range that ends exactly at the data area length is allowed.
* **Out-of-Bounds**: Any attempt to read beyond the logical length returns a clean `OutOfBounds` error.

## The `probe-fs` Command & Filesystem Probe Limits

The `cryptovol probe-fs` command is a diagnostic helper designed to verify that data decryption is working correctly and to inspect the filesystem structure of the first decrypted sector.

> [!IMPORTANT]
> **`probe-fs` is not filesystem support.** It does not parse FAT tables, walk directories, or navigate cluster chains.

### Signature Detection
The probe reads exactly the first decrypted 512-byte sector (logical offset `0`) and looks for conservative boot sector signatures:
* **exFAT**: OEM name `EXFAT   ` (exactly 8 bytes, with trailing spaces) at bytes 3–10. Checked first, before FAT-like heuristics.
* **NTFS**: OEM name `NTFS    ` (exactly 8 bytes, with trailing spaces) at bytes 3–10. Checked after exFAT and before FAT-like heuristics.
* **FAT-like**: Plausible jump instructions (`0xEB` or `0xE9`), valid OEM names (e.g., `MSDOS5.0`), and the standard boot sector signature `0x55AA` at bytes 510–511.
* **Unknown**: Returned if the sector is successfully decrypted but does not match either signature.

## FAT, exFAT, and NTFS Directory Listing

The decrypted data reader exposed by `TcvcOpenedVolume::data_reader` is consumed by `cryptovol-fs-fat`, `cryptovol-fs-exfat`, and `cryptovol-fs-ntfs` for directory listing and file extraction. These filesystem readers take a `BlockReader` backed by the `TcvcDataReader`, enabling read-only access without mounting or FUSE. The CLI probes the first decrypted sector and dispatches to the appropriate reader: exFAT if the OEM name is `EXFAT   `, NTFS if the OEM name is `NTFS    `, FAT for FAT-like boot sectors, and unknown otherwise.

See [fat-directory-listing.md](fat-directory-listing.md) for FAT scope and limitations. See [exfat-readonly.md](exfat-readonly.md) for exFAT scope and limitations. See [ntfs-readonly.md](ntfs-readonly.md) for NTFS scope and limitations.

## Security Constraints and Limitations

* **No Decrypted Dump**: There is no command or debug interface to print raw decrypted sectors or write the decrypted block device to disk.
* **Key Preservation**: Primary and secondary AES-XTS master keys are maintained privately inside the `TcvcOpenedVolume` handle and are never exposed via standard Display, Debug, or logs.
* **Ignored Containers**: Real test containers (`*.hc`, `*.tc`) are git-ignored in the `testdata/generated/` folder.
* **Compatibility Scope**: The current implementation supports only the `tcvc-aes-sha512-basic` profile. It does not imply broad compatibility with arbitrary VeraCrypt or TrueCrypt containers.
